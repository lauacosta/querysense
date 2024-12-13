use axum::{Extension, extract::State};
use querysense_common::SearchResult;
use tracing::instrument;

use crate::{routes::Params, routes::SearchExtractor, startup::AppState};

#[axum::debug_handler]
#[instrument(name = "Realizando la búsqueda", skip(app, client))]
pub async fn search(
    SearchExtractor(params): SearchExtractor<Params>,
    State(app): State<AppState>,
    client: Extension<reqwest::Client>,
) -> SearchResult {
    tracing::debug!(?params);
    // let params: Params = serde_json::from_value(params)?;

    // match app.cache {
    //     Cache::Enabled => {
    //         todo!();
    //     }
    //     Cache::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    // };

    params.strategy.search(&app.db_path, &client, params).await
}
