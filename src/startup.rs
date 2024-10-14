use core::fmt;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use axum::{body::Body, http::Request, routing::get, serve::Serve, Router};
use rusqlite::ffi::sqlite3_auto_extension;
use serde::de::DeserializeOwned;
use sqlite_vec::sqlite3_vec_init;
use tokio::signal;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::{error_span, Level};

use crate::configuration::FeatureState;
use crate::routes::index;
use crate::{
    configuration::Settings,
    routes::{get_from_db, health_check},
};
use crate::{RegistroSQLITE, TneaData};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub cache: FeatureState,
}

pub struct Application {
    pub port: u16,
    pub host: String,
    pub server: Serve<Router, Router>,
}

impl Application {
    /// # Errors
    /// Fallará si no logra obtener la direccion local del `tokio::net::TcpListener`.
    ///
    /// # Panics
    /// Entrará en panicos si no es capaz de:
    /// 1. Vincular un `tokio::net::TcpListener` a la dirección dada.
    /// 2. Falla en conectarse con el servidor de `MeiliSearch`.
    #[tracing::instrument(name = "Construyendo la aplicación.", skip(configuration))]
    pub async fn build(configuration: Settings) -> anyhow::Result<Self> {
        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );

        tracing::info!("Definiendo la direccion HTTP...");
        let listener = match tokio::net::TcpListener::bind(&address).await {
            Ok(listener) => listener,
            Err(err) => {
                tracing::error!("{err}. Tratando con otro puerto...");
                match tokio::net::TcpListener::bind(format!("{}:0", configuration.application.host))
                    .await
                {
                    Ok(listener) => listener,
                    Err(err) => {
                        tracing::error!("No hay puertos disponibles, finalizando la aplicación...");
                        return Err(err.into());
                    }
                }
            }
        };

        let port = listener
            .local_addr()
            .expect("Fallo al encontrar la local address")
            .port();
        let host = configuration.application.host;

        tracing::info!("Definiendo la direccion HTTP listo!");

        let data: Vec<TneaData> = parse_and_embed(
            "./csv/",
            Template::from_str(&configuration.application.template)?,
        )?;

        let db = Arc::new(Mutex::new(setup_sqlite(data)?));
        let cache = configuration.application.cache;

        let state = AppState { db, cache };

        let server = build_server(listener, state);

        tracing::info!("Construyendo la aplicación listo!");

        Ok(Self { port, host, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn host(&self) -> String {
        self.host.clone()
    }

    /// # Errors
    ///
    /// Devolverá error si ocurre algun inconveniente con tokio para programar la tarea asíncrona.
    /// # Panics
    ///
    /// Entrará en pánico si no es capaz de instalar el handler requerido.
    pub async fn run_until_stopped(self) -> std::io::Result<()> {
        self.server
            // https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs
            .with_graceful_shutdown(async move {
                let ctrl_c = async {
                    signal::ctrl_c()
                        .await
                        .expect("Fallo en instalar el handler para Ctrl+C");
                };
                #[cfg(unix)]
                let terminate = async {
                    signal::unix::signal(signal::unix::SignalKind::terminate())
                        .expect("Fallo en instalar el handler para las señales")
                        .recv()
                        .await;
                };

                #[cfg(not(unix))]
                let terminate = std::future::pending::<()>();

                tokio::select! {
                    () = ctrl_c => {
                    },
                    () = terminate => {
                    },
                }
            })
            .await
    }
}

pub fn build_server(listener: tokio::net::TcpListener, state: AppState) -> Serve<Router, Router> {
    let server = Router::new()
        .route("/health_check", get(health_check))
        .route("/", get(index))
        // .route("/search", get(search))
        .route("/historial", get(get_from_db))
        // .nest_service(
        //     "/",
        //     ServeDir::new("./dist").not_found_service(ServeFile::new("./fallout.html")),
        // )
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &Request<Body>| {
                            let request_id = request
                                .extensions()
                                .get::<RequestId>()
                                .map_or_else(|| "desconocido".into(), ToString::to_string);

                            error_span!(
                                "request",
                                id = %request_id,
                                method = %request.method(),
                                uri = %request.uri()
                            )
                        })
                        .on_response(
                            DefaultOnResponse::new()
                                .include_headers(true)
                                .level(Level::INFO),
                        ),
                )
                .layer(RequestIdLayer),
        );

    axum::serve(listener, server)
}

fn setup_sqlite(data: Vec<TneaData>) -> anyhow::Result<rusqlite::Connection> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    let path = std::env::var("DATABASE_URL").map_err(|err| {
        anyhow!(
            "La variable de ambiente `DATABASE_URL` no fue encontrada. {}",
            err
        )
    })?;

    let db = rusqlite::Connection::open(path)?;
    let (sqlite_version, vec_version): (String, String) =
        db.query_row("select sqlite_version(), vec_version()", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

    tracing::info!("sqlite_version={sqlite_version}, vec_version={vec_version}");

    tracing::info!("Creando las tablas historial, tnea...");
    db.execute_batch(
        "create table if not exists historial (
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
    ",
    )
    .map_err(|err| anyhow!(err))
    .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

    tracing::info!("Creando las tablas historial, tnea... listo!");

    tracing::info!("Abriendo transacción para insertar datos en la tabla tnea!");

    db.execute("BEGIN TRANSACTION", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

    {
        let mut statement = db.prepare(
            "insert into tnea (
            id,
            email,
            nombre,
            sexo,
            fecha_nacimiento,
            edad,
            provincia,
            ciudad,
            descripcion,
            estudios,
            estudios_mas_recientes,
            experiencia
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        for d in data {
            let TneaData {
                id,
                email,
                nombre,
                sexo,
                fecha_nacimiento,
                edad,
                provincia,
                ciudad,
                descripcion,
                estudios,
                estudios_mas_recientes,
                experiencia,
            } = d;

            statement.execute((
                id,
                email,
                nombre,
                sexo,
                fecha_nacimiento,
                edad,
                provincia,
                ciudad,
                descripcion,
                estudios,
                estudios_mas_recientes,
                experiencia,
            ))?;
        }
    }

    let num = db.execute("COMMIT", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );
    tracing::info!("Se insertaron {num} columnas!");

    Ok(db)
}

fn parse_and_embed<R, P>(path: P, template: Template) -> anyhow::Result<Vec<R>>
where
    R: RegistroSQLITE + DeserializeOwned,
    P: AsRef<Path> + fmt::Display,
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
                .map_err(|err| anyhow!("{source} no pudo se deserializado. Hay que controlar que tenga los headers correctos. Err: {err}"))?;

        result.extend(data);

        tracing::info!("Leyendo {}{}... listo!", path, source);
    }

    Ok(result)
}

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
