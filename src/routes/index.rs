use crate::{
    startup::AppState,
    templates::{Index, TneaDisplay},
};
use axum::extract::State;

#[allow(clippy::unused_async)]
#[axum::debug_handler]
pub async fn index(State(AppState { db, .. }): State<AppState>) -> Index {
    let db = db.lock().await;
    let mut statement = match db.prepare(
        "select
            email,
            sexo,
            edad,
            template
        from tnea;
    ",
    ) {
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::warn!("{}", err);
            return Index::default();
        }
    };

    let rows = match statement.query_map([], |row| {
        let email = row.get(0).unwrap_or_default();
        let sexo = row.get(1).unwrap_or_default();
        let edad = row.get(2).unwrap_or_default();
        let template = row.get(3).unwrap_or_default();

        let data = TneaDisplay::new(email, sexo, edad, template, -1.0);

        Ok(data)
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
