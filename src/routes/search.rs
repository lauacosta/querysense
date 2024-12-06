use axum::{
    extract::{Query, State},
    Extension,
};
use tracing::instrument;

use crate::{cli::Cache, routes::Params, startup::AppState, templates::SearchResponse};

#[axum::debug_handler]
#[instrument(name = "Realizando la búsqueda", skip(app, client))]
pub async fn search(
    Query(params): Query<Params>,
    State(app): State<AppState>,
    client: Extension<reqwest::Client>,
) -> SearchResponse {
    match app.cache {
        Cache::Enabled => {
            todo!();
        }
        Cache::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };

    params.strategy.search(&app.db_path, &client, params).await
}
