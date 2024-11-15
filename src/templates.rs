use std::fmt::Display;

use askama_axum::{IntoResponse, Template};
use rusqlite::{
    types::{FromSql, FromSqlError, ValueRef},
    ToSql,
};
use serde::Deserialize;

pub enum SearchResponse {
    Common(Table),
    RrfTable(RrfTable),
    Fallback(Fallback),
}

impl IntoResponse for SearchResponse {
    fn into_response(self) -> askama_axum::Response {
        match self {
            SearchResponse::Common(table) => table.into_response(),
            SearchResponse::RrfTable(rrf_table) => rrf_table.into_response(),
            SearchResponse::Fallback(fallback) => fallback.into_response(),
        }
    }
}

impl From<Table> for SearchResponse {
    fn from(value: Table) -> Self {
        SearchResponse::Common(value)
    }
}

impl From<RrfTable> for SearchResponse {
    fn from(value: RrfTable) -> Self {
        SearchResponse::RrfTable(value)
    }
}

impl From<Fallback> for SearchResponse {
    fn from(value: Fallback) -> Self {
        SearchResponse::Fallback(value)
    }
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub historial: Vec<Historial>,
}

#[derive(Template)]
#[template(path = "fallback.html")]
pub struct Fallback;

#[derive(Template)]
#[template(path = "table.html")]
pub struct Table {
    pub msg: String,
    pub table: Vec<TneaDisplay>,
    pub historial: Vec<Historial>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            msg: "No se encontraron ningun registro.".to_string(),
            table: vec![TneaDisplay::default()],
            historial: vec![Historial::default()],
        }
    }
}

#[derive(Template)]
#[template(path = "table_rrf.html")]
pub struct RrfTable {
    pub msg: String,
    pub table: Vec<ReRankDisplay>,
    pub historial: Vec<Historial>,
}

impl Default for RrfTable {
    fn default() -> Self {
        Self {
            msg: "No se encontraron ningun registro.".to_string(),
            table: vec![ReRankDisplay::default()],
            historial: vec![Historial::default()],
        }
    }
}

pub enum TableData {
    Standard(Vec<TneaDisplay>),
    Rrf(Vec<ReRankDisplay>),
}

#[derive(Debug, Clone, Default)]
pub struct TneaDisplay {
    email: String,
    provincia: String,
    ciudad: String,
    pub edad: u64,
    pub sexo: Sexo,
    template: String,
    pub score: f32,
}

impl TneaDisplay {
    #[must_use]
    pub fn new(
        email: String,
        provincia: String,
        ciudad: String,
        edad: u64,
        sexo: Sexo,
        template: String,
        score: f32,
    ) -> Self {
        Self {
            email,
            provincia,
            ciudad,
            edad,
            sexo,
            template,
            score,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReRankDisplay {
    template: String,
    email: String,

    provincia: String,
    ciudad: String,
    pub edad: u64,
    pub sexo: Sexo,
    fts_rank: i64,
    vec_rank: i64,
    pub combined_rank: f32,
    vec_score: f32,
    fts_score: f32,
}

impl ReRankDisplay {
    #[must_use]
    pub fn new(
        template: String,
        email: String,
        provincia: String,
        ciudad: String,
        edad: u64,
        sexo: Sexo,
        fts_rank: i64,
        vec_rank: i64,
        combined_rank: f32,
        vec_score: f32,
        fts_score: f32,
    ) -> Self {
        Self {
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
        }
    }
}

// El dataset solamente distingue entre estos dos.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
pub enum Sexo {
    #[default]
    U,
    F,
    M,
}

impl ToSql for Sexo {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let value = match self {
            Sexo::F => "F",
            Sexo::M => "M",
            Sexo::U => "U",
        };
        Ok(rusqlite::types::ToSqlOutput::from(value))
    }
}

impl FromSql for Sexo {
    fn column_result(value: ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value {
            ValueRef::Text(text) => match text {
                b"F" => Ok(Sexo::F),
                b"M" => Ok(Sexo::M),
                _ => Ok(Sexo::U),
            },
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl Display for Sexo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let content = match self {
            Sexo::U => "No definido",
            Sexo::F => "F",
            Sexo::M => "M",
        };
        write!(f, "{}", content)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Historial {
    pub id: u64,
    pub query: String,
}

impl Historial {
    #[must_use]
    pub fn new(id: u64, query: String) -> Self {
        Self { id, query }
    }
}
