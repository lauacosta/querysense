use crate::configuration::FeatureState;
use crate::routes::Historial;
use crate::startup::AppState;
use axum::http::StatusCode;
use axum::{extract::State, Json};
use chrono::NaiveDateTime;

#[axum::debug_handler]
pub async fn get_from_db(
    State(AppState { db, cache, .. }): State<AppState>,
) -> (StatusCode, Json<Vec<Historial>>) {
    match cache {
        FeatureState::Enabled => {
            let err_handler = |err| {
                tracing::warn!("Fallo al retirar registros de la tabla historial!, {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(vec![Historial::default()]),
                )
            };

            let conn = db.lock().await;

            let mut statement = match conn.prepare("SELECT id, query, result, timestamp as \"timestamp: NaiveDateTime\" from historial") {
Ok(stmt) => stmt,
                Err(err) => return err_handler(err.to_string()),
            };
            let rows = match statement.query_map([], |row| {
                let id = row.get(0)?;
                let query = row.get(1)?;
                let result = row.get(2)?;
                let timestamp =
                    row.get::<_, String>(3)?
                        .parse::<NaiveDateTime>()
                        .map_err(|err| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(err),
                            )
                        })?;
                Ok(Historial::new(id, query, result, Some(timestamp)))
            }) {
                Ok(r) => r,
                Err(err) => return err_handler(err.to_string()),
            };

            let mut result = Vec::new();
            for row in rows {
                match row {
                    Ok(r) => result.push(r),
                    Err(err) => return err_handler(err.to_string()),
                };
            }

            tracing::info!("Se ha extraido registros para el historial exitosamente!");
            (StatusCode::OK, Json(result))
        }

        FeatureState::Disabled => {
            tracing::info!("El caché se encuentra desactivado!");
            (StatusCode::NO_CONTENT, Json(vec![Historial::default()]))
        }
    }
}
