mod assets;
mod health_check;
mod historial;
mod index;
mod search;

pub use assets::*;
use axum::{async_trait, extract::FromRequestParts, response::IntoResponse};
use color_eyre::Report;
pub use health_check::*;
pub use historial::*;
use http::{Uri, request::Parts};
pub use index::*;

use querysense_common::{HttpError, SearchResult};
use querysense_openai::embed_single;
use querysense_sqlite::{get_historial, normalize};
use querysense_ui::{Historial, ReRankDisplay, ResponseMarker, RrfTable, Sexo, Table, TneaDisplay};
use reqwest::Client;
use rusqlite::{Connection, ToSql};
pub use search::*;

use serde::{Deserialize, de::DeserializeOwned};
use thiserror::Error;
use tracing::instrument;
use zerocopy::IntoBytes;

#[derive(Deserialize, Debug, Clone)]
pub struct Params {
    #[serde(rename = "query")]
    search_str: String,
    strategy: SearchStrategy,
    sexo: Sexo,
    edad_min: u64,
    edad_max: u64,
    peso_fts: f32,
    peso_semantic: f32,
    #[serde(rename = "k")]
    k_neighbors: u64,
}

#[derive(Debug)]
pub struct SearchString {
    query: String,
    provincia: Option<String>,
    ciudad: Option<String>,
}

impl SearchString {
    pub fn parse(search_str: &str) -> Self {
        if let Some((query, filters)) = search_str.split_once('|') {
            if let Some((provincia, ciudad)) = filters.split_once(',') {
                // TODO: Filter if it is only whitespace.
                let provincia = Some(format!("%{}%", normalize(provincia)));
                let ciudad = Some(format!("%{}%", normalize(ciudad)));
                return Self {
                    query: normalize(query),
                    provincia,
                    ciudad,
                };
            } else {
                let provincia = Some(format!("%{}%", normalize(filters)));
                return Self {
                    query: normalize(query),
                    provincia,
                    ciudad: None,
                };
            }
        } else {
            return Self {
                query: normalize(search_str),
                provincia: None,
                ciudad: None,
            };
        }
    }
}

#[derive(Deserialize, Debug, Clone, Copy)]
pub enum SearchStrategy {
    Fts,
    Semantic,
    KeywordFirst,
    ReciprocalRankFusion,
    ReRankBySemantics,
}

impl SearchStrategy {
    pub async fn search(self, db_path: &str, client: &Client, params: Params) -> SearchResult {
        let db = Connection::open(db_path)
            .expect("Deberia ser un path valido a una base de datos sqlite.");
        let search = SearchString::parse(&params.search_str);
        tracing::debug!(?search);
        let query = search.query;
        let provincia = search.provincia;
        let ciudad = search.ciudad;

        match self {
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

                if provincia.is_some() {
                    search_query.add_filter(" and provincia like :provincia", &[&provincia]);
                }
                if ciudad.is_some() {
                    search_query.add_filter(" and ciudad like :ciudad", &[&ciudad]);
                }

                match params.sexo {
                    Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::U => (),
                };

                search_query.push_str(" order by rank");

                let table = search_query.execute(|row| {
                    let score = row.get::<_, f32>(0).unwrap_or_default() * -1.;
                    let email: String = row.get(1).unwrap_or_default();
                    let provincia: String = row.get(2).unwrap_or_default();
                    let ciudad: String = row.get(3).unwrap_or_default();
                    let edad: u64 = row.get(4).unwrap_or_default();
                    let sexo: Sexo = row.get(5).unwrap_or_default();
                    let template: String = row.get(6).unwrap_or_default();

                    let data =
                        TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                    Ok(data)
                })?;

                tracing::info!(
                    "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                    query,
                    table.len(),
                    table.first().map_or_else(Default::default, |d| d.score),
                    table.last().map_or_else(Default::default, |d| d.score),
                );

                let historial = update_historial(&db, &params.search_str)?;

                Ok(Table {
                    msg: format!("Hay un total de {} resultados.", table.len()),
                    table,
                    historial,
                }
                .into_response())
            }
            SearchStrategy::Semantic => {
                let query_emb = embed_single(query.to_string(), client)
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

                if provincia.is_some() {
                    search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
                }

                if ciudad.is_some() {
                    search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
                }

                match params.sexo {
                    Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::U => (),
                };

                let table = search_query.execute(|row| {
                    let score = row.get::<_, f32>(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let provincia: String = row.get(2).unwrap_or_default();
                    let ciudad: String = row.get(3).unwrap_or_default();
                    let edad: u64 = row.get(4).unwrap_or_default();
                    let sexo: Sexo = row.get(5).unwrap_or_default();
                    let template: String = row.get(6).unwrap_or_default();

                    let data =
                        TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);

                    Ok(data)
                })?;

                tracing::info!(
                    "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                    query,
                    table.len(),
                    table.first().map_or_else(Default::default, |d| d.score),
                    table.last().map_or_else(Default::default, |d| d.score),
                );

                let historial = update_historial(&db, &params.search_str)?;

                Ok(Table {
                    msg: format!("Hay un total de {} resultados.", table.len()),
                    table,
                    historial,
                }
                .into_response())
            }
            SearchStrategy::ReciprocalRankFusion => {
                let query_emb = embed_single(query.to_string(), client)
                    .await
                    .map_err(|err| tracing::error!("{err}"))
                    .expect("Fallo al crear un embedding del query");

                let embedding = query_emb.as_bytes();
                // Normalizo los datos que estan en un rango de 0 a 100 para que esten de 0 a 1.
                let weight_vec = params.peso_semantic / 100.0;
                let weight_fts: f32 = params.peso_fts / 100.0;
                let rrf_k: i64 = 60;
                let k = params.k_neighbors;

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
                    &k,
                    &query,
                    &rrf_k,
                    &weight_fts,
                    &weight_vec,
                    &params.edad_min,
                    &params.edad_max,
                ]);

                if provincia.is_some() {
                    search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
                }

                if ciudad.is_some() {
                    search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
                }

                match params.sexo {
                    Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::U => (),
                };

                search_query.push_str(
                    " order by combined_rank desc
                    ) 
                    select * from final;
                ",
                );

                let table = search_query.execute(|row| {
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
                })?;

                tracing::info!(
                    "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                    query,
                    table.len(),
                    table
                        .first()
                        .map_or_else(Default::default, |d| d.combined_rank),
                    table
                        .last()
                        .map_or_else(Default::default, |d| d.combined_rank),
                );

                let historial = update_historial(&db, &params.search_str)?;

                Ok(RrfTable {
                    msg: format!("Hay un total de {} resultados.", table.len()),
                    table,
                    historial,
                }
                .into_response())
            }

            SearchStrategy::KeywordFirst => {
                let query_emb = embed_single(query.to_string(), client)
                    .await
                    .map_err(|err| tracing::error!("{err}"))
                    .expect("Fallo al crear un embedding del query");

                let embedding = query_emb.as_bytes();
                let k = params.k_neighbors;

                let mut search_query = SearchQuery::new(
                    &db,
                    "
                with fts_matches as (
                select
                    rowid as row_id,
                    rank as score
                from fts_tnea
                where template match :query
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
                    &query,
                    &k,
                    &embedding,
                    &params.edad_min,
                    &params.edad_max,
                ]);

                if provincia.is_some() {
                    search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
                }

                if ciudad.is_some() {
                    search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
                }

                match params.sexo {
                    Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::U => (),
                };

                search_query.push_str(" ) select * from final;");

                let rows = search_query.execute(|row| {
                    let template: String = row.get(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let provincia: String = row.get(2).unwrap_or_default();
                    let ciudad: String = row.get(3).unwrap_or_default();
                    let edad: u64 = row.get(4).unwrap_or_default();
                    let sexo: Sexo = row.get(5).unwrap_or_default();
                    let score: f32 = row.get(6).unwrap_or_default();

                    let data =
                        TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                    Ok(data)
                })?;

                tracing::info!(
                    "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                    query,
                    rows.len(),
                    rows.first().map_or_else(Default::default, |d| d.score),
                    rows.last().map_or_else(Default::default, |d| d.score),
                );

                let historial = update_historial(&db, &params.search_str)?;

                Ok(Table {
                    msg: format!("Hay un total de {} resultados.", rows.len()),
                    table: rows,
                    historial,
                }
                .into_response())
            }
            SearchStrategy::ReRankBySemantics => {
                let query_emb = embed_single(query.to_string(), client)
                    .await
                    .map_err(|err| tracing::error!("{err}"))
                    .expect("Fallo al crear un embedding del query");
                let embedding = query_emb.as_bytes();
                let k = params.k_neighbors;

                let mut search_query = SearchQuery::new(
                    &db,
                    "
                with fts_matches as (
                select
                    rowid,
                    rank as score
                from fts_tnea
                where template match :query
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
                search_query.add_bindings(&[&query, &k, &params.edad_min, &params.edad_max]);

                if provincia.is_some() {
                    search_query.add_filter(" and tnea.provincia like :provincia", &[&provincia]);
                }

                if ciudad.is_some() {
                    search_query.add_filter(" and tnea.ciudad like :ciudad", &[&ciudad]);
                }

                match params.sexo {
                    Sexo::M => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::F => search_query.add_filter(" and sexo = :sexo", &[&params.sexo]),
                    Sexo::U => (),
                };

                search_query.add_filter(
                    " order by vec_distance_cosine(:embedding, embeddings.template_embedding)
                )
                select * from final;",
                    &[&embedding],
                );

                let rows = search_query.execute(|row| {
                    let template: String = row.get(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let provincia: String = row.get(2).unwrap_or_default();
                    let ciudad: String = row.get(3).unwrap_or_default();
                    let edad: u64 = row.get(4).unwrap_or_default();
                    let sexo: Sexo = row.get(5).unwrap_or_default();
                    let score = row.get::<_, f32>(6).unwrap_or_default() * -1.;

                    let data =
                        TneaDisplay::new(email, provincia, ciudad, edad, sexo, template, score);
                    Ok(data)
                })?;

                tracing::info!(
                    "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}`",
                    query,
                    rows.len(),
                    rows.first().map_or_else(Default::default, |d| d.score),
                    rows.last().map_or_else(Default::default, |d| d.score),
                );

                let historial = update_historial(&db, &params.search_str)?;

                Ok(Table {
                    msg: format!("Hay un total de {} resultados.", rows.len()),
                    table: rows,
                    historial,
                }
                .into_response())
            }
        }
    }
}

impl TryFrom<String> for SearchStrategy {
    type Error = Report;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "fts" => Ok(Self::Fts),
            "semantic_search" => Ok(Self::Semantic),
            "rrf" => Ok(Self::ReciprocalRankFusion),
            "hkf" => Ok(Self::KeywordFirst),
            "rrs" => Ok(Self::ReRankBySemantics),
            other => Err(SearchStrategyError::UnsupportedSearchStrategy(other.to_string()).into()),
        }
    }
}

#[derive(Debug, Error)]
enum SearchStrategyError {
    #[error(
        "'{0}' No es una estrategia de búsqueda soportada, usa 'fts', 'semantic_search', 'HKF' o 'rrf'"
    )]
    UnsupportedSearchStrategy(String),
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

    fn execute<F, T>(&self, map_fn: F) -> Result<Vec<T>, HttpError>
    where
        T: ResponseMarker,
        F: Fn(&rusqlite::Row) -> rusqlite::Result<T>,
    {
        tracing::debug!("{:?}", self.stmt_str);
        let mut statement = self.db.prepare(&self.stmt_str)?;

        let table = statement
            .query_map(&*self.bindings, map_fn)?
            .collect::<Result<Vec<T>, _>>()?;

        Ok(table)
    }
}

#[instrument(name = "Actualizando el historial", skip(db))]
fn update_historial(
    db: &rusqlite::Connection,
    query: &str,
) -> eyre::Result<Vec<Historial>, HttpError> {
    querysense_sqlite::update_historial(db, query)?;

    Ok(get_historial(db)?)
}

pub struct SearchExtractor<T>(pub T);

impl<T> SearchExtractor<T>
where
    T: DeserializeOwned,
{
    pub fn try_from_uri(value: &Uri) -> Result<Self, HttpError> {
        let query = value.query().unwrap_or_default();
        let params = serde_urlencoded::from_str(query)?;
        Ok(SearchExtractor(params))
    }
}

#[async_trait]
impl<T, S> FromRequestParts<S> for SearchExtractor<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Self::try_from_uri(&parts.uri)
    }
}