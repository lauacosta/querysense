use axum::{
    extract::{Query, State},
    Extension,
};
use rusqlite::ToSql;
use serde::Deserialize;
use tracing::instrument;
use zerocopy::IntoBytes;

use crate::{
    cli::Cache,
    openai,
    routes::{ReportError, SearchStrategy},
    sqlite,
    startup::AppState,
    templates::{
        Fallback, Historial, ReRankDisplay, ResponseMarker, RrfTable, SearchResponse, Sexo, Table,
        TneaDisplay,
    },
};

#[derive(Deserialize, Debug)]
pub struct Params {
    #[serde(rename = "query")]
    search_str: String,
    strategy: SearchStrategy,
    sexo: Sexo,
    edad_min: u64,
    edad_max: u64,
    peso_fts: f32,
    peso_semantic: f32,
}

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
    let db = app.db.lock().await;

    let (query, provincia, ciudad) =
        if let Some((query, filters)) = params.search_str.split_once('|') {
            if let Some((provincia, ciudad)) = filters.split_once(',') {
                (
                    sqlite::normalize(query),
                    sqlite::normalize(provincia),
                    sqlite::normalize(ciudad),
                )
            } else {
                (sqlite::normalize(query), String::new(), String::new())
            }
        } else {
            (
                sqlite::normalize(&params.search_str),
                String::new(),
                String::new(),
            )
        };

    match params.strategy {
        SearchStrategy::Fts => {
            let mut search_query = SearchQuery::new(
                &db,
                "select
                    rank as score, 
                    email, 
                    provincia,
                    ciudad,
                    edad, 
                    sexo, 
                    highlight(fts_tnea, 5, '<b style=\"color: green;\">', '</b>') as template,
                    'fts' as match_type
                from fts_tnea
                where template match :query
                and edad between :edad_min and :edad_max
                ",
            );
            search_query.add_bindings(&[&query, &params.edad_min, &params.edad_max]);

            if !provincia.is_empty() {
                search_query.add_filter(" and provincia like :provincia", &[&provincia]);
            }
            if !ciudad.is_empty() {
                search_query.add_filter(" and ciudad like :ciudad", &[&ciudad]);
            }

            let sexo = params.sexo;
            match sexo {
                Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&sexo]),
                Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&sexo]),
                Sexo::U => (),
            };

            search_query.push_str(" order by rank");

            let table = match search_query.execute(|row| {
                let score = row.get::<_, f32>(0).unwrap_or_default() * -1.;
                let email: String = row.get(1).unwrap_or_default();
                let provincia: String = row.get(2).unwrap_or_default();
                let ciudad: String = row.get(3).unwrap_or_default();
                let edad: u64 = row.get(4).unwrap_or_default();
                let sexo: Sexo = row.get(5).unwrap_or_default();
                let template: String = row.get(6).unwrap_or_default();

                let data = TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                Ok(data)
            }) {
                Ok(rows) => rows,
                Err(response) => return response,
            };

            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                query,
                table.len(),
                table.first().map_or_else(Default::default, |d| d.score),
                table.last().map_or_else(Default::default, |d| d.score),
            );

            let historial = match update_historial(&db, &params.search_str) {
                Ok(historial) => historial,
                Err(response) => return response,
            };

            Table {
                msg: format!("Hay un total de {} resultados.", table.len()),
                table,
                historial,
            }
            .into()
        }
        SearchStrategy::Semantic => {
            let query_emb = openai::embed_single(query.to_string(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let embedding = query_emb.as_bytes();

            let mut search_query = SearchQuery::new(
                &db,
                "
                select
                    vec_tnea.distance,
                    tnea.email,
                    tnea.provincia,
                    tnea.ciudad,
                    tnea.edad,
                    tnea.sexo,
                    tnea.template,
                    'vec' as match_type
                from vec_tnea
                left join tnea on tnea.id = vec_tnea.row_id
                where template_embedding match :embedding
                and k = 1000
                and tnea.edad between :edad_min and :edad_max
                ",
            );
            search_query.add_bindings(&[&embedding, &params.edad_min, &params.edad_max]);

            if !provincia.is_empty() {
                search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
            }

            if !ciudad.is_empty() {
                search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
            }

            match params.sexo {
                Sexo::U => (),
                Sexo::F => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::F]),
                Sexo::M => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::M]),
            };

            let table = match search_query.execute(|row| {
                let score = row.get::<_, f32>(0).unwrap_or_default();
                let email: String = row.get(1).unwrap_or_default();
                let provincia: String = row.get(2).unwrap_or_default();
                let ciudad: String = row.get(3).unwrap_or_default();
                let edad: u64 = row.get(4).unwrap_or_default();
                let sexo: Sexo = row.get(5).unwrap_or_default();
                let template: String = row.get(6).unwrap_or_default();

                let data = TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);

                Ok(data)
            }) {
                Ok(rows) => rows,
                Err(response) => return response,
            };

            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                query,
                table.len(),
                table.first().map_or_else(Default::default, |d| d.score),
                table.last().map_or_else(Default::default, |d| d.score),
            );

            let historial = match update_historial(&db, &params.search_str) {
                Ok(historial) => historial,
                Err(response) => return response,
            };

            Table {
                msg: format!("Hay un total de {} resultados.", table.len()),
                table,
                historial,
            }
            .into()
        }
        SearchStrategy::HybridRrf => {
            let query_emb = openai::embed_single(query.to_string(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");
            let embedding = query_emb.as_bytes();
            // Normalizo los datos que estan en un rango de 0 a 100 para que esten de 0 a 1.
            let weight_vec = params.peso_semantic / 100.0;
            let weight_fts: f32 = params.peso_fts / 100.0;
            let rrf_k: i64 = 60;
            let k: i64 = 1_000;

            let mut search_query = SearchQuery::new(
                &db,
                "
                with vec_matches as (
                select
                    row_id,
                    row_number() over (order by distance) as rank_number,
                    distance
                from vec_tnea
                where
                    template_embedding match :embedding
                    and k = :k
                ),

                fts_matches as (
                select
                    rowid as row_id,
                    row_number() over (order by rank) as rank_number,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                final as (
                select
                    tnea.template,
                    tnea.email,
                    tnea.provincia, 
                    tnea.ciudad,
                    tnea.edad,
                    tnea.sexo,
                    vec_matches.rank_number as vec_rank,
                    fts_matches.rank_number as fts_rank,
                    (
                        coalesce(1.0 / (:rrf_k + fts_matches.rank_number), 0.0) * :weight_fts +
                        coalesce(1.0 / (:rrf_k + vec_matches.rank_number), 0.0) * :weight_vec
                    ) as combined_rank,
                    vec_matches.distance as vec_distance,
                    fts_matches.score as fts_score
                from fts_matches
                full outer join vec_matches on vec_matches.row_id = fts_matches.row_id
                join tnea on tnea.id = coalesce(fts_matches.row_id, vec_matches.row_id)
                where tnea.edad between :edad_min and :edad_max
            ",
            );
            search_query.add_bindings(&[
                &embedding,
                &query,
                &k,
                &weight_fts,
                &weight_vec,
                &rrf_k,
                &params.edad_min,
                &params.edad_max,
            ]);

            if !provincia.is_empty() {
                search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
            }

            if !ciudad.is_empty() {
                search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
            }

            match params.sexo {
                Sexo::U => (),
                Sexo::F => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::F]),
                Sexo::M => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::M]),
            };

            search_query.push_str(
                " 
                order by combined_rank desc
                ) 
                select * from final;
                ",
            );

            let table = match search_query.execute(|row| {
                let template: String = row.get(0).unwrap_or_default();
                let email: String = row.get(1).unwrap_or_default();
                let provincia: String = row.get(2).unwrap_or_default();
                let ciudad: String = row.get(3).unwrap_or_default();
                let edad: u64 = row.get(4).unwrap_or_default();
                let sexo: Sexo = row.get(5).unwrap_or_default();
                let vec_rank: i64 = row.get(6).unwrap_or_default();
                let fts_rank: i64 = row.get(7).unwrap_or_default();
                let combined_rank: f32 = row.get(8).unwrap_or_default();
                let vec_score: f32 = row.get(9).unwrap_or_default();
                let fts_score = row.get::<_, f32>(10).unwrap_or_default() * -1.;

                let data = ReRankDisplay::new(
                    template,
                    email,
                    provincia,
                    ciudad,
                    edad,
                    sexo,
                    fts_rank,
                    vec_rank,
                    combined_rank,
                    vec_score,
                    fts_score,
                );
                Ok(data)
            }) {
                Ok(rows) => rows,
                Err(response) => return response,
            };

            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                query,
                table.len(),
                table.first().map_or_else(Default::default, |d| d.combined_rank),
                table.last().map_or_else(Default::default, |d| d.combined_rank),
            );

            let historial = match update_historial(&db, &params.search_str) {
                Ok(historial) => historial,
                Err(response) => return response,
            };

            RrfTable {
                msg: format!("Hay un total de {} resultados.", table.len()),
                table,
                historial,
            }
            .into()
        }
        SearchStrategy::HybridKf => {
            let query_emb = openai::embed_single(query.to_string(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let embedding = query_emb.as_bytes();
            let k: i64 = 1000;

            let mut search_query = SearchQuery::new(
                &db,
                "
                with fts_matches as (
                select
                    rowid as row_id,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                vec_matches as (
                select
                    row_id,
                    distance as score
                from vec_tnea
                where
                    template_embedding match :embedding
                    and k = :k
                order by distance
                ),

                combined as (
                select 'fts' as match_type, * from fts_matches
                union all
                select 'vec' as match_type, * from vec_matches
                ),

                final as (
                select distinct
                    tnea.template,
                    tnea.email,
                    tnea.provincia,
                    tnea.ciudad,
                    tnea.edad,
                    tnea.sexo,
                    combined.score,
                    combined.match_type
                from combined
                left join tnea on tnea.id = combined.row_id
                where tnea.edad between :edad_min and :edad_max
                ",
            );
            search_query.add_bindings(&[
                &k,
                &query,
                &embedding,
                &params.edad_min,
                &params.edad_max,
            ]);

            if !provincia.is_empty() {
                search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
            }

            if !ciudad.is_empty() {
                search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
            }

            match params.sexo {
                Sexo::U => (),
                Sexo::F => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::F]),
                Sexo::M => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::M]),
            };

            search_query.push_str(" ) select * from final;");

            let rows = match search_query.execute(|row| {
                let template: String = row.get(0).unwrap_or_default();
                let email: String = row.get(1).unwrap_or_default();
                let provincia: String = row.get(2).unwrap_or_default();
                let ciudad: String = row.get(3).unwrap_or_default();
                let edad: u64 = row.get(4).unwrap_or_default();
                let sexo: Sexo = row.get(5).unwrap_or_default();
                let score: f32 = row.get(6).unwrap_or_default();

                let data = TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                Ok(data)
            }) {
                Ok(rows) => rows,
                Err(response) => return response,
            };

            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                query,
                rows.len(),
                rows.first().map_or_else(Default::default, |d| d.score),
                rows.last().map_or_else(Default::default, |d| d.score),
            );

            let historial = match update_historial(&db, &params.search_str) {
                Ok(historial) => historial,
                Err(response) => return response,
            };

            Table {
                msg: format!("Hay un total de {} resultados.", rows.len()),
                table: rows,
                historial,
            }
            .into()
        }
        SearchStrategy::HybridReRank => {
            let query_emb = openai::embed_single(query.to_string(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");
            let embedding = query_emb.as_bytes();
            let k: i64 = 1000;

            let mut search_query = SearchQuery::new(
                &db,
                "
                with fts_matches as (
                select
                    rowid,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                embeddings AS (
                    SELECT
                        row_id as rowid,
                        template_embedding
                    FROM vec_tnea
                    WHERE row_id IN (SELECT rowid FROM fts_matches)
                ),

                final as (
                select
                    tnea.template,
                    tnea.email,
                    tnea.provincia,
                    tnea.ciudad,
                    tnea.edad,
                    tnea.sexo,
                    fts_matches.score,
                    'fts' as match_type
                from fts_matches
                left join tnea on tnea.id = fts_matches.rowid
                left join embeddings on embeddings.rowid = fts_matches.rowid
                where tnea.edad between :edad_min and :edad_max
            ",
            );
            search_query.add_bindings(&[
                &k,
                &query,
                &embedding,
                &params.edad_min,
                &params.edad_max,
            ]);

            if !provincia.is_empty() {
                search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
            }

            if !ciudad.is_empty() {
                search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
            }

            match params.sexo {
                Sexo::U => (),
                Sexo::F => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::F]),
                Sexo::M => search_query.add_filter(" and tnea.sexo like :sexo", &[&Sexo::M]),
            };

            search_query.push_str(
                " order by vec_distance_cosine(:embedding, embeddings.template_embedding)
                )
                select * from final;",
            );

            let rows = match search_query.execute(|row| {
                let template: String = row.get(0).unwrap_or_default();
                let email: String = row.get(1).unwrap_or_default();
                let provincia: String = row.get(2).unwrap_or_default();
                let ciudad: String = row.get(3).unwrap_or_default();
                let edad: u64 = row.get(4).unwrap_or_default();
                let sexo: Sexo = row.get(5).unwrap_or_default();
                let score = row.get::<_, f32>(6).unwrap_or_default() * -1.;

                let data = TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                Ok(data)
            }) {
                Ok(rows) => rows,
                Err(response) => return response,
            };

            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                query,
                rows.len(),
                rows.first().map_or_else(Default::default, |d| d.score),
                rows.last().map_or_else(Default::default, |d| d.score),
            );

            let historial = match update_historial(&db, &params.search_str) {
                Ok(historial) => historial,
                Err(response) => return response,
            };

            Table {
                msg: format!("Hay un total de {} resultados.", rows.len()),
                table: rows,
                historial,
            }
            .into()
        }
    }
}

#[instrument(name = "Actualizando el historial", skip(db))]
fn update_historial(
    db: &rusqlite::Connection,
    query: &str,
) -> eyre::Result<Vec<Historial>, SearchResponse> {
    if let Err(err) = sqlite::update_historial(db, query) {
        tracing::error!("{:?}", err);
        return Err(Fallback.into());
    }

    let historial = match sqlite::get_historial(db) {
        Ok(historial) => historial,
        Err(err) => {
            tracing::error!("{:?}", err);
            return Err(Fallback.into());
        }
    };
    Ok(historial)
}

struct SearchQuery<'a> {
    db: &'a rusqlite::Connection,
    pub stmt_str: String,
    pub bindings: Vec<&'a dyn ToSql>,
}

impl<'a> SearchQuery<'a> {
    fn new(db: &'a rusqlite::Connection, base_stmt: &str) -> Self {
        Self {
            db,
            stmt_str: base_stmt.to_string(),
            bindings: Vec::new(),
        }
    }

    fn add_filter(&mut self, filter: &str, binding: &[&'a dyn ToSql]) {
        self.stmt_str.push_str(filter);
        self.bindings.extend_from_slice(binding);
    }

    fn add_bindings(&mut self, binding: &[&'a dyn ToSql]) {
        self.bindings.extend_from_slice(binding);
    }

    fn push_str(&mut self, stmt: &str) {
        self.stmt_str.push_str(stmt);
    }

    fn execute<F, T>(&self, map_fn: F) -> Result<Vec<T>, SearchResponse>
    where
        T: ResponseMarker,
        F: Fn(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        let mut statement = match self.db.prepare(&self.stmt_str) {
            Ok(stmt) => stmt,
            Err(err) => {
                let err = ReportError(err.into());
                tracing::error!("{:?}", err);
                return Err(Fallback.into());
            }
        };

        let table = match statement.query_map(&*self.bindings, map_fn) {
            Ok(rows) => Ok(rows.collect::<Result<Vec<T>, _>>().unwrap_or_default()),
            Err(err) => {
                let err = ReportError(err.into());
                tracing::error!("{:?}", err);
                return Err(Fallback.into());
            }
        };

        table
    }
}
