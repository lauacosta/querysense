use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};
use serde_aux::prelude::deserialize_number_from_string;

use crate::configuration;

#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct TneaData {
    pub email: String,
    pub nombre: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub sexo: String,
    pub fecha_nacimiento: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub edad: usize,
    pub provincia: String,
    pub ciudad: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub descripcion: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub estudios: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub experiencia: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub estudios_mas_recientes: String,
}

// https://serde.rs/field-attrs.html#deserialize_with
fn default_if_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(match s {
        Some(ref s) if s.is_empty() => String::new(),
        Some(s) => s,
        None => String::new(),
    })
}

pub trait RegistroSQLITE {}

impl RegistroSQLITE for TneaData {}

pub fn parse_and_embed(
    path: impl AsRef<Path> + std::fmt::Display,
    template: &configuration::Template,
) -> eyre::Result<Vec<TneaData>> {
    let mut datasources = Vec::new();

    tracing::info!("Escaneando los archivos .csv disponibles...");

    for file in std::fs::read_dir(&path)? {
        let path = file?.path();

        if path.is_file() && path.extension().is_some_and(|str| str == "csv") {
            if let Some(filename) = path.file_name() {
                datasources.push(filename.to_string_lossy().to_string());
            }
        }
    }

    tracing::info!("Escaneando los archivos .csv disponibles... listo!");

    let mut reader_config = csv::ReaderBuilder::new();
    let mut result = Vec::new();

    for source in datasources {
        tracing::info!("Leyendo {}{}...", path, source);
        let mut reader = reader_config
            .flexible(true)
            .has_headers(true)
            .from_path(format!("{path}{source}"))?;

        let headers: Vec<String> = reader
            .headers()?
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect();

        for field in &template.fields {
            if !headers.contains(field) {
                return Err(eyre::eyre!(
                    "El archivo {}{} no tiene el header {}.",
                    path,
                    source,
                    field
                ));
            }
        }

        let data = reader
                .deserialize()
                .collect::<Result<Vec<TneaData>, csv::Error>>()
                .map_err(|err| eyre::eyre!("{source} no pudo se deserializado. Hay que controlar que tenga los headers correctos. Err: {err}"))?;

        result.extend(data);

        tracing::info!("Leyendo {}{}... listo!", path, source);
    }

    Ok(result)
}
