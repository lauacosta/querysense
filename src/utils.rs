use std::path::{Path, PathBuf};

use eyre::eyre;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct TneaData {
    pub email: String,
    pub nombre: String,
    #[serde(deserialize_with = "default_if_empty")]
    pub sexo: String,
    pub fecha_nacimiento: String,
    #[serde(deserialize_with = "deserialize_number_from_string_including_empty")]
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
    Ok(s.unwrap_or_default())
}

fn deserialize_number_from_string_including_empty<'de, D>(
    deserializer: D,
) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) if s.is_empty() => Ok(0),
        serde_json::Value::String(s) => s.parse::<usize>().map_err(serde::de::Error::custom),
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("Invalid number format"))
            .map(|n| n as usize),
        serde_json::Value::Null => Ok(0),
        _ => Err(serde::de::Error::custom("Expected string or number")),
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum DataSources {
    Csv,
    Json,
}

impl DataSources {
    pub fn from_extension(ext: &str) -> eyre::Result<Self> {
        let file = match ext {
            "csv" => DataSources::Csv,
            "json" => DataSources::Json,
            _ => return Err(eyre!("Extension desconocida {ext}")),
        };

        Ok(file)
    }
}

pub(crate) fn parse_sources(path: impl AsRef<Path>) -> eyre::Result<Vec<(PathBuf, DataSources)>> {
    let mut datasources = Vec::new();

    tracing::info!("Escaneando los archivos disponibles...");
    for file in std::fs::read_dir(&path)? {
        let path = file?.path().clone();

        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
                let file = DataSources::from_extension(ext)?;
                datasources.push((path, file));
            }
        }
    }

    tracing::info!("Escaneando los archivos disponibles... listo!");

    Ok(datasources)
}
