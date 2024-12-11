use axum::{
    Extension,
    extract::{Query, State},
};
use querysense_cli::Cache;
use querysense_ui::SearchResponse;
use tracing::instrument;

use crate::{routes::Params, startup::AppState};

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
