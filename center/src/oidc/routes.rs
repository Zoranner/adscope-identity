use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use adscope_protocol::{User, UserStatus};
use adscope_store::{AuthorizationCodeRecord, OAuthClientRecord, OAuthClientType};
use axum::{
    Form, Json, Router,
    body::Bytes,
    extract::{
        DefaultBodyLimit, OriginalUri, State,
        rejection::{BytesRejection, FormRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use url::Url;

use crate::{
    auth::constant_time_eq,
    oidc::{
        IdTokenUserClaims,
        crypto::{pkce_s256, random_urlsafe, sha256_token},
        validation::{
            OIDC_BODY_LIMIT_BYTES, validate_client_id, validate_code_challenge,
            validate_code_verifier, validate_nonce, validate_redirect_uri, validate_response_mode,
            validate_scopes, validate_state,
        },
    },
    session::UserSession,
    state::AppState,
};

const PUBLIC_METADATA_CACHE_CONTROL: &str = "public, max-age=300";
const SSO_COOKIE_NAME: &str = "adscope_sso";
const CSRF_TTL_SECONDS: u64 = 300;
const AUTHORIZATION_CODE_CLEANUP_LIMIT: u64 = 100;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(openid_configuration),
        )
        .route("/oauth2/jwks", get(jwks))
        .route(
            "/oauth2/token",
            post(token)
                .fallback(token_method_not_allowed)
                .layer(DefaultBodyLimit::max(OIDC_BODY_LIMIT_BYTES)),
        )
        .route("/oauth2/userinfo", get(userinfo))
        .route(
            "/oauth2/authorize",
            get(authorize)
                .post(confirm_authorization)
                .layer(DefaultBodyLimit::max(OIDC_BODY_LIMIT_BYTES)),
        )
        .route("/api/oauth2/authorize/context", get(authorization_context))
}

async fn authorize(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    jar: CookieJar,
) -> Response {
    let request = match authorization_from_uri(&uri) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let validated = match validate_authorization(&state, request).await {
        Ok(validated) => validated,
        Err(error) => return error.into_response(),
    };
    let authorization_path = validated.request.internal_path("/oauth2/authorize");
    if let Some(session) = session_from_cookie(&jar, &state) {
        match active_user(&state, &session.employee_id).await {
            Ok(Some(_)) => {
                return no_store_redirect(&validated.request.internal_path("/authorize"));
            }
            Ok(None) => {}
            Err(error) => return error.into_response(),
        }
    }

    let mut login = Url::parse("https://adscope.invalid/login").expect("fixed login URL");
    login
        .query_pairs_mut()
        .append_pair("continue", &authorization_path);
    no_store_redirect(&format!(
        "{}?{}",
        login.path(),
        login.query().unwrap_or_default()
    ))
}

async fn authorization_context(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    jar: CookieJar,
) -> Response {
    let request = match authorization_from_uri(&uri) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    let validated = match validate_authorization(&state, request).await {
        Ok(validated) => validated,
        Err(error) => return error.into_response(),
    };
    let Some(session) = session_from_cookie(&jar, &state) else {
        return AuthorizationError::invalid_session().into_response();
    };
    let user = match active_user(&state, &session.employee_id).await {
        Ok(Some(user)) => user,
        Ok(None) => return AuthorizationError::invalid_session().into_response(),
        Err(error) => return error.into_response(),
    };
    let now = match unix_seconds() {
        Ok(now) => now,
        Err(error) => return error.into_response(),
    };
    let digest = validated.request.digest();
    let csrf_token = match state.csrf_signer.issue(
        &session.employee_id,
        &digest,
        now.saturating_add(CSRF_TTL_SECONDS),
    ) {
        Ok(token) => token,
        Err(_) => return AuthorizationError::server_error().into_response(),
    };

    no_store_json(AuthorizationContextResponse {
        client_name: validated.client.name,
        user: AuthorizationUser::from(&user),
        claims: claims_for_scopes(&user, &validated.scopes),
        csrf_token,
        authorization: validated.request,
    })
    .into_response()
}

async fn confirm_authorization(
    State(state): State<AppState>,
    jar: CookieJar,
    form: Result<Form<AuthorizationDecisionForm>, FormRejection>,
) -> Response {
    let Form(form) = match form {
        Ok(form) => form,
        Err(rejection) => {
            let status = rejection.status();
            return AuthorizationError::local(status, "invalid_request").into_response();
        }
    };
    let decision = form.decision.clone();
    let csrf_token = form.csrf_token.clone();
    let request = form.into_authorization();
    let validated = match validate_authorization(&state, request).await {
        Ok(validated) => validated,
        Err(error) => return error.into_response(),
    };
    let Some(session) = session_from_cookie(&jar, &state) else {
        return AuthorizationError::invalid_session().into_response();
    };
    match active_user(&state, &session.employee_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return AuthorizationError::invalid_session().into_response(),
        Err(error) => return error.into_response(),
    }
    let now = match unix_seconds() {
        Ok(now) => now,
        Err(error) => return error.into_response(),
    };
    if !state.csrf_signer.verify(
        &csrf_token,
        &session.employee_id,
        &validated.request.digest(),
        now,
    ) {
        return validated.redirect_error("invalid_request", true);
    }

    match decision.as_str() {
        "cancel" => validated.redirect_error("access_denied", true),
        "approve" => approve_authorization(&state, validated, session, now).await,
        _ => validated.redirect_error("invalid_request", true),
    }
}

async fn approve_authorization(
    state: &AppState,
    validated: ValidatedAuthorization,
    session: UserSession,
    now: u64,
) -> Response {
    let Ok(now_i64) = i64::try_from(now) else {
        return AuthorizationError::server_error().into_response();
    };
    if state
        .repository
        .delete_expired_authorization_codes(now_i64, AUTHORIZATION_CODE_CLEANUP_LIMIT)
        .await
        .is_err()
    {
        return AuthorizationError::server_error().into_response();
    }
    let code = match random_urlsafe(32) {
        Ok(code) => code,
        Err(_) => return AuthorizationError::server_error().into_response(),
    };
    let ttl = state.oidc.config().authorization_code_ttl().as_secs();
    let Some(expires_at) = now
        .checked_add(ttl)
        .and_then(|value| i64::try_from(value).ok())
    else {
        return AuthorizationError::server_error().into_response();
    };
    let Ok(auth_time) = i64::try_from(session.auth_time) else {
        return AuthorizationError::server_error().into_response();
    };
    let record = AuthorizationCodeRecord {
        code_hash: sha256_token(&code),
        client_id: validated.client.client_id,
        employee_id: session.employee_id,
        redirect_uri: validated.request.redirect_uri.clone(),
        scopes: validated.scopes,
        nonce: validated.request.nonce.clone(),
        code_challenge: validated.request.code_challenge.clone(),
        auth_time,
        expires_at,
    };
    if state
        .repository
        .store_authorization_code(record)
        .await
        .is_err()
    {
        return AuthorizationError::server_error().into_response();
    }

    redirect_with_pairs(
        &validated.request.redirect_uri,
        &[("code", code.as_str()), ("state", &validated.request.state)],
    )
}

async fn validate_authorization(
    state: &AppState,
    request: AuthorizationRequest,
) -> Result<ValidatedAuthorization, AuthorizationError> {
    if validate_client_id(&request.client_id).is_err() {
        return Err(AuthorizationError::invalid_request());
    }
    let client = state
        .repository
        .get_oauth_client(&request.client_id)
        .await
        .map_err(|_| AuthorizationError::server_error())?
        .filter(|client| client.enabled)
        .ok_or_else(AuthorizationError::invalid_request)?;
    if validate_redirect_uri(
        client.client_type,
        &client.redirect_uris,
        &request.redirect_uri,
        state.oidc.config().allow_insecure_web_loopback_redirects(),
    )
    .is_err()
    {
        return Err(AuthorizationError::invalid_request());
    }

    let state_is_valid = validate_state(&request.state).is_ok();
    let trusted_error = |error| {
        AuthorizationError::redirect(
            request.redirect_uri.clone(),
            state_is_valid.then(|| request.state.clone()),
            error,
        )
    };
    if !state_is_valid {
        return Err(trusted_error("invalid_request"));
    }
    if request.response_type != "code" {
        return Err(trusted_error("unsupported_response_type"));
    }
    if validate_response_mode(request.response_mode.as_deref()).is_err()
        || validate_nonce(&request.nonce).is_err()
        || validate_code_challenge(&request.code_challenge).is_err()
        || request.code_challenge_method != "S256"
        || !matches!(request.prompt.as_deref(), None | Some("none"))
    {
        return Err(trusted_error("invalid_request"));
    }
    let scopes = validate_scopes(&request.scope)
        .map_err(|_| trusted_error("invalid_scope"))?
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if scopes
        .iter()
        .any(|scope| !client.allowed_scopes.contains(scope))
    {
        return Err(trusted_error("invalid_scope"));
    }
    if request.prompt.as_deref() == Some("none") {
        return Err(trusted_error("interaction_required"));
    }

    Ok(ValidatedAuthorization {
        request,
        client,
        scopes,
    })
}

async fn openid_configuration(
    State(state): State<AppState>,
) -> (HeaderMap, Json<OpenIdConfiguration>) {
    let issuer = state.oidc.config().issuer();
    public_metadata_json(OpenIdConfiguration {
        issuer: issuer.to_string(),
        authorization_endpoint: format!("{issuer}/oauth2/authorize"),
        token_endpoint: format!("{issuer}/oauth2/token"),
        userinfo_endpoint: format!("{issuer}/oauth2/userinfo"),
        jwks_uri: format!("{issuer}/oauth2/jwks"),
        response_types_supported: ["code"],
        grant_types_supported: ["authorization_code"],
        subject_types_supported: ["public"],
        id_token_signing_alg_values_supported: ["RS256"],
        scopes_supported: ["openid", "profile", "email", "phone"],
        token_endpoint_auth_methods_supported: ["client_secret_basic", "none"],
        code_challenge_methods_supported: ["S256"],
    })
}

async fn jwks(State(state): State<AppState>) -> impl IntoResponse {
    public_metadata_json(state.oidc.jwks().clone())
}

async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    match exchange_token(&state, &headers, body).await {
        Ok(response) => token_json(response).into_response(),
        Err(error) => error.into_response(),
    }
}

async fn token_method_not_allowed() -> TokenError {
    TokenError::invalid_request(StatusCode::METHOD_NOT_ALLOWED)
}

async fn exchange_token(
    state: &AppState,
    headers: &HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<TokenResponse, TokenError> {
    if !has_form_content_type(headers) {
        return Err(TokenError::invalid_request(StatusCode::BAD_REQUEST));
    }
    let body = body.map_err(|rejection| TokenError::invalid_request(rejection.status()))?;
    let request = TokenRequest::parse(&body)?;
    let authentication = ClientAuthentication::parse(headers);
    let authenticated_client = state
        .repository
        .get_oauth_client(&request.client_id)
        .await
        .map_err(|_| TokenError::server_error())?
        .ok_or_else(TokenError::invalid_client)?;
    if let Err(error) =
        authenticate_token_client(&authenticated_client, &request.client_id, &authentication)
    {
        let now = token_now()?;
        destroy_token_code(state, &request.code, now).await?;
        return Err(error);
    }
    if request.grant_type != "authorization_code" {
        return Err(TokenError::unsupported_grant_type());
    }
    let now = token_now()?;
    let exchange = state
        .repository
        .consume_authorization_code_for_exchange(&sha256_token(&request.code), now)
        .await
        .map_err(|_| TokenError::server_error())?
        .ok_or_else(TokenError::invalid_grant)?;
    let record = exchange.code;
    let client = exchange.client.ok_or_else(TokenError::invalid_grant)?;
    if !client.enabled
        || client.client_id != authenticated_client.client_id
        || client.client_type != authenticated_client.client_type
    {
        return Err(TokenError::invalid_grant());
    }
    authenticate_token_client(&client, &request.client_id, &authentication)?;
    if record.client_id != request.client_id
        || record.redirect_uri != request.redirect_uri
        || validate_code_verifier(&request.code_verifier).is_err()
        || !constant_time_eq(
            record.code_challenge.as_bytes(),
            pkce_s256(&request.code_verifier).as_bytes(),
        )
    {
        return Err(TokenError::invalid_grant());
    }
    let scope = record.scopes.join(" ");
    if validate_scopes(&scope).is_err()
        || record
            .scopes
            .iter()
            .any(|scope| !client.allowed_scopes.contains(scope))
    {
        return Err(TokenError::invalid_grant());
    }
    let user = exchange
        .user
        .filter(|user| user.employee_id == record.employee_id && user.status == UserStatus::Active)
        .ok_or_else(TokenError::invalid_grant)?;
    let access_token = state
        .oidc
        .issue_access_token(&user.employee_id, &client.client_id, &scope)
        .map_err(|_| TokenError::server_error())?;
    let profile = record.scopes.iter().any(|scope| scope == "profile");
    let email = record.scopes.iter().any(|scope| scope == "email");
    let phone = record.scopes.iter().any(|scope| scope == "phone");
    let auth_time = u64::try_from(record.auth_time).map_err(|_| TokenError::server_error())?;
    let user_claims = IdTokenUserClaims {
        preferred_username: if profile {
            non_empty(Some(&user.username))
        } else {
            None
        },
        name: if profile {
            non_empty(Some(&user.display_name))
        } else {
            None
        },
        email: if email {
            non_empty(user.email.as_deref())
        } else {
            None
        },
        phone_number: if phone {
            non_empty(user.mobile.as_deref()).or_else(|| non_empty(user.telephone.as_deref()))
        } else {
            None
        },
    };
    let id_token = state
        .oidc
        .issue_id_token_with_user_claims(
            &user.employee_id,
            &client.client_id,
            auth_time,
            &record.nonce,
            user_claims,
        )
        .map_err(|_| TokenError::server_error())?;

    Ok(TokenResponse {
        token_type: "Bearer",
        expires_in: state.oidc.config().token_ttl().as_secs(),
        scope,
        access_token,
        id_token,
    })
}

fn token_now() -> Result<i64, TokenError> {
    let now = unix_seconds().map_err(|_| TokenError::server_error())?;
    i64::try_from(now).map_err(|_| TokenError::server_error())
}

async fn destroy_token_code(state: &AppState, code: &str, now: i64) -> Result<(), TokenError> {
    state
        .repository
        .consume_authorization_code(&sha256_token(code), now)
        .await
        .map(|_| ())
        .map_err(|_| TokenError::server_error())
}

fn has_form_content_type(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(header::CONTENT_TYPE).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<mime::Mime>().ok())
        .is_some_and(|value| {
            value.type_() == mime::APPLICATION && value.subtype() == mime::WWW_FORM_URLENCODED
        })
}

fn authenticate_token_client(
    client: &OAuthClientRecord,
    body_client_id: &str,
    authentication: &ClientAuthentication,
) -> Result<(), TokenError> {
    match client.client_type {
        OAuthClientType::Web => {
            let ClientAuthentication::Basic(credentials) = authentication else {
                return Err(TokenError::invalid_client());
            };
            let Some(expected_hash) = client.client_secret_hash.as_deref() else {
                return Err(TokenError::invalid_client());
            };
            let provided_hash = sha256_token(&credentials.secret);
            if credentials.client_id != body_client_id
                || !constant_time_eq(expected_hash.as_bytes(), provided_hash.as_bytes())
            {
                return Err(TokenError::invalid_client());
            }
        }
        OAuthClientType::Desktop => {
            if !matches!(authentication, ClientAuthentication::None)
                || client.client_secret_hash.is_some()
            {
                return Err(TokenError::invalid_client());
            }
        }
    }
    Ok(())
}

async fn userinfo(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<(HeaderMap, Json<UserInfoResponse>), OidcRouteError> {
    let token = bearer_token(&headers).ok_or(OidcRouteError::InvalidToken)?;
    let claims = state
        .oidc
        .verify_access_token(token)
        .map_err(|_| OidcRouteError::InvalidToken)?;
    let client = state
        .repository
        .get_oauth_client(&claims.client_id)
        .await
        .map_err(|_| OidcRouteError::ServerError)?
        .ok_or(OidcRouteError::InvalidToken)?;
    if !client.enabled || client.client_id != claims.client_id {
        return Err(OidcRouteError::InvalidToken);
    }
    let user = state
        .repository
        .get_user(&claims.sub)
        .await
        .map_err(|_| OidcRouteError::ServerError)?
        .ok_or(OidcRouteError::InvalidToken)?;
    if user.status != UserStatus::Active {
        return Err(OidcRouteError::InvalidToken);
    }

    let scopes = claims.scope.split(' ').collect::<Vec<_>>();
    let profile = scopes.contains(&"profile");
    let email = if scopes.contains(&"email") {
        non_empty(user.email.as_deref())
    } else {
        None
    };
    let phone_number = if scopes.contains(&"phone") {
        non_empty(user.mobile.as_deref()).or_else(|| non_empty(user.telephone.as_deref()))
    } else {
        None
    };
    Ok(no_store_json(UserInfoResponse {
        sub: user.employee_id,
        preferred_username: profile.then_some(user.username),
        name: profile.then_some(user.display_name),
        email,
        phone_number,
    }))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("Bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn authorization_from_uri(uri: &Uri) -> Result<AuthorizationRequest, AuthorizationError> {
    let query = uri
        .query()
        .ok_or_else(AuthorizationError::invalid_request)?;
    if query.len() > OIDC_BODY_LIMIT_BYTES || !has_valid_form_encoding(query.as_bytes()) {
        return Err(AuthorizationError::invalid_request());
    }
    AuthorizationRequest::parse(query)
}

fn has_valid_form_encoding(value: &[u8]) -> bool {
    value.split(|byte| *byte == b'&').all(|pair| {
        let (name, value) = match pair.iter().position(|byte| *byte == b'=') {
            Some(separator) => (&pair[..separator], &pair[separator + 1..]),
            None => (pair, &[][..]),
        };
        [name, value].into_iter().all(valid_form_component)
    })
}

fn valid_form_component(value: &[u8]) -> bool {
    let mut decoded = Vec::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        match value[index] {
            b'%' => {
                if index + 2 >= value.len()
                    || !value[index + 1].is_ascii_hexdigit()
                    || !value[index + 2].is_ascii_hexdigit()
                {
                    return false;
                }
                let high = hex_value(value[index + 1]);
                let low = hex_value(value[index + 2]);
                decoded.push((high << 4) | low);
                index += 3;
            }
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    std::str::from_utf8(&decoded).is_ok()
}

fn hex_value(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        b'A'..=b'F' => value - b'A' + 10,
        _ => unreachable!("hex value is validated before conversion"),
    }
}

fn session_from_cookie(jar: &CookieJar, state: &AppState) -> Option<UserSession> {
    jar.get(SSO_COOKIE_NAME)
        .and_then(|cookie| state.user_sessions.verify(cookie.value()))
}

async fn active_user(
    state: &AppState,
    employee_id: &str,
) -> Result<Option<User>, AuthorizationError> {
    state
        .repository
        .get_user(employee_id)
        .await
        .map(|user| user.filter(|user| user.status == UserStatus::Active))
        .map_err(|_| AuthorizationError::server_error())
}

fn claims_for_scopes(user: &User, scopes: &[String]) -> Value {
    let mut claims = Map::new();
    claims.insert("sub".to_string(), Value::String(user.employee_id.clone()));
    if scopes.iter().any(|scope| scope == "profile") {
        claims.insert(
            "preferred_username".to_string(),
            Value::String(user.username.clone()),
        );
        claims.insert("name".to_string(), Value::String(user.display_name.clone()));
    }
    if scopes.iter().any(|scope| scope == "email")
        && let Some(email) = non_empty(user.email.as_deref())
    {
        claims.insert("email".to_string(), Value::String(email));
    }
    if scopes.iter().any(|scope| scope == "phone")
        && let Some(phone) =
            non_empty(user.mobile.as_deref()).or_else(|| non_empty(user.telephone.as_deref()))
    {
        claims.insert("phone_number".to_string(), Value::String(phone));
    }
    Value::Object(claims)
}

fn unix_seconds() -> Result<u64, AuthorizationError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AuthorizationError::server_error())
}

fn redirect_with_pairs(redirect_uri: &str, pairs: &[(&str, &str)]) -> Response {
    let Ok(mut redirect) = Url::parse(redirect_uri) else {
        return AuthorizationError::server_error().into_response();
    };
    {
        let mut query = redirect.query_pairs_mut();
        for (name, value) in pairs {
            query.append_pair(name, value);
        }
    }
    no_store_redirect(redirect.as_str())
}

fn no_store_redirect(location: &str) -> Response {
    let mut response = Redirect::to(location).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationRequest {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    nonce: String,
    code_challenge: String,
    code_challenge_method: String,
    response_mode: Option<String>,
    prompt: Option<String>,
}

impl AuthorizationRequest {
    fn parse(encoded: &str) -> Result<Self, AuthorizationError> {
        let mut fields = HashMap::new();
        for (name, value) in url::form_urlencoded::parse(encoded.as_bytes()) {
            let name = name.into_owned();
            if !matches!(
                name.as_str(),
                "response_type"
                    | "client_id"
                    | "redirect_uri"
                    | "scope"
                    | "state"
                    | "nonce"
                    | "code_challenge"
                    | "code_challenge_method"
                    | "response_mode"
                    | "prompt"
            ) || fields.insert(name, value.into_owned()).is_some()
            {
                return Err(AuthorizationError::invalid_request());
            }
        }
        let mut required = |name: &str| {
            fields
                .remove(name)
                .ok_or_else(AuthorizationError::invalid_request)
        };
        Ok(Self {
            response_type: required("response_type")?,
            client_id: required("client_id")?,
            redirect_uri: required("redirect_uri")?,
            scope: required("scope")?,
            state: required("state")?,
            nonce: required("nonce")?,
            code_challenge: required("code_challenge")?,
            code_challenge_method: required("code_challenge_method")?,
            response_mode: fields.remove("response_mode"),
            prompt: fields.remove("prompt"),
        })
    }

    fn internal_path(&self, path: &str) -> String {
        let mut url = Url::parse("https://adscope.invalid").expect("fixed internal origin");
        url.set_path(path);
        self.append_query(&mut url);
        format!("{}?{}", url.path(), url.query().unwrap_or_default())
    }

    fn append_query(&self, url: &mut Url) {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", &self.response_type)
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("scope", &self.scope)
            .append_pair("state", &self.state)
            .append_pair("nonce", &self.nonce)
            .append_pair("code_challenge", &self.code_challenge)
            .append_pair("code_challenge_method", &self.code_challenge_method);
        if let Some(response_mode) = &self.response_mode {
            query.append_pair("response_mode", response_mode);
        }
        if let Some(prompt) = &self.prompt {
            query.append_pair("prompt", prompt);
        }
    }

    fn digest(&self) -> String {
        sha256_token(serde_json::to_vec(self).expect("authorization request must serialize"))
    }
}

struct TokenRequest {
    grant_type: String,
    client_id: String,
    redirect_uri: String,
    code: String,
    code_verifier: String,
}

impl TokenRequest {
    fn parse(encoded: &[u8]) -> Result<Self, TokenError> {
        if encoded.len() > OIDC_BODY_LIMIT_BYTES || !has_valid_form_encoding(encoded) {
            return Err(TokenError::invalid_request(StatusCode::BAD_REQUEST));
        }
        let encoded = std::str::from_utf8(encoded)
            .map_err(|_| TokenError::invalid_request(StatusCode::BAD_REQUEST))?;
        let mut fields = HashMap::new();
        for (name, value) in url::form_urlencoded::parse(encoded.as_bytes()) {
            let name = name.into_owned();
            if !matches!(
                name.as_str(),
                "grant_type" | "client_id" | "redirect_uri" | "code" | "code_verifier"
            ) || fields.insert(name, value.into_owned()).is_some()
            {
                return Err(TokenError::invalid_request(StatusCode::BAD_REQUEST));
            }
        }
        let mut required = |name: &str| {
            fields
                .remove(name)
                .ok_or_else(|| TokenError::invalid_request(StatusCode::BAD_REQUEST))
        };
        let request = Self {
            grant_type: required("grant_type")?,
            client_id: required("client_id")?,
            redirect_uri: required("redirect_uri")?,
            code: required("code")?,
            code_verifier: required("code_verifier")?,
        };
        if validate_client_id(&request.client_id).is_err() {
            return Err(TokenError::invalid_request(StatusCode::BAD_REQUEST));
        }
        Ok(request)
    }
}

enum ClientAuthentication {
    None,
    Basic(BasicCredentials),
    Invalid,
}

impl ClientAuthentication {
    fn parse(headers: &HeaderMap) -> Self {
        let mut values = headers.get_all(header::AUTHORIZATION).iter();
        let Some(value) = values.next() else {
            return Self::None;
        };
        if values.next().is_some() {
            return Self::Invalid;
        }
        let Ok(value) = value.to_str() else {
            return Self::Invalid;
        };
        let Some((scheme, encoded)) = value.split_once(' ') else {
            return Self::Invalid;
        };
        if !scheme.eq_ignore_ascii_case("Basic")
            || encoded.is_empty()
            || encoded.chars().any(char::is_whitespace)
        {
            return Self::Invalid;
        }
        let Ok(decoded) = STANDARD.decode(encoded) else {
            return Self::Invalid;
        };
        let Ok(decoded) = String::from_utf8(decoded) else {
            return Self::Invalid;
        };
        let Some((client_id, secret)) = decoded.split_once(':') else {
            return Self::Invalid;
        };
        if client_id.is_empty() || secret.is_empty() {
            return Self::Invalid;
        }
        Self::Basic(BasicCredentials {
            client_id: client_id.to_string(),
            secret: secret.to_string(),
        })
    }
}

struct BasicCredentials {
    client_id: String,
    secret: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationDecisionForm {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    nonce: String,
    code_challenge: String,
    code_challenge_method: String,
    response_mode: Option<String>,
    prompt: Option<String>,
    decision: String,
    csrf_token: String,
}

impl AuthorizationDecisionForm {
    fn into_authorization(self) -> AuthorizationRequest {
        AuthorizationRequest {
            response_type: self.response_type,
            client_id: self.client_id,
            redirect_uri: self.redirect_uri,
            scope: self.scope,
            state: self.state,
            nonce: self.nonce,
            code_challenge: self.code_challenge,
            code_challenge_method: self.code_challenge_method,
            response_mode: self.response_mode,
            prompt: self.prompt,
        }
    }
}

struct ValidatedAuthorization {
    request: AuthorizationRequest,
    client: OAuthClientRecord,
    scopes: Vec<String>,
}

impl ValidatedAuthorization {
    fn redirect_error(&self, error: &'static str, include_state: bool) -> Response {
        AuthorizationError::redirect(
            self.request.redirect_uri.clone(),
            include_state.then(|| self.request.state.clone()),
            error,
        )
        .into_response()
    }
}

#[derive(Serialize)]
struct AuthorizationContextResponse {
    client_name: String,
    user: AuthorizationUser,
    claims: Value,
    csrf_token: String,
    authorization: AuthorizationRequest,
}

#[derive(Serialize)]
struct AuthorizationUser {
    employee_id: String,
    username: String,
    display_name: String,
}

impl From<&User> for AuthorizationUser {
    fn from(user: &User) -> Self {
        Self {
            employee_id: user.employee_id.clone(),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
        }
    }
}

enum AuthorizationError {
    Local {
        status: StatusCode,
        error: &'static str,
    },
    Redirect {
        redirect_uri: String,
        state: Option<String>,
        error: &'static str,
    },
}

impl AuthorizationError {
    fn local(status: StatusCode, error: &'static str) -> Self {
        Self::Local { status, error }
    }

    fn invalid_request() -> Self {
        Self::local(StatusCode::BAD_REQUEST, "invalid_request")
    }

    fn invalid_session() -> Self {
        Self::local(StatusCode::UNAUTHORIZED, "invalid_session")
    }

    fn server_error() -> Self {
        Self::local(StatusCode::INTERNAL_SERVER_ERROR, "server_error")
    }

    fn redirect(redirect_uri: String, state: Option<String>, error: &'static str) -> Self {
        Self::Redirect {
            redirect_uri,
            state,
            error,
        }
    }
}

impl IntoResponse for AuthorizationError {
    fn into_response(self) -> Response {
        match self {
            Self::Local { status, error } => no_store_json(OidcErrorResponse { error })
                .into_response()
                .map_status(status),
            Self::Redirect {
                redirect_uri,
                state,
                error,
            } => {
                let mut pairs = vec![("error", error)];
                if let Some(state) = state.as_deref() {
                    pairs.push(("state", state));
                }
                redirect_with_pairs(&redirect_uri, &pairs)
            }
        }
    }
}

trait ResponseStatusExt {
    fn map_status(self, status: StatusCode) -> Self;
}

impl ResponseStatusExt for Response {
    fn map_status(mut self, status: StatusCode) -> Self {
        *self.status_mut() = status;
        self
    }
}

fn public_metadata_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(PUBLIC_METADATA_CACHE_CONTROL),
    );
    (headers, Json(value))
}

fn no_store_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (headers, Json(value))
}

fn token_json<T>(value: T) -> (HeaderMap, Json<T>) {
    let mut headers = HeaderMap::new();
    token_cache_headers(&mut headers);
    (headers, Json(value))
}

fn token_cache_headers(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

#[derive(Serialize)]
struct OpenIdConfiguration {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    jwks_uri: String,
    response_types_supported: [&'static str; 1],
    grant_types_supported: [&'static str; 1],
    subject_types_supported: [&'static str; 1],
    id_token_signing_alg_values_supported: [&'static str; 1],
    scopes_supported: [&'static str; 4],
    token_endpoint_auth_methods_supported: [&'static str; 2],
    code_challenge_methods_supported: [&'static str; 1],
}

#[derive(Serialize)]
struct UserInfoResponse {
    sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phone_number: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    token_type: &'static str,
    expires_in: u64,
    scope: String,
    access_token: String,
    id_token: String,
}

struct TokenError {
    status: StatusCode,
    error: &'static str,
    authenticate: bool,
}

impl TokenError {
    fn invalid_request(status: StatusCode) -> Self {
        Self {
            status,
            error: "invalid_request",
            authenticate: false,
        }
    }

    fn unsupported_grant_type() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "unsupported_grant_type",
            authenticate: false,
        }
    }

    fn invalid_client() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error: "invalid_client",
            authenticate: true,
        }
    }

    fn invalid_grant() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            error: "invalid_grant",
            authenticate: false,
        }
    }

    fn server_error() -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error: "server_error",
            authenticate: false,
        }
    }
}

impl IntoResponse for TokenError {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, Json(OidcErrorResponse { error: self.error })).into_response();
        token_cache_headers(response.headers_mut());
        if self.authenticate {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Basic"));
        }
        response
    }
}

#[derive(Clone, Copy)]
enum OidcRouteError {
    InvalidToken,
    ServerError,
}

impl IntoResponse for OidcRouteError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),
            Self::ServerError => (StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        };
        let mut response = (status, Json(OidcErrorResponse { error })).into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if matches!(self, Self::InvalidToken) {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Bearer error=\"invalid_token\""),
            );
        }
        response
    }
}

#[derive(Serialize)]
struct OidcErrorResponse {
    error: &'static str,
}
