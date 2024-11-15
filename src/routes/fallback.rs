use crate::templates::Fallback;

pub async fn fallback() -> impl axum::response::IntoResponse {
    (http::StatusCode::NOT_FOUND, Fallback)
}
