mod assets;
mod fallback;
mod health_check;
mod historial;
mod index;
mod search;

pub use assets::*;
pub use fallback::*;
pub use health_check::*;
pub use historial::*;
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

pub enum SearchStrategy {
    Fts,
    Semantic,
    Hybrid,
}

impl TryFrom<String> for SearchStrategy {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "fts" => Ok(Self::Fts),
            "semantic_search" => Ok(Self::Semantic),
            "hybrid_search" => Ok(Self::Hybrid),
            other => Err(format!(
                "{other} No es una estrategia de búsqueda soportada, usa 'fts', 'semantic_search' o 'hybrid_search'",
            )),
        }
    }
}
