use clap::{command, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(long = "log-level", default_value = "INFO")]
    pub loglevel: String,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Inicia el cliente web para realizar búsquedas
    Serve,
    /// Actualiza las bases de datos
    Sync {
        /// Fuerza la actualización incluso cuando la base de datos no está vacía.
        #[arg(short, long, default_value = "false")]
        force: bool,
        /// Determina la estrategia para actualizar la base de datos.
        #[arg(value_enum, short, long, default_value_t = SyncStrategy::Fts)]
        sync_strat: SyncStrategy,

        /// Determina si utilizar un modelo local (actualmente es distilBERT) o remoto (Actualmente solo es "text-embedding-3-small").
        #[arg(value_enum, short, long, default_value_t = Model::Local)]
        model: Model,
    },

    Embed {
        /// Input que transformar a un embedding
        #[arg(short, long)]
        input: String,
        /// Determina si utilizar un modelo local (actualmente es distilBERT) o remoto (actualmente solo es "text-embedding-3-small").
        #[arg(value_enum, short, long, default_value_t = Model::Local)]
        model: Model,
    },
}

#[derive(Clone, ValueEnum)]
pub enum SyncStrategy {
    Fts,
    Vector,
    All,
}

#[derive(Clone, ValueEnum)]
pub enum Model {
    OpenAI,
    Local,
}
