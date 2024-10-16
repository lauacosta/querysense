use axum::extract::Path as AxumPath;
use axum::handler::HandlerWithoutStateExt;
use axum::response::IntoResponse;
use std::sync::Arc;

use axum::{body::Body, http::Request, routing::get, serve::Serve, Router};
use http::{header, HeaderMap, StatusCode};
use tokio::signal;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::{error_span, Level};

use crate::configuration::FeatureState;
use crate::init_sqlite;
use crate::routes::search;
use crate::{
    configuration::Settings,
    routes::{get_from_db, health_check, index},
};

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

        tracing::debug!("Definiendo la direccion HTTP...");
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

        tracing::debug!("Definiendo la direccion HTTP listo!");

        let db = Arc::new(Mutex::new(init_sqlite()?));
        let cache = configuration.application.cache;

        let state = AppState { db, cache };

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
        .route("/", get(index))
        .route("/health", get(health_check))
        .route("/search", get(search))
        .route("/historial", get(get_from_db))
        .route("/_assets/*path", get(handle_assets))
        .fallback_service(fallback.into_service())
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

static MAIN_CSS: &str = include_str!("../assets/index.css");
async fn handle_assets(AxumPath(path): AxumPath<String>) -> impl IntoResponse {
    let mut headers = HeaderMap::new();

    if path == "index.css" {
        headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
        (StatusCode::OK, headers, MAIN_CSS)
    } else {
        (StatusCode::NOT_FOUND, headers, "")
    }
}

async fn fallback() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        "404 Not Found. Por favor, revisa la URL.",
    )
}
