mod health_check;
mod historial;
mod query;

pub use health_check::*;
pub use historial::*;
pub use query::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Historial {
    pub id: i64,
    pub query: String,
    pub result: String,
    pub timestamp: Option<chrono::NaiveDateTime>,
}
