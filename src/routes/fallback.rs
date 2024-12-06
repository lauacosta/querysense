use crate::templates::Fallback;

#[tracing::instrument(name = "Ha ocurrido un error, mostrando la pantalla auxiliar")]
pub async fn fallback() -> impl axum::response::IntoResponse {
    (http::StatusCode::NOT_FOUND, Fallback)
}
