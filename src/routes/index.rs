use crate::templates::Index;

#[tracing::instrument(name = "Sirviendo la página inicial")]
#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn index() -> Index {
    Index {}
}
