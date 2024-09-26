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
            let conn = db;

            let result = match sqlx::query_as!(
                Historial,
                "SELECT id, query, result, timestamp as \"timestamp: NaiveDateTime\" from historial",
            )
                .fetch_all(&conn)
            .await
            {
                Ok(data) => data,
                Err(err) => {
                    tracing::warn!("Fallo al retirar registros de la tabla historial!, {}", err);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(vec![Historial::default()]),
                    );
                }
            };

            tracing::info!("Se ha extraido registros para el historial exitosamente!");
            (StatusCode::OK, Json(result))
        }

        FeatureState::Disabled => {
            tracing::info!("El caché se encuentra desactivado!");
            (StatusCode::NO_CONTENT, Json(vec![Historial::default()]))
        }
    }
}
