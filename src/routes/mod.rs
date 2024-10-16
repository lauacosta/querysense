mod health_check;
mod historial;
mod index;
mod search;

pub use health_check::*;
pub use historial::*;
pub use index::*;
pub use search::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Historial {
    pub id: i64,
    pub query: String,
    pub result: String,
    pub timestamp: Option<chrono::NaiveDateTime>,
}

impl Historial {
    pub fn new(
        id: i64,
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
