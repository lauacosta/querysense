use axum::{body::Body, http::Request, routing::get, serve::Serve, Router};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::process::Child;
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::{error_span, Level};

use crate::configuration::{FeatureState, RequestConfig};
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

impl Application {
    /// # Errors
    /// Fallará si no logra obtener la direccion local del `tokio::net::TcpListener`.
    ///
    /// # Panics
    /// Entrará en panicos si no es capaz de:
    /// 1. Vincular un `tokio::net::TcpListener` a la dirección dada.
    /// 2. Falla en conectarse con el servidor de `MeiliSearch`.
    pub async fn build(configuration: Settings) -> anyhow::Result<Self> {
        let address = format!(
            "{}:{}",
            configuration.application.host, configuration.application.port
        );
        let listener = match tokio::net::TcpListener::bind(address).await {
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

        let port = listener.local_addr()?.port();
        let host = configuration.application.host;
        let ranking_score_threshold = configuration.request_config.ranking_score_threshold;
        let cache = configuration.application.cache;

        let search_client = configuration
            .search_engine
            .connect_to_meili()
            .await
            .expect("Fallo en conectarse con el servidor de MeiliSearch, es probable que la sesión no haya sido iniciada");

        let db = SqlitePoolOptions::new().connect_lazy_with(
            SqliteConnectOptions::new()
                .filename("./tnea_gestion.db")
                .create_if_missing(true),
        );

        let request_config = configuration.request_config;

        let state = AppState {
            search_client,
            db,
            ranking_score_threshold,
            cache,
            request_config,
        };

        let server = build_server(listener, state);

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
                        meili_bin.wait().unwrap();
                    },
                    () = terminate => {
                        meili_bin.wait().unwrap();
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
