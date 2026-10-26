use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
pub type Result<T> = std::result::Result<T, ApiError>;

impl ApiError {
    pub fn invalid() -> Self {
        Self(StatusCode::BAD_REQUEST, "Invalid request.")
    }
    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "Missing or invalid credentials.")
    }
    pub fn internal() -> Self {
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Storage operation failed.",
        )
    }
    pub fn limited() -> Self {
        Self(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts. Try again later.",
        )
    }
    pub fn gone() -> Self {
        Self(StatusCode::GONE, "Invitation expired or already used.")
    }
}
impl From<rusqlite::Error> for ApiError {
    fn from(_: rusqlite::Error) -> Self {
        Self::internal()
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.0,
            Json(ErrorBody {
                error: self.1.into(),
            }),
        )
            .into_response();
        if self.0 == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert("retry-after", "900".parse().unwrap());
        }
        response
    }
}
