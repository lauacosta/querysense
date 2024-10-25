use axum::extract::{Query, State};
use serde::Deserialize;
use tracing::instrument;
use zerocopy::IntoBytes;

use crate::{
    configuration::FeatureState,
    openai,
    startup::AppState,
    templates::{Table, TneaDisplay},
};

#[derive(Deserialize, Debug)]
pub struct Params {
    query: String,
    // filtros: Option<Vec<String>>,
}

#[axum::debug_handler]
#[instrument(name = "FTS", skip(app))]
pub async fn ft_search(Query(params): Query<Params>, State(app): State<AppState>) -> Table {
    match app.cache {
        FeatureState::Enabled => {
            todo!();
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };
    let db = app.db.lock().await;
    let mut statement = match db.prepare(
        "select
            rank, 
            email, 
            edad, 
            sexo, 
            highlight(fts_tnea, 3, '<b style=\"color: green;\">', '</b>') as template
        from fts_tnea
        where template match :query
        order by rank 
        ",
    ) {
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let rows = match statement.query_map(&[(":query", &params.query)], |row| {
        let rank: f32 = row.get(0).unwrap_or_default();
        let email: String = row.get(1).unwrap_or_default();
        let edad: usize = row.get(2).unwrap_or_default();
        let sexo: String = row.get(3).unwrap_or_default();
        let template: String = row.get(4).unwrap_or_default();

        let data = TneaDisplay::new(email, edad, sexo, template, rank);

        Ok(data)
    }) {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let mut table = Vec::new();
    for row in rows {
        match row {
            Ok(r) => table.push(r),
            Err(err) => {
                tracing::warn!("{}", err);
                return Table::default();
            }
        };
    }

    tracing::info!(
        "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}` (umbral: {})",
        params.query,
        table.len(),
        table.first().map_or_else(Default::default, |d| d.rank),
        table.last().map_or_else(Default::default, |d| d.rank),
        -1.0
    );

    match app.cache {
        FeatureState::Enabled => {
            todo!()
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };

    Table {
        msg: format!("Hay un total de {} resultados.", table.len()),
        table,
    }
}

#[axum::debug_handler]
#[instrument(name = "Busqueda semántica", skip(app))]
pub async fn semantic_search(Query(params): Query<Params>, State(app): State<AppState>) -> Table {
    match app.cache {
        FeatureState::Enabled => {
            todo!();
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };

    let client = reqwest::Client::new();
    let query_emb = openai::embed_single(params.query.clone(), &client)
        .await
        .map_err(|err| tracing::error!("{err}"))
        .expect("Fallo al crear un embedding del query");

    let db = app.db.lock().await;
    let mut statement = match db.prepare(
        "
        select 
            vec_tnea.distance,
            tnea.email,
            tnea.edad,
            tnea.sexo,
            tnea.template
        from vec_tnea
        left join tnea on tnea.id = vec_tnea.row_id
        where template_embedding match :embedding
        and k = 10
        order by distance;
        ",
    ) {
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let rows = match statement.query_map(&[(":embedding", query_emb.as_bytes())], |row| {
        let rank: f32 = row.get(0).unwrap_or_default();
        let email: String = row.get(1).unwrap_or_default();
        let edad: usize = row.get(2).unwrap_or_default();
        let sexo: String = row.get(3).unwrap_or_default();
        let template: String = row.get(4).unwrap_or_default();

        let data = TneaDisplay::new(email, edad, sexo, template, rank);

        Ok(data)
    }) {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let mut table = Vec::new();
    for row in rows {
        match row {
            Ok(r) => table.push(r),
            Err(err) => {
                tracing::warn!("{}", err);
                return Table::default();
            }
        };
    }
    table.sort_by(|a, b| b.rank.partial_cmp(&a.rank).unwrap());

    tracing::info!(
        "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}` (umbral: {})",
        params.query,
        table.len(),
        table.first().map_or_else(Default::default, |d| d.rank),
        table.last().map_or_else(Default::default, |d| d.rank),
        -1.0
    );

    match app.cache {
        FeatureState::Enabled => {
            todo!()
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };

    Table {
        msg: format!("Hay un total de {} resultados.", table.len()),
        table,
    }
}
