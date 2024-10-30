use askama_axum::{IntoResponse, Template};

pub enum DisplayableContent {
    Common(Table),
    RrfTable(RrfTable),
}

impl IntoResponse for DisplayableContent {
    fn into_response(self) -> askama_axum::Response {
        match self {
            DisplayableContent::Common(table) => table.into_response(),
            DisplayableContent::RrfTable(rrf_table) => rrf_table.into_response(),
        }
    }
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index;

#[derive(Template)]
#[template(path = "table.html")]
pub struct Table {
    pub msg: String,
    pub table: Vec<TneaDisplay>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            msg: "No se encontraron ningun registro.".to_string(),
            table: vec![TneaDisplay::default()],
        }
    }
}

#[derive(Template)]
#[template(path = "table_rrf.html")]
pub struct RrfTable {
    pub msg: String,
    pub table: Vec<ReRankDisplay>,
}

impl Default for RrfTable {
    fn default() -> Self {
        Self {
            msg: "No se encontraron ningun registro.".to_string(),
            table: vec![ReRankDisplay::default()],
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
    edad: usize,
    sexo: String,
    template: String,
    pub score: f32,
    match_type: String,
}

impl TneaDisplay {
    #[must_use]
    pub fn new(
        email: String,
        edad: usize,
        sexo: String,
        template: String,
        score: f32,
        match_type: String,
    ) -> Self {
        Self {
            email,
            edad,
            sexo,
            template,
            score,
            match_type,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ReRankDisplay {
    template: String,
    email: String,
    edad: usize,
    sexo: String,
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
        edad: usize,
        sexo: String,
        fts_rank: i64,
        vec_rank: i64,
        combined_rank: f32,
        vec_score: f32,
        fts_score: f32,
    ) -> Self {
        Self {
            template,
            email,
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
