pub mod cli;
pub mod configuration;
pub mod openai;
pub mod routes;
pub mod sqlite;
pub mod startup;
pub mod templates;
pub mod utils;

#[cfg(feature = "local")]
pub mod embeddings;
