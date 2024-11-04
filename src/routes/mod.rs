mod assets;
mod fallback;
mod health_check;
mod historial;
mod index;
mod search;

use askama_axum::{IntoResponse, Response};
pub use assets::*;
pub use fallback::*;
pub use health_check::*;
pub use historial::*;
use http::StatusCode;
pub use index::*;
pub use search::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Historial {
    pub id: usize,
    pub query: String,
    pub result: String,
    pub timestamp: Option<chrono::NaiveDateTime>,
}

impl Historial {
    #[must_use]
    pub fn new(
        id: usize,
        query: String,
        result: String,
        timestamp: Option<chrono::NaiveDateTime>,
    ) -> Self {
        Self {
            id,
            query,
            result,
            timestamp,
        }
    }
}

#[derive(Deserialize, Debug)]
pub enum SearchStrategy {
    Fts,
    Semantic,
    HybridKf,
    HybridRrf,
    HybridReRank,
}

impl TryFrom<String> for SearchStrategy {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "fts" => Ok(Self::Fts),
            "semantic_search" => Ok(Self::Semantic),
            "rrf" => Ok(Self::HybridRrf),
            "hkf" => Ok(Self::HybridKf),
            "rrs" => Ok(Self::HybridReRank),
            other => Err(format!(
                "{other} No es una estrategia de búsqueda soportada, usa 'fts', 'semantic_search', 'HKF' o 'rrf'",
            )),
        }
    }
}

pub struct ReportError(eyre::Report);

impl From<eyre::Report> for ReportError {
    fn from(err: eyre::Report) -> Self {
        ReportError(err)
    }
}

impl IntoResponse for ReportError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Internal server error: {:?}", self.0),
        )
            .into_response()
    }
}
