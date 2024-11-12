use axum::extract::State;

use crate::{sqlite, startup::AppState, templates::Index};

use super::ReportError;

#[tracing::instrument(name = "Sirviendo la página inicial")]
#[axum::debug_handler]
pub async fn index(State(app): State<AppState>) -> eyre::Result<Index, ReportError> {
    let db = app.db.lock().await;
    let historial = sqlite::get_historial(&db)?;

    Ok(Index { historial })
}
