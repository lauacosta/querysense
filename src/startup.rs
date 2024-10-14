use axum::{body::Body, http::Request, routing::get, serve::Serve, Router};
use http::Uri;
use meilisearch_sdk::features::ExperimentalFeatures;
use secrecy::{ExposeSecret, Secret};
use serde_json::json;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::fmt::{Debug, Display};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::Duration;
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::{error_span, Level};

use crate::configuration::{
    FeatureState, InnerSettings, MeiliExperimentalFeatures, MeiliSettings, RequestConfig,
};
use crate::TneaData;
use crate::{
    configuration::Settings,
    routes::{get_from_db, health_check, search},
};

pub struct Application {
    pub port: u16,
    pub host: String,
    pub server: Serve<Router, Router>,
}

#[derive(Clone)]
pub struct AppState {
    pub search_client: meilisearch_sdk::client::Client,
    pub db: SqlitePool,
    pub ranking_score_threshold: f64,
    pub cache: FeatureState,
    pub request_config: RequestConfig,
}

#[derive(Debug)]
struct MeiliService {
    client: meilisearch_sdk::client::Client,
    bin: Child,
    experimental_features: MeiliExperimentalFeatures,
    settings: InnerSettings,
    address: Uri,
    master_key: Secret<String>,
}

impl MeiliService {
    #[tracing::instrument(name = "Creando un subproceso para MeiliSearch", skip(config))]
    pub async fn start(config: MeiliSettings) -> anyhow::Result<Self> {
        let address: Uri = format!("http://{}:{}", config.host, config.port)
            .parse()
            .expect("No es una URI bien conformada");

        let meili_client = meilisearch_sdk::client::Client::new(
            address.to_string(),
            Some(config.master_key.expose_secret()),
        )
        .expect("Fallo al iniciar un cliente con el servidor especificado.");

        let manifest_dir = PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR")
                .expect("Fallo en encontrar la variable de ambiente `CARGO_MANIFEST_DIR`"),
        );
        let meili_path = manifest_dir.join("meilisearch");

        let meilisearch_bin = std::process::Command::new(meili_path)
            .stderr(std::process::Stdio::inherit())
            .args([
                format!("--master-key={}", config.master_key.expose_secret()).as_str(),
                "--dump-dir",
                "meili_data/dumps/",
                "--no-analytics",
            ])
            .spawn()
            .expect("Fallo en encontrar el ejecutable `meilisearch`.");

        let tries = 10;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        for i in 0..tries {
            let response = match client.get(format!("{address}/health")).send().await {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        "Deberia ser capaz de consultar el estado de MeiliSearch. {err}"
                    );
                    tracing::info!("Esperando a que Meilisearch inicie... ({i}/{tries})");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };

            if response.status().as_u16() == 200 {
                tracing::info!("Meilisearch inició correctamente!");
                return Ok(Self {
                    bin: meilisearch_bin,
                    client: meili_client,
                    settings: config.settings,
                    master_key: config.master_key,
                    address,
                    experimental_features: config.experimental_features,
                });
            }
            tracing::info!("Esperando a que Meilisearch inicie... ({i}/{tries})");
            std::thread::sleep(Duration::from_secs(5));
        }

        Err(anyhow::anyhow!("Meilisearch no inició correctamente"))
    }

    /// # Errors
    ///
    /// Devolverá error si no es capaz de:
    /// 1. Encontrar los archivos definidos por `files`.
    /// 2. Serializar el contenido de los archivos en `TneaData`.
    /// 3. Crear el cliente http.
    /// 4. Modificar la configuración del índice con respecto a `vector_store`.
    ///
    /// # Panics
    ///
    /// Entrará en pánico si no es capaz de:
    /// 1. Crear el índice `tnea`.
    /// 2. Actualizar la configuración del índice `tnea`.
    #[tracing::instrument(name = "Configurando MeiliSearch", skip(self, files))]
    pub async fn setup_meili<P>(&self, files: Vec<P>) -> anyhow::Result<()>
    where
        P: AsRef<Path> + Display + Debug,
    {
        if let Err(err) = self.client.get_index("tnea").await {
            tracing::warn!("{}", err);
            tracing::info!("Creando el índice `tnea`!");
            self.client
                .create_index("tnea", Some("id"))
                .await
                .expect("Deberia ser capaz de crear el índice `tnea`");
        }

        let index = self
            .client
            .get_index("tnea")
            .await
            .expect("El índice debería estar creado");

        if index
            .get_stats()
            .await
            .expect("Fallo en obtener los stats del índice `tnea`")
            .number_of_documents
            == 0
        {
            tracing::info!("El índice `tnea` no tiene documentos cargados!");

            let mut documents = Vec::new();

            tracing::info!("Leyendo los archivos .csv para cargar los documentos en `tnea`...");
            for file in files {
                tracing::info!("Leyendo {file}...");
                let mut reader = csv::ReaderBuilder::new()
                    .flexible(true)
                    .trim(csv::Trim::All)
                    .has_headers(true)
                    .from_path(format!("./csv/{file}"))
                    .unwrap_or_else(|_| panic!("No se pudo encontrar el archivo {file}"));

                for doc in reader.deserialize() {
                    let val: TneaData = doc?;
                    documents.push(val);
                }
            }
            tracing::info!("Leyendo los archivos .csv para cargar los documentos en `tnea` listo!");

            tracing::info!("Programando los datos leidos para su carga...");
            let _ = self
                .client
                .index("tnea")
                .add_documents(&documents, Some("id"))
                .await
                .expect("Fallo al añadir documentos al índice `tnea`");

            tracing::info!("Programando los datos leidos para su carga listo!");
        }

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Fallo al crear el cliente http");

        let json_settings = {
            match self.experimental_features.vec_store {
                FeatureState::Enabled => {
                    let mut features = ExperimentalFeatures::new(&self.client);
                    features.set_vector_store(true).update().await?;
                    json!(
                    {
                        "pagination": {
                            "maxTotalHits": self.settings.pagination.max_total_hits
                        },

                        "embedders": {
                            "default": {
                                "source": self.settings.embedders.source.as_str(),
                                "apiKey": self.settings.embedders.api_key.expose_secret(),
                                "model": self.settings.embedders.model,
                                "documentTemplate": self.settings.embedders.document_template.expose_secret(),
                                "dimensions": self.settings.embedders.dimensions
                            }
                        }
                    })
                }
                FeatureState::Disabled => {
                    let mut features = ExperimentalFeatures::new(&self.client);
                    features.set_vector_store(false).update().await?;
                    json!(
                    {
                        "pagination": {
                            "maxTotalHits": self.settings.pagination.max_total_hits
                        },
                    })
                }
            }
        };

        let response = http_client
            .patch(format!("{}/indexes/tnea/settings", self.address))
            .header(
                "Authorization",
                format!("Bearer {}", self.master_key.expose_secret()),
            )
            .header("Content-Type", "application/json")
            .json(&json_settings)
            .send()
            .await
            .expect("Fallo al enviar el http request hacia el servidor para configurar el índice `tnea`");

        assert_eq!(response.status().as_u16(), 202);
        tracing::info!("El índice 'tnea' se ha configurado exitosamente!");

        Ok(())
    }
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
    pub async fn build(configuration: Settings) -> anyhow::Result<(Self, Child)> {
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

        let ranking_score_threshold = configuration.request_config.ranking_score_threshold;
        let cache = configuration.application.cache;

        tracing::info!("Escaneando los archivos .csv disponibles...");
        let mut files = Vec::new();

        for file in std::fs::read_dir("./csv/").expect("No se encuentra el directorio /csv/") {
            let path = file?.path();

            if path.is_file() && path.extension().is_some_and(|str| str == "csv") {
                if let Some(filename) = path.file_name() {
                    files.push(filename.to_string_lossy().to_string());
                }
            }
        }
        tracing::info!("Escaneando los archivos .csv disponibles... listo!");

        let service = MeiliService::start(configuration.search_engine).await?;
        service.setup_meili(files).await?;

        let db = SqlitePoolOptions::new().connect_lazy_with(
            SqliteConnectOptions::new()
                .extension("vec0")
                .filename("./tnea_gestion.db")
                .create_if_missing(true),
        );

        let request_config = configuration.request_config;

        let state = AppState {
            search_client: service.client,
            db,
            ranking_score_threshold,
            cache,
            request_config,
        };

        let server = build_server(listener, state);

        tracing::info!("Construyendo la aplicación listo!");

        Ok((Self { port, host, server }, service.bin))
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
    pub async fn run_until_stopped(self, mut meili_bin: Child) -> Result<(), std::io::Error> {
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
                        tracing::info!("Apagando el servidor!");
                        meili_bin.wait().expect("Meilisearch no estaba siendo ejecutado!");
                        tracing::info!("Apagando Meilisearch!");
                    },
                    () = terminate => {
                        tracing::info!("Apagando el servidor!");
                        meili_bin.wait().expect("Meilisearch no estaba siendo ejecutado!");
                        tracing::info!("Apagando Meilisearch!");
                    },
                }
            })
            .await
    }
}

pub fn build_server(listener: tokio::net::TcpListener, state: AppState) -> Serve<Router, Router> {
    // let cors = CorsLayer::new()
    //     .allow_origin("http://0.0.0.0:3000".parse::<HeaderValue>().unwrap())
    //     .allow_methods([Method::GET, Method::POST])
    //     .allow_headers(Any);

    let server = Router::new()
        .route("/health_check", get(health_check))
        // .route("/", get(index))
        .route("/search", get(search))
        .route("/historial", get(get_from_db))
        .nest_service(
            "/",
            ServeDir::new("./dist").not_found_service(ServeFile::new("./fallout.html")),
        )
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
    // .layer(cors);

    axum::serve(listener, server)
}
