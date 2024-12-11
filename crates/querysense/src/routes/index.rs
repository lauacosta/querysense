use axum::extract::State;
use querysense_sqlite::get_historial;
use querysense_ui::Index;
use rusqlite::Connection;

use crate::startup::AppState;

use super::ReportError;

#[tracing::instrument(name = "Sirviendo la página inicial")]
#[axum::debug_handler]
pub async fn index(State(app): State<AppState>) -> eyre::Result<Index, ReportError> {
    let db = Connection::open(app.db_path)
        .expect("Deberia ser un path valido a una base de datos SQLite");
    let historial = get_historial(&db)?;

    Ok(Index { historial })
}
