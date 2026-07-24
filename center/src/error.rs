use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub(crate) enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    Persistence,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::Persistence => StatusCode::INTERNAL_SERVER_ERROR,
        };
        status.into_response()
    }
}
