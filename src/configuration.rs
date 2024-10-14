use config::Config;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde_aux::prelude::deserialize_number_from_string;
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

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub application: ApplicationSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApplicationSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
    pub cache: FeatureState,
    pub template: String,
}

#[derive(Debug, Deserialize, Clone)]
pub enum FeatureState {
    Enabled,
    Disabled,
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
#[tracing::instrument]
pub fn from_configuration() -> Result<Settings, config::ConfigError> {
    tracing::info!("Leyendo la configuracion...");
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
        .expect("Falla al crear un nuevo `Watcher`");

        watcher
            .watch(Path::new("configuration"), RecursiveMode::Recursive)
            .expect("Falla al observar el path `/configuration/`");

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

    tracing::info!("Leyendo la configuracion listo!");
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
