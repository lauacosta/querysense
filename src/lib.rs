use std::{path::Path, str::FromStr};

use rusqlite::ffi::sqlite3_auto_extension;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_aux::prelude::deserialize_number_from_string;
use sqlite_vec::sqlite3_vec_init;

pub mod configuration;
pub mod routes;
pub mod startup;
pub mod templates;

#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct TneaData {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub id: usize,
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
        Some(ref s) if s.is_empty() => "No definido".to_string(),
        Some(s) => s,
        None => "No definido".to_string(),
    })
}

pub trait RegistroSQLITE {}

impl RegistroSQLITE for TneaData {}

#[derive(Debug)]
pub struct Template {
    pub template: String,
    pub fields: Vec<String>,
}

impl FromStr for Template {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(anyhow::anyhow!("Un template no puede ser un string vacío"));
        }

        let mut start = 0;
        let separator = "{{";
        let separator_len = separator.len();
        let mut fields = Vec::new();

        while let Some(open_idx) = s[start..].find("{{") {
            if let Some(close_idx) = s[start + open_idx..].find("}}") {
                let field = &s[start + open_idx + separator_len..start + open_idx + close_idx];

                fields.push(field.trim().to_string());

                start += open_idx + close_idx + separator_len;
            } else {
                return Err(anyhow::anyhow!("El template esta mal conformado"));
            }
        }

        Ok(Self {
            template: s.to_string(),
            fields,
        })
    }
}

pub fn init_sqlite() -> anyhow::Result<rusqlite::Connection> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    let path = std::env::var("DATABASE_URL").map_err(|err| {
        anyhow::anyhow!(
            "La variable de ambiente `DATABASE_URL` no fue encontrada. {}",
            err
        )
    })?;
    Ok(rusqlite::Connection::open(path)?)
}

pub fn setup_sqlite(db: &rusqlite::Connection, template: &Template) -> anyhow::Result<()> {
    let (sqlite_version, vec_version): (String, String) =
        db.query_row("select sqlite_version(), vec_version()", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

    tracing::debug!("sqlite_version={sqlite_version}, vec_version={vec_version}");

    let fields_str = template.fields.join(",");
    db.execute_batch(
        format!(
            "
        create table if not exists historial (
            id integer primary key,
            query text not null unique,
            result text not null,
            timestamp datetime default current_timestamp
        );

        create index if not exists idx_query_timestamp on historial(query, timestamp);

        create table if not exists tnea(
            id integer primary key,
            email text,
            nombre text,
            sexo text,
            fecha_nacimiento text,
            edad integer not null,
            provincia text,
            ciudad text,
            descripcion text,
            estudios text,
            experiencia text,
            estudios_mas_recientes text
        );

        create virtual table if not exists fts_tnea using fts5(
            {fields_str},
            content='tnea', content_rowid='id'
        );


        " // .load ./vec0

                      // create virtual table if not exists vec_tnea using vec0(
                      //     user_id integer primary key,
                      //     template_embedding float[1024]
                      // );
        )
        .as_str(),
    )
    .map_err(|err| anyhow::anyhow!(err))
    .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

    Ok(())
}

pub fn parse_and_embed<R, P>(path: P, template: &Template) -> anyhow::Result<Vec<R>>
where
    R: RegistroSQLITE + DeserializeOwned + std::fmt::Debug,
    P: AsRef<Path> + std::fmt::Display,
{
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
            .map(|v| v.to_string())
            .collect();

        for field in &template.fields {
            if !headers.contains(field) {
                return Err(anyhow::anyhow!(
                    "El archivo /{}/{} no tiene el header {}.",
                    path,
                    source,
                    field
                ));
            }
        }

        let data = reader
                .deserialize()
                .collect::<Result<Vec<R>, csv::Error>>()
                .map_err(|err| anyhow::anyhow!("{source} no pudo se deserializado. Hay que controlar que tenga los headers correctos. Err: {err}"))?;

        result.extend(data);

        tracing::info!("Leyendo {}{}... listo!", path, source);
    }

    Ok(result)
}

pub fn print_title() {
    println!(
        "
 .d88b.  db    db d88888b d8888b. db    db .d8888. d88888b d8b   db .d8888. d88888b 
.8P  Y8. 88    88 88'     88  `8D `8b  d8' 88'  YP 88'     888o  88 88'  YP 88'     
88    88 88    88 88ooooo 88oobY'  `8bd8'  `8bo.   88ooooo 88V8o 88 `8bo.   88ooooo 
88    88 88    88 88ooooo 88`8b      88      `Y8b. 88ooooo 88 V8o88   `Y8b. 88ooooo 
`8P  d8' 88b  d88 88.     88 `88.    88    db   8D 88.     88  V888 db   8D 88.     
 `Y88'Y8 ~Y8888P' Y88888P 88   YD    YP    `8888Y' Y88888P VP   V8P `8888Y' Y88888P 
    "
    );
}
