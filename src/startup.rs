use axum::handler::HandlerWithoutStateExt;
use axum::Extension;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{body::Body, http::Request, routing::get, serve::Serve, Router};
use tokio::signal;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tower_request_id::{RequestId, RequestIdLayer};
use tracing::{error_span, instrument, Level};

use crate::cli::Cache;
use crate::configuration::{self, ApplicationSettings};
use crate::routes;
use crate::sqlite::init_sqlite;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Arc<Mutex<rusqlite::Connection>>,
    pub cache: Cache,
}

#[derive(Debug)]
pub struct Application {
    pub port: u16,
    pub host: IpAddr,
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
    pub async fn build(configuration: ApplicationSettings) -> eyre::Result<Self> {
        let address = format!("{}:{}", configuration.host, configuration.port);

        let listener = match tokio::net::TcpListener::bind(&address).await {
            Ok(listener) => listener,
            Err(err) => {
                tracing::error!("{err}. Tratando con otro puerto...");
                match tokio::net::TcpListener::bind(format!("{}:0", configuration.host)).await {
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

        let host = configuration.host;

        let db = Arc::new(Mutex::new(init_sqlite()?));
        let cache = configuration.cache;

        let state = AppState { db, cache };

        let server = build_server(listener, state)?;

        Ok(Self { port, host, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn host(&self) -> String {
        self.host.to_string()
    }

    /// # Errors
    ///
    /// Devolverá error si ocurre algun inconveniente con tokio para programar la tarea asíncrona.
    /// # Panics
    ///
    /// Entrará en pánico si no es capaz de instalar el handler requerido.
    #[tracing::instrument(skip(self))]
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

pub fn build_server(
    listener: tokio::net::TcpListener,
    state: AppState,
) -> eyre::Result<Serve<Router, Router>> {
    let server = Router::new()
        .route("/", get(routes::index))
        .route("/health", get(routes::health_check))
        .route("/search", get(routes::search))
        .route("/historial", get(routes::get_from_db))
        .route("/_assets/*path", get(routes::handle_assets))
        .fallback_service(routes::fallback.into_service())
        .with_state(state)
        .layer(Extension(
            reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(5))
                .build()?,
        ))
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

    Ok(axum::serve(listener, server))
}

#[instrument(skip(configuration))]
pub async fn run_server(configuration: configuration::ApplicationSettings) -> eyre::Result<()> {
    match Application::build(configuration).await {
        Ok(app) => {
            tracing::info!(
                "La aplicación está disponible en http://{}:{}.",
                app.host(),
                app.port()
            );
            if let Err(e) = app.run_until_stopped().await {
                tracing::error!("Error ejecutando el servidor HTTP: {:?}", e);
                return Err(e.into());
            }
        }
        Err(e) => {
            tracing::error!("Fallo al iniciar el servidor: {:?}", e);
            return Err(e);
        }
    }
    Ok(())
}
