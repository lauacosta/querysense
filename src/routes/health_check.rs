use axum::http::StatusCode;

pub async fn health_check() -> StatusCode {
    StatusCode::NO_CONTENT
}
