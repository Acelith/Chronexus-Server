#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use chronexus_server::{AppState, config::Config, routes};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr};
use tower::ServiceExt;

pub struct Fixture {
    pub _directory: tempfile::TempDir,
    pub state: AppState,
    pub app: Router,
}
impl Fixture {
    pub fn new() -> Self {
        let directory = tempfile::tempdir_in("/private/tmp")
            .or_else(|_| tempfile::tempdir())
            .unwrap();
        let config = Config::parse(HashMap::from([
            (
                "CHRONEXUS_DATABASE".into(),
                directory
                    .path()
                    .join("server.sqlite3")
                    .display()
                    .to_string(),
            ),
            (
                "CHRONEXUS_PUBLIC_ORIGIN".into(),
                "http://127.0.0.1:8080".into(),
            ),
            ("CHRONEXUS_ALLOW_LOOPBACK_HTTP".into(), "true".into()),
        ]))
        .unwrap();
        let state = AppState::new(config).unwrap();
        let app = routes::router(state.clone());
        Self {
            _directory: directory,
            state,
            app,
        }
    }
    pub async fn call(
        &self,
        method: &str,
        path: &str,
        body: Value,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        call(
            self.app.clone(),
            method,
            path,
            body,
            token,
            "127.0.0.1:8080",
            None,
        )
        .await
    }
    pub async fn account(&self, name: &str) -> Value {
        let code = self.state.database.issue_invite(None).unwrap();
        let (status,body)=self.call("POST","/api/v1/auth/redeem",json!({"inviteCode":code,"username":name,"displayName":name,"password":"synthetic-password-123","device":"test desktop"}),None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body
    }
}
pub async fn call(
    app: Router,
    method: &str,
    path: &str,
    body: Value,
    token: Option<&str>,
    host: &str,
    origin: Option<&str>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    let mut request = request
        .body(Body::from(if body.is_null() {
            String::new()
        } else {
            body.to_string()
        }))
        .unwrap();
    request.extensions_mut().insert(ConnectInfo(
        "127.0.0.1:50000".parse::<SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

/// Request IDs must be UUIDv7 so the server can verify their age.
pub fn request_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

pub mod table;
