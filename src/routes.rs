use crate::{
    AppState,
    auth::{DeviceSession, Login, Recover, Redeem, SignedIn},
    error::{ApiError, ErrorBody, Result},
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, DefaultBodyLimit, Path, Request, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Serialize;
use std::net::{IpAddr, SocketAddr};
use utoipa::{
    OpenApi, ToSchema,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    api: &'static str,
    version: u32,
    server_version: &'static str,
    capabilities: Vec<&'static str>,
}
#[derive(Serialize, ToSchema)]
pub struct Health {
    status: &'static str,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(crate::campaign_routes::router())
        .route("/api/v1/meta", get(meta))
        .route("/api/v1/openapi.json", get(openapi))
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/redeem", post(redeem))
        .route("/api/v1/auth/recover", post(recover))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/sessions", get(sessions).delete(revoke_all))
        .route("/api/v1/auth/sessions/{sessionId}", delete(revoke_session))
        .fallback(|| async {
            ApiError(
                StatusCode::NOT_FOUND,
                "Endpoint not implemented or not found.",
            )
        })
        .layer(DefaultBodyLimit::max(8192))
        .layer(middleware::from_fn_with_state(state.clone(), security))
        .with_state(state)
}

// Trust only the socket peer. Forwarded headers from other hosts are never used.
fn client_ip(state: &AppState, peer: IpAddr, headers: &HeaderMap) -> Result<IpAddr> {
    if state.config.loopback_http {
        if !peer.is_loopback() {
            return Err(ApiError(StatusCode::FORBIDDEN, "Loopback access required."));
        }
        return Ok(peer);
    }
    if !state
        .config
        .trusted_proxies
        .iter()
        .any(|net| net.contains(&peer))
    {
        return Err(ApiError(StatusCode::FORBIDDEN, "Untrusted reverse proxy."));
    }
    if headers
        .get("x-forwarded-proto")
        .and_then(|h| h.to_str().ok())
        != Some("https")
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "HTTPS reverse proxy required.",
        ));
    }
    // The trusted proxy must overwrite, not append, the client's address.
    headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or(ApiError(
            StatusCode::BAD_REQUEST,
            "Reverse proxy must supply one client IP.",
        ))
}
async fn security(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let result = (|| {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .ok_or_else(ApiError::internal)?
            .0;
        if ["/health/live", "/health/ready"].contains(&request.uri().path())
            && peer.ip().is_loopback()
        {
            return Ok(None);
        }
        let ip = client_ip(&state, peer.ip(), request.headers())?;
        if request.headers().get("host").and_then(|h| h.to_str().ok())
            != Some(state.config.authority.as_str())
        {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Host must match the configured public origin.",
            ));
        }
        if request
            .headers()
            .get("origin")
            .is_some_and(|h| h.to_str().ok() != Some(state.config.public_origin.as_str()))
        {
            return Err(ApiError(StatusCode::FORBIDDEN, "Origin not allowed."));
        }
        if request.headers().contains_key("cookie") {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "Use Bearer authentication; cookie sessions are not supported.",
            ));
        }
        Ok(Some(ip))
    })();
    let mut response = match result {
        Ok(ip) => {
            let mut request = request;
            if let Some(ip) = ip {
                request.extensions_mut().insert(ip);
            }
            match tokio::time::timeout(std::time::Duration::from_secs(20), next.run(request)).await
            {
                Ok(reply) => reply,
                Err(_) => ApiError(
                    StatusCode::REQUEST_TIMEOUT,
                    "Request timed out; check the outcome before retrying.",
                )
                .into_response(),
            }
        }
        Err(error) => error.into_response(),
    };
    for (key, value) in [
        ("cache-control", "no-store"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        (
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ),
    ] {
        response
            .headers_mut()
            .insert(key, HeaderValue::from_static(value));
    }
    if !state.config.loopback_http {
        response.headers_mut().insert(
            "strict-transport-security",
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}
pub(crate) fn body<T>(value: std::result::Result<Json<T>, JsonRejection>) -> Result<T> {
    value.map(|Json(value)| value).map_err(|e| {
        let code = if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::BAD_REQUEST
        };
        ApiError(code, "Invalid JSON request or body too large.")
    })
}
pub(crate) fn token(headers: &HeaderMap) -> Result<String> {
    if headers.get_all("authorization").iter().count() != 1 {
        return Err(ApiError::unauthorized());
    }
    headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64)
        .map(String::from)
        .ok_or_else(ApiError::unauthorized)
}

#[utoipa::path(get,path="/api/v1/meta",responses((status=200,body=Meta)),tag="service")]
async fn meta() -> Json<Meta> {
    Json(Meta {
        api: "chronexus-sync",
        version: 1,
        server_version: env!("CARGO_PKG_VERSION"),
        capabilities: vec!["accounts", "sessions", "campaigns"],
    })
}
#[utoipa::path(get,path="/health/live",responses((status=200,body=Health)),tag="service")]
async fn live() -> Json<Health> {
    Json(Health { status: "ok" })
}
#[utoipa::path(get,path="/health/ready",responses((status=200,body=Health),(status=500,body=ErrorBody)),tag="service")]
async fn ready(State(state): State<AppState>) -> Result<Json<Health>> {
    state.blocking(|db| db.ready()).await?;
    Ok(Json(Health { status: "ready" }))
}
async fn openapi() -> Json<serde_json::Value> {
    Json(serde_json::to_value(contract()).expect("serializable OpenAPI"))
}

#[utoipa::path(post,path="/api/v1/auth/login",request_body=Login,responses((status=200,body=SignedIn),(status=400,body=ErrorBody),(status=401,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody)),tag="accounts")]
async fn login(
    State(state): State<AppState>,
    axum::Extension(ip): axum::Extension<IpAddr>,
    request: std::result::Result<Json<Login>, JsonRejection>,
) -> Result<Json<SignedIn>> {
    let request = body(request)?;
    let dummy = state.dummy_hash.clone();
    state
        .blocking(move |db| db.login(&request, &ip.to_string(), &dummy))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/auth/redeem",request_body=Redeem,responses((status=200,body=SignedIn),(status=400,body=ErrorBody),(status=409,body=ErrorBody),(status=410,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody)),tag="accounts")]
async fn redeem(
    State(state): State<AppState>,
    axum::Extension(ip): axum::Extension<IpAddr>,
    request: std::result::Result<Json<Redeem>, JsonRejection>,
) -> Result<Json<SignedIn>> {
    let request = body(request)?;
    state
        .blocking(move |db| db.redeem(&request, &ip.to_string()))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/auth/recover",request_body=Recover,responses((status=200,body=SignedIn),(status=400,body=ErrorBody),(status=410,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody)),tag="accounts")]
async fn recover(
    State(state): State<AppState>,
    axum::Extension(ip): axum::Extension<IpAddr>,
    request: std::result::Result<Json<Recover>, JsonRejection>,
) -> Result<Json<SignedIn>> {
    let request = body(request)?;
    state
        .blocking(move |db| db.recover(&request, &ip.to_string()))
        .await
        .map(Json)
}
#[utoipa::path(post,path="/api/v1/auth/logout",security(("bearer"=[])),responses((status=204),(status=401,body=ErrorBody)),tag="sessions")]
async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.revoke(&token, None, false))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(get,path="/api/v1/auth/sessions",security(("bearer"=[])),responses((status=200,body=Vec<DeviceSession>),(status=401,body=ErrorBody)),tag="sessions")]
async fn sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeviceSession>>> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.sessions(&token))
        .await
        .map(Json)
}
#[utoipa::path(delete,path="/api/v1/auth/sessions",security(("bearer"=[])),responses((status=204),(status=401,body=ErrorBody)),tag="sessions")]
async fn revoke_all(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.revoke(&token, None, true))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
#[utoipa::path(delete,path="/api/v1/auth/sessions/{sessionId}",params(("sessionId"=String,Path,description="Public session UUID, never the token")),security(("bearer"=[])),responses((status=204),(status=400,body=ErrorBody),(status=401,body=ErrorBody),(status=404,body=ErrorBody)),tag="sessions")]
async fn revoke_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let token = token(&headers)?;
    state
        .blocking(move |db| db.revoke(&token, Some(&id), false))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(OpenApi)]
#[openapi(
    paths(
        meta,
        live,
        ready,
        login,
        redeem,
        recover,
        logout,
        sessions,
        revoke_all,
        revoke_session
    ),
    components(schemas(
        Meta,
        Health,
        Login,
        Redeem,
        Recover,
        SignedIn,
        DeviceSession,
        ErrorBody
    )),
    info(
        title = "Chronexus Server accounts API",
        description = "S1-S3: accounts, sessions and campaign membership for sync protocol v1. Content sharing and browser UI pending."
    )
)]
struct Contract;
pub fn contract() -> utoipa::openapi::OpenApi {
    let mut api = Contract::openapi();
    api.merge(crate::campaign_routes::CampaignContract::openapi());
    api.components.as_mut().unwrap().add_security_scheme(
        "bearer",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
    );
    api
}
