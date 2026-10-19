use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderName, Request, StatusCode, header::CACHE_CONTROL},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use reqwest::{Client, Url, redirect::Policy};
use serde_json::json;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:8443";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Login,
    Admin,
}

#[derive(Clone)]
pub struct AppState {
    client: Client,
    center_url: Url,
    login_enabled: bool,
    admin_enabled: bool,
    web_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub center_url: Url,
    pub login_enabled: bool,
    pub admin_enabled: bool,
    pub tls_cert_file: String,
    pub tls_key_file: String,
    pub web_root: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let listen_addr = std::env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| DEFAULT_LISTEN_ADDR.to_string())
            .parse()
            .context("LISTEN_ADDR must be a socket address")?;
        let center_url = std::env::var("CENTER_URL").context("CENTER_URL is required")?;
        let center_url = Url::parse(&center_url).context("CENTER_URL must be an absolute URL")?;
        if !matches!(center_url.scheme(), "http" | "https") {
            bail!("CENTER_URL must use http or https");
        }
        if center_url.path() != "/" {
            bail!("CENTER_URL must not include a path");
        }
        if center_url.query().is_some() || center_url.fragment().is_some() {
            bail!("CENTER_URL must not include query or fragment");
        }

        let web_root = default_web_root();
        if !web_root.join("index.html").is_file() {
            bail!("WEB_ROOT must contain index.html: {}", web_root.display());
        }

        Ok(Self {
            listen_addr,
            center_url,
            login_enabled: parse_bool_env("LOGIN_ENABLED", true)?,
            admin_enabled: parse_bool_env("ADMIN_ENABLED", true)?,
            tls_cert_file: std::env::var("TLS_CERT_FILE").context("TLS_CERT_FILE is required")?,
            tls_key_file: std::env::var("TLS_KEY_FILE").context("TLS_KEY_FILE is required")?,
            web_root,
        })
    }
}

impl AppState {
    pub fn new(config: &Config) -> anyhow::Result<Self> {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(90))
            .build()
            .context("failed to build upstream HTTP client")?;

        Ok(Self {
            client,
            center_url: config.center_url.clone(),
            login_enabled: config.login_enabled,
            admin_enabled: config.admin_enabled,
            web_root: config.web_root.clone(),
        })
    }

    #[cfg(test)]
    fn for_tests(login_enabled: bool, admin_enabled: bool, web_root: PathBuf) -> Self {
        Self {
            client: Client::builder()
                .redirect(Policy::none())
                .build()
                .expect("test HTTP client must build"),
            center_url: Url::parse("http://127.0.0.1:1").expect("test URL must parse"),
            login_enabled,
            admin_enabled,
            web_root,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .fallback(handle_request)
        .with_state(state)
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok\n")
}

pub fn gate_for_path(path: &str) -> Option<Gate> {
    if matches!(
        path,
        "/login"
            | "/authorize"
            | "/oauth2/authorize"
            | "/api/auth/login"
            | "/api/oauth2/authorize/context"
    ) {
        Some(Gate::Login)
    } else if path == "/admin"
        || path.starts_with("/admin/")
        || path == "/api/admin"
        || path.starts_with("/api/admin/")
    {
        Some(Gate::Admin)
    } else {
        None
    }
}

pub fn is_center_path(path: &str) -> bool {
    path == "/api"
        || path.starts_with("/api/")
        || path == "/oauth2"
        || path.starts_with("/oauth2/")
        || path == "/.well-known"
        || path.starts_with("/.well-known/")
}

async fn handle_request(State(state): State<AppState>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_string();
    if let Some(gate) = gate_for_path(&path) {
        let enabled = match gate {
            Gate::Login => state.login_enabled,
            Gate::Admin => state.admin_enabled,
        };
        if !enabled {
            return forbidden_response(gate, path.starts_with("/api/"));
        }
    }

    if !is_center_path(&path) {
        return serve_static(&state, request).await;
    }

    match forward(state, request).await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "upstream request failed");
            upstream_error_response(path.starts_with("/api/"))
        }
    }
}

async fn serve_static(state: &AppState, request: Request<Body>) -> Response {
    let service = ServeDir::new(&state.web_root)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(state.web_root.join("index.html")));
    match service.oneshot(request).await {
        Ok(response) => {
            let (parts, body) = response.into_parts();
            Response::from_parts(parts, Body::new(body))
        }
        Err(error) => {
            tracing::error!(%error, "static file request failed");
            upstream_error_response(false)
        }
    }
}

async fn forward(state: AppState, request: Request<Body>) -> anyhow::Result<Response> {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    let upstream_url = state
        .center_url
        .join(path_and_query)
        .context("failed to build upstream URL")?;

    let mut upstream = state.client.request(method, upstream_url);
    for (name, value) in request.headers() {
        if name.as_str() == "host" || is_hop_by_hop(name) {
            continue;
        }
        upstream = upstream.header(name, value);
    }

    let body = request.into_body();
    let response = upstream
        .body(reqwest::Body::wrap_stream(body.into_data_stream()))
        .send()
        .await
        .context("upstream request failed")?;

    let status = response.status();
    let headers = response.headers().clone();
    let body = Body::from_stream(response.bytes_stream());
    let mut proxied = Response::builder().status(status);
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name) {
            continue;
        }
        proxied = proxied.header(name, value);
    }
    proxied
        .body(body)
        .context("failed to build upstream response")
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn forbidden_response(gate: Gate, is_api: bool) -> Response {
    let (code, message) = match gate {
        Gate::Login => ("login_disabled", "登录入口已关闭"),
        Gate::Admin => ("admin_disabled", "管理入口已关闭"),
    };
    let mut response = if is_api {
        (
            StatusCode::FORBIDDEN,
            Json(json!({ "error": code, "message": message })),
        )
            .into_response()
    } else {
        (
            StatusCode::FORBIDDEN,
            Html(error_page("入口已关闭", message)),
        )
            .into_response()
    };
    response.headers_mut().insert(
        CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    response
}

fn upstream_error_response(is_api: bool) -> Response {
    let mut response = if is_api {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "upstream_unavailable" })),
        )
            .into_response()
    } else {
        (
            StatusCode::BAD_GATEWAY,
            Html(error_page(
                "服务暂不可用",
                "Center 暂时无法连接，请稍后重试。",
            )),
        )
            .into_response()
    };
    response.headers_mut().insert(
        CACHE_CONTROL,
        "no-store".parse().expect("static header value"),
    );
    response
}

fn error_page(title: &str, message: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; background: #f8fafc; color: #0f172a; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; }}
main {{ width: min(28rem, calc(100% - 3rem)); padding: 2rem; border: 1px solid #e2e8f0; border-radius: 1rem; background: #fff; box-shadow: 0 12px 40px rgb(15 23 42 / 8%); }}
h1 {{ margin: 0 0 .75rem; font-size: 1.4rem; }}
p {{ margin: 0; color: #475569; line-height: 1.7; }}
</style>
</head>
<body><main><h1>{title}</h1><p>{message}</p></main></body>
</html>"#
    )
}

fn parse_bool_env(name: &str, default: bool) -> anyhow::Result<bool> {
    match std::env::var(name) {
        Ok(value) => {
            parse_bool_value(&value).ok_or_else(|| anyhow::anyhow!("{name} must be true or false"))
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(error) => Err(error.into()),
    }
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn default_web_root() -> PathBuf {
    if let Ok(path) = std::env::var("WEB_ROOT") {
        return PathBuf::from(path);
    }

    for candidate in [
        PathBuf::from("web"),
        PathBuf::from("center/web/.output/public"),
    ] {
        if candidate.join("index.html").is_file() {
            return candidate;
        }
    }

    PathBuf::from("web")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::CONTENT_TYPE;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[test]
    fn classifies_login_paths() {
        for path in [
            "/login",
            "/authorize",
            "/oauth2/authorize",
            "/api/auth/login",
            "/api/oauth2/authorize/context",
        ] {
            assert_eq!(gate_for_path(path), Some(Gate::Login), "{path}");
        }
    }

    #[test]
    fn classifies_admin_paths() {
        for path in ["/admin", "/admin/users", "/api/admin", "/api/admin/users"] {
            assert_eq!(gate_for_path(path), Some(Gate::Admin), "{path}");
        }
    }

    #[test]
    fn leaves_protocol_and_user_paths_open() {
        for path in [
            "/.well-known/openid-configuration",
            "/oauth2/token",
            "/oauth2/userinfo",
            "/oauth2/jwks",
            "/api/me",
            "/api/connector/sync",
        ] {
            assert_eq!(gate_for_path(path), None, "{path}");
        }
    }

    #[test]
    fn boolean_values_are_strict() {
        assert_eq!(parse_bool_value("true"), Some(true));
        assert_eq!(parse_bool_value("false"), Some(false));
        assert_eq!(parse_bool_value("TRUE"), None);
        assert_eq!(parse_bool_value("1"), None);
    }

    #[tokio::test]
    async fn disabled_login_returns_html_403() {
        let web_root = test_web_root();
        let response = router(AppState::for_tests(false, true, web_root.clone()))
            .oneshot(
                Request::builder()
                    .uri("/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("登录入口已关闭"));
        std::fs::remove_dir_all(web_root).unwrap();
    }

    #[tokio::test]
    async fn disabled_admin_returns_json_403() {
        let web_root = test_web_root();
        let response = router(AppState::for_tests(true, false, web_root.clone()))
            .oneshot(
                Request::builder()
                    .uri("/api/admin/users")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("admin_disabled"));
        std::fs::remove_dir_all(web_root).unwrap();
    }

    #[test]
    fn classifies_center_paths() {
        for path in [
            "/api",
            "/api/me",
            "/oauth2/token",
            "/oauth2/jwks",
            "/.well-known/openid-configuration",
        ] {
            assert!(is_center_path(path), "{path}");
        }
        for path in ["/", "/login", "/admin", "/_nuxt/app.js"] {
            assert!(!is_center_path(path), "{path}");
        }
    }

    #[tokio::test]
    async fn serves_frontend_and_assets() {
        let web_root = test_web_root();
        let app = router(AppState::for_tests(true, true, web_root.clone()));

        let page = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/domains")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let body = page.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("Adscope Web Shell"));

        let asset = app
            .oneshot(
                Request::builder()
                    .uri("/_nuxt/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(asset.status(), StatusCode::OK);
        let body = asset.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("console.log('adscope')"));

        std::fs::remove_dir_all(web_root).unwrap();
    }

    fn test_web_root() -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("adscope-gateway-test-{suffix}"));
        std::fs::create_dir_all(root.join("_nuxt")).unwrap();
        std::fs::write(root.join("index.html"), "<main>Adscope Web Shell</main>").unwrap();
        std::fs::write(root.join("_nuxt").join("app.js"), "console.log('adscope')").unwrap();
        root
    }
}
