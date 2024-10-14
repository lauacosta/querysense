use crate::{
    startup::AppState,
    templates::{Index, TneaDisplay},
};
use axum::extract::State;

#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn index(State(AppState { db, .. }): State<AppState>) -> Index {
    tracing::info!("Aca!?>?");
    let db = db.lock().await;
    let mut statement = match db.prepare(
        "select
            id,
            email,
            nombre,
            sexo,
            fecha_nacimiento,
            edad,
            provincia,
            ciudad,
            descripcion,
            estudios,
            experiencia,
            estudios_mas_recientes
        from tnea
    ",
    ) {
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::warn!("{}", err);
            return Index::default();
        }
    };

    tracing::info!("Aca!?>?2");
    let rows = match statement.query_map([], |row| {
        let id = row.get(0).unwrap_or_default();
        let email = row.get(1).unwrap_or_default();
        let nombre = row.get(2).unwrap_or_default();
        let sexo = row.get(3).unwrap_or_default();
        let fecha_nacimiento = row.get(4).unwrap_or_default();
        let edad = row.get(5).unwrap_or_default();
        let provincia = row.get(6).unwrap_or_default();
        let ciudad = row.get(7).unwrap_or_default();
        let descripcion = row.get(8).unwrap_or_default();
        let estudios = row.get(9).unwrap_or_default();
        let experiencia = row.get(10).unwrap_or_default();
        let estudios_mas_recientes = row.get(11).unwrap_or_default();

        Ok(TneaDisplay::new(
            id,
            email,
            nombre,
            sexo,
            fecha_nacimiento,
            edad,
            provincia,
            ciudad,
            descripcion,
            estudios,
            experiencia,
            estudios_mas_recientes,
        ))
    }) {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("{}", err);
            return Index::default();
        }
    };

    let mut table = Vec::new();
    for row in rows {
        match row {
            Ok(r) => table.push(r),
            Err(err) => {
                tracing::warn!("{}", err);
                return Index::default();
            }
        };
    }
    Index {
        msg: format!("Hay un total de {} resultados.", table.len()),
        table,
    }
}
