use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::State,
    http::{HeaderName, Request, StatusCode, header, header::CACHE_CONTROL},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use reqwest::{Client, Url, redirect::Policy};
use serde_json::json;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:18086";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicIssuer {
    origin: String,
    host: String,
}

impl PublicIssuer {
    fn parse(value: &str) -> anyhow::Result<Self> {
        let value = value.trim();
        let url = Url::parse(value).context("PUBLIC_ISSUER must be an absolute URL")?;
        if url.scheme() != "https" {
            bail!("PUBLIC_ISSUER must use HTTPS");
        }
        if url.host().is_none() {
            bail!("PUBLIC_ISSUER must include a host");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("PUBLIC_ISSUER must not include userinfo");
        }
        if url.query().is_some() {
            bail!("PUBLIC_ISSUER must not include a query");
        }
        if url.fragment().is_some() {
            bail!("PUBLIC_ISSUER must not include a fragment");
        }
        if url.path() != "/" {
            bail!("PUBLIC_ISSUER path must be '/' or empty");
        }

        let host = url
            .host_str()
            .expect("validated PUBLIC_ISSUER has a host")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if host.is_empty() {
            bail!("PUBLIC_ISSUER host must not be empty");
        }
        let mut normalized = url;
        normalized
            .set_host(Some(&host))
            .expect("validated PUBLIC_ISSUER host must remain valid");
        normalized.set_path("");
        let origin = normalized.as_str().trim_end_matches('/').to_string();
        let host = origin
            .strip_prefix("https://")
            .expect("validated PUBLIC_ISSUER uses HTTPS")
            .to_string();
        Ok(Self { origin, host })
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn host(&self) -> &str {
        &self.host
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerTls {
    Http,
    Https { cert_file: String, key_file: String },
}

fn listener_tls(
    cert_file: Option<String>,
    key_file: Option<String>,
) -> anyhow::Result<ListenerTls> {
    match (cert_file, key_file) {
        (None, None) => Ok(ListenerTls::Http),
        (Some(cert_file), Some(key_file)) => Ok(ListenerTls::Https {
            cert_file,
            key_file,
        }),
        _ => bail!("TLS_CERT_FILE and TLS_KEY_FILE must be set together"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Login,
    Admin,
}

#[derive(Clone)]
pub struct AppState {
    client: Client,
    core_url: Url,
    public_issuer_host: String,
    login_enabled: bool,
    admin_enabled: bool,
    web_root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: SocketAddr,
    pub core_url: Url,
    pub public_issuer: PublicIssuer,
    pub login_enabled: bool,
    pub admin_enabled: bool,
    pub tls: ListenerTls,
    pub web_root: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let listen_addr = std::env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| DEFAULT_LISTEN_ADDR.to_string())
            .parse()
            .context("LISTEN_ADDR must be a socket address")?;
        let core_url = std::env::var("CORE_URL").context("CORE_URL is required")?;
        let core_url = Url::parse(&core_url).context("CORE_URL must be an absolute URL")?;
        if !matches!(core_url.scheme(), "http" | "https") {
            bail!("CORE_URL must use http or https");
        }
        if core_url.path() != "/" {
            bail!("CORE_URL must not include a path");
        }
        if core_url.query().is_some() || core_url.fragment().is_some() {
            bail!("CORE_URL must not include query or fragment");
        }
        let public_issuer = std::env::var("PUBLIC_ISSUER")
            .context("PUBLIC_ISSUER is required")
            .and_then(|value| PublicIssuer::parse(&value))?;
        let tls = listener_tls(
            optional_env("TLS_CERT_FILE")?,
            optional_env("TLS_KEY_FILE")?,
        )?;

        let web_root = default_web_root();
        if !web_root.join("index.html").is_file() {
            bail!("WEB_ROOT must contain index.html: {}", web_root.display());
        }

        Ok(Self {
            listen_addr,
            core_url,
            public_issuer,
            login_enabled: parse_bool_env("LOGIN_ENABLED", true)?,
            admin_enabled: parse_bool_env("ADMIN_ENABLED", true)?,
            tls,
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
            core_url: config.core_url.clone(),
            public_issuer_host: config.public_issuer.host().to_string(),
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
            core_url: Url::parse("http://127.0.0.1:1").expect("test URL must parse"),
            public_issuer_host: "auth.example.test".to_string(),
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

pub fn is_core_path(path: &str) -> bool {
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

    if !is_core_path(&path) {
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
        .core_url
        .join(path_and_query)
        .context("failed to build upstream URL")?;

    let mut upstream = state.client.request(method, upstream_url);
    for (name, value) in request.headers() {
        if name.as_str() == "host" || is_hop_by_hop(name) {
            continue;
        }
        upstream = upstream.header(name, value);
    }
    upstream = upstream.header(header::HOST, &state.public_issuer_host);

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
                "Core 暂时无法连接，请稍后重试。",
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

fn optional_env(name: &str) -> anyhow::Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
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
        PathBuf::from("portal/static"),
        PathBuf::from("portal/web/.output/public"),
    ] {
        if candidate.join("index.html").is_file() {
            return candidate;
        }
    }

    PathBuf::from("portal/static")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, header::CONTENT_TYPE};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn public_issuer_is_normalized_and_rejects_invalid_origins() {
        assert_eq!(
            PublicIssuer::parse("https://AUTH-A.EXAMPLE.COM.:443")
                .unwrap()
                .origin(),
            "https://auth-a.example.com"
        );
        assert_eq!(
            PublicIssuer::parse("https://AUTH-A.EXAMPLE.COM.:443")
                .unwrap()
                .host(),
            "auth-a.example.com"
        );

        for value in [
            "http://auth-a.example.com",
            "https://auth-a.example.com/path",
            "https://auth-a.example.com?query=value",
            "https://auth-a.example.com#fragment",
            "https://user@auth-a.example.com",
        ] {
            assert!(PublicIssuer::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn tls_files_must_be_configured_together() {
        assert_eq!(listener_tls(None, None).unwrap(), ListenerTls::Http);
        assert_eq!(
            listener_tls(Some("cert.pem".into()), Some("key.pem".into())).unwrap(),
            ListenerTls::Https {
                cert_file: "cert.pem".into(),
                key_file: "key.pem".into(),
            }
        );
        assert!(listener_tls(Some("cert.pem".into()), None).is_err());
        assert!(listener_tls(None, Some("key.pem".into())).is_err());
    }

    fn clear_portal_env() {
        unsafe {
            for name in [
                "CORE_URL",
                "WEB_ROOT",
                "PUBLIC_ISSUER",
                "TLS_CERT_FILE",
                "TLS_KEY_FILE",
                "LOGIN_ENABLED",
                "ADMIN_ENABLED",
            ] {
                std::env::remove_var(name);
            }
        }
    }

    fn set_portal_env(web_root: &std::path::Path, public_issuer: Option<&str>) {
        unsafe {
            std::env::set_var("CORE_URL", "https://core.example.com");
            std::env::set_var("WEB_ROOT", web_root);
            match public_issuer {
                Some(value) => std::env::set_var("PUBLIC_ISSUER", value),
                None => std::env::remove_var("PUBLIC_ISSUER"),
            }
        }
    }

    #[test]
    fn config_requires_public_issuer() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_portal_env();
        let web_root = test_web_root();
        set_portal_env(&web_root, None);

        let error = Config::from_env().unwrap_err();
        assert!(error.to_string().contains("PUBLIC_ISSUER is required"));

        clear_portal_env();
        std::fs::remove_dir_all(web_root).ok();
    }

    #[test]
    fn config_reads_public_issuer_and_listener_mode() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_portal_env();
        let web_root = test_web_root();
        set_portal_env(&web_root, Some("https://AUTH-A.EXAMPLE.COM.:443"));

        let config = Config::from_env().unwrap();
        assert_eq!(config.public_issuer.origin(), "https://auth-a.example.com");
        assert_eq!(config.public_issuer.host(), "auth-a.example.com");
        assert_eq!(config.tls, ListenerTls::Http);

        unsafe {
            std::env::set_var("TLS_CERT_FILE", "cert.pem");
            std::env::set_var("TLS_KEY_FILE", "key.pem");
        }
        assert!(matches!(
            Config::from_env().unwrap().tls,
            ListenerTls::Https { .. }
        ));

        unsafe {
            std::env::remove_var("TLS_KEY_FILE");
        }
        assert!(Config::from_env().is_err());

        clear_portal_env();
        std::fs::remove_dir_all(web_root).ok();
    }

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
            "/api/syncer/sync",
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
    fn classifies_core_paths() {
        for path in [
            "/api",
            "/api/me",
            "/oauth2/token",
            "/oauth2/jwks",
            "/.well-known/openid-configuration",
        ] {
            assert!(is_core_path(path), "{path}");
        }
        for path in ["/", "/login", "/admin", "/_nuxt/app.js"] {
            assert!(!is_core_path(path), "{path}");
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

    async fn mock_core() -> (Url, tokio::task::JoinHandle<()>) {
        let app = Router::new().fallback(|headers: HeaderMap| async move {
            Json(json!({
                "host": headers
                    .get(header::HOST)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
            }))
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (Url::parse(&format!("http://{address}")).unwrap(), handle)
    }

    #[tokio::test]
    async fn forwarded_requests_override_client_host_with_public_issuer() {
        let (core_url, handle) = mock_core().await;
        let web_root = test_web_root();
        let config = Config {
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            core_url,
            public_issuer: PublicIssuer::parse("https://auth-a.example.com").unwrap(),
            login_enabled: true,
            admin_enabled: true,
            tls: ListenerTls::Http,
            web_root: web_root.clone(),
        };
        let response = router(AppState::new(&config).unwrap())
            .oneshot(
                Request::builder()
                    .uri("/api/me")
                    .header(header::HOST, "attacker.example.test")
                    .header("x-forwarded-host", "attacker.example.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["host"], "auth-a.example.com");

        handle.abort();
        std::fs::remove_dir_all(web_root).ok();
    }

    fn test_web_root() -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("adscope-portal-test-{suffix}"));
        std::fs::create_dir_all(root.join("_nuxt")).unwrap();
        std::fs::write(root.join("index.html"), "<main>Adscope Web Shell</main>").unwrap();
        std::fs::write(root.join("_nuxt").join("app.js"), "console.log('adscope')").unwrap();
        root
    }
}
