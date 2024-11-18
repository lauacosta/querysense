use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use eyre::eyre;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{configuration, routes::ReportError};

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
        serde_json::Value::String(s) => {
            usize::from_str_radix(&s, 10).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("Invalid number format"))
            .map(|n| n as usize),
        serde_json::Value::Null => Ok(0),
        _ => Err(serde::de::Error::custom("Expected string or number")),
    }
}

#[derive(Debug, PartialEq)]
enum DataSources {
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

pub fn parse_and_embed(
    path: impl AsRef<Path>,
    template: &configuration::Template,
) -> eyre::Result<Vec<TneaData>> {
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

    let mut reader_config = csv::ReaderBuilder::new();
    let mut result = Vec::new();

    for (source, ext) in datasources {
        tracing::info!("Leyendo {source:?}...");

        let data = match ext {
            DataSources::Csv => {
                let mut reader = reader_config
                    .flexible(true)
                    .trim(csv::Trim::All)
                    .has_headers(true)
                    .quote(b'"')
                    .from_path(&source)?;

                let headers: Vec<String> = reader
                    .headers()?
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                for field in &template.fields {
                    if !headers.contains(field) {
                        return Err(eyre::eyre!(
                            "El archivo {source:?} no tiene el header {field}.",
                        ));
                    }
                }

                reader
                .deserialize()
                .collect::<Result<Vec<TneaData>, csv::Error>>()
                .map_err(|err| eyre::eyre!("{source:?} no pudo ser deserializado. Hay que controlar que tenga los headers correctos. Err: {err}"))?
            }
            DataSources::Json => {
                let file = File::open(&source)?;
                let reader = BufReader::new(file);

                serde_json::from_reader(reader)?
            }
        };

        let total = data.len();

        result.extend(data);

        tracing::info!("Leyendo {source:?}... listo! - {total} nuevos registros");
    }

    Ok(result)
}
