use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub(crate) enum ApiError {
    Unauthorized,
    Forbidden,
    Persistence,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::Persistence => StatusCode::INTERNAL_SERVER_ERROR,
        };
        status.into_response()
    }
}
