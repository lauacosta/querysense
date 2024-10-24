pub async fn fallback() -> impl axum::response::IntoResponse {
    (
        http::StatusCode::NOT_FOUND,
        "404 Not Found. Por favor, revisa la URL.",
    )
}
