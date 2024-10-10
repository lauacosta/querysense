use config::Config;
use meilisearch_sdk::features::ExperimentalFeatures;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use serde_aux::prelude::deserialize_number_from_string;
use serde_json::json;
use std::{
    path::Path,
    sync::{mpsc::channel, RwLock},
    time::Duration,
};

// https://github.com/mehcode/config-rs/blob/master/examples/watch/main.rs
lazy_static::lazy_static! {
    static ref SETTINGS: RwLock<Config> = {
        let base_path = std::env::current_dir().expect("Fallo al determinar el directorio actual");
        let configuration_directory = base_path.join("configuration");

        let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Fallo al parsear APP_ENVIRONMENT.");

        let settings = config::Config::builder()
        .add_source(config::File::from(configuration_directory.join("base")).required(true))
        .add_source(
            config::File::from(configuration_directory.join(environment.as_str()))
            .required(true),
        )
        .add_source(config::Environment::with_prefix("app").separator("__"))
        .build()
        .expect("Fallo al parsear el archivo de configuración.");

        RwLock::new(settings)
    };
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub search_engine: MeiliSettings,
    pub request_config: RequestConfig,
}

#[derive(Debug, Deserialize)]
pub struct ApplicationSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub cache: FeatureState,
}

#[derive(Debug, Deserialize)]
pub struct MeiliSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    // #[serde(deserialize_with = "as_f64")]
    pub master_key: Secret<String>,
    pub settings: InnerSettings,
    pub experimental_features: MeiliExperimentalFeatures,
}

#[derive(Deserialize, Debug, Clone)]
pub struct InnerSettings {
    pagination: PaginationSetting,
    embedders: Embedders,
}

#[derive(Debug, Deserialize)]
pub struct MeiliExperimentalFeatures {
    pub vec_store: FeatureState,
    pub metrics: FeatureState,
    pub logs_route: FeatureState,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct RequestConfig {
    pub hybrid: HybridSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_ranking_score: Option<bool>,
    pub ranking_score_threshold: f64,
    pub show_ranking_score_details: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HybridSettings {
    pub semantic_ratio: f64,
    pub embedder: String,
}

#[derive(Debug, Deserialize, Clone)]
pub enum FeatureState {
    Enabled,
    Disabled,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct PaginationSetting {
    pub max_total_hits: usize,
}

#[derive(Deserialize, Debug, Clone)]
struct Embedders {
    source: EmbedderSource,
    api_key: Secret<String>,
    model: String,

    document_template: Secret<String>,
    dimensions: usize,
}

impl MeiliSettings {
    /// # Errors
    ///
    /// Devolverá error si no es capaz de:
    /// 1. Conectarse a la sesión de `MeiliSearch` utilizando el SDK.
    /// 2. Actualizar la configuración del índice `tnea`.
    ///
    /// # Panics
    ///
    /// Entrará en pánico si no es capaz de:
    /// 1. Conectarse a la sesión de `MeiliSearch` utilizando el SDK.
    /// 2. Actualizar la configuración del índice `tnea`.
    pub async fn connect_to_meili(&self) -> anyhow::Result<meilisearch_sdk::client::Client> {
        let address = format!("http://{}:{}", self.host, self.port);

        let meili_client =
            meilisearch_sdk::client::Client::new(&address, Some(self.master_key.expose_secret()))
                .expect("Fallo al iniciar un cliente con el servidor especificado.");

        let json_settings = {
            match self.experimental_features.vec_store {
                FeatureState::Enabled => {
                    let mut features = ExperimentalFeatures::new(&meili_client);
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
                    let mut features = ExperimentalFeatures::new(&meili_client);
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

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let response = client
            .patch(format!("{address}/indexes/tnea/settings"))
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

        Ok(meili_client)
    }
}

/// # Errors
///
/// Devolverá error si no es capaz de:
/// 1. Determinar el directorio actual.
/// 2. Parsear un `APP_ENVIRONMENT` válido.
/// 3. Parsear correctamente el archivo de configuración `.yml`.
/// 4. Deserializar el archivo de configuracion en `Settings`
///
/// # Panics
///
/// Entrará en pánico si no es capaz de
/// 1. Determinar el directorio actual.
/// 2. Parsear un `APP_ENVIRONMENT` válido.
/// 3. Parsear correctamente el archivo de configuración `.yml`.
/// 4. Deserializar el archivo de configuracion en `Settings`
pub fn from_configuration() -> Result<Settings, config::ConfigError> {
    let settings: Settings = SETTINGS
        .read()
        .expect("Fallo al leer RwLock<Config>")
        .clone()
        .try_deserialize()
        .expect("Fallo al deserializar la configuración en la struct Settings");

    std::thread::spawn(|| {
        let (tx, _rx) = channel();

        let mut watcher: RecommendedWatcher = Watcher::new(
            tx,
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        )
        .unwrap();

        watcher
            .watch(Path::new("configuration"), RecursiveMode::Recursive)
            .unwrap();

        // loop {
        //     match rx.recv() {
        //         Ok(Ok(Event {
        //             kind: notify::event::EventKind::Modify(_),
        //             ..
        //         })) => {
        //             println!(" * Settings.toml written; refreshing configuration ...");
        //             SETTINGS.write().unwrap().refresh().unwrap();
        //             show();
        //         }

        //         Err(e) => println!("watch error: {:?}", e),

        //         _ => {
        //             // Ignore event
        //         }
        //     }
        // }
    });

    Ok(settings)
}

#[derive(Clone, Debug, Deserialize)]
pub enum EmbedderSource {
    OpenAi,
    HuggingFace,
    Ollama,
    Rest,
}

impl EmbedderSource {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAi => "openAi",
            Self::HuggingFace => "huggingFace",
            Self::Ollama => "ollama",
            Self::Rest => "rest",
        }
    }
}

impl TryFrom<String> for EmbedderSource {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "ollama" => Ok(Self::Ollama),
            "openai" => Ok(Self::OpenAi),
            "rest" => Ok(Self::Rest),
            "huggingface" => Ok(Self::HuggingFace),
            other => Err(format!(
                "{other} No es un proveedor soportado, usa 'ollama', 'hugginface', 'openai' o 'rest'",
            )),
        }
    }
}

pub enum Environment {
    Local,
    Production,
}

impl Environment {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} No es un ambiente soportado, usa 'local' o 'production'",
            )),
        }
    }
}
