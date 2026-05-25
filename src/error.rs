use crate::inventory::UpstreamError;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug)]
pub(crate) enum AppError {
    Database(sqlx::Error),
    Upstream(UpstreamError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Database(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::Upstream(error) => (StatusCode::BAD_GATEWAY, error.to_string()),
        };

        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}
