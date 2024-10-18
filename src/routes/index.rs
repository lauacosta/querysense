use crate::templates::Index;

#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn index() -> Index {
    Index {}
}
