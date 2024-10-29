use std::net::IpAddr;

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
    Serve {
        #[clap(short = 'I', long, default_value = "127.0.0.1")]
        interface: IpAddr,

        #[clap(short = 'P', long, default_value_t = 3000)]
        port: u16,

        #[arg(value_enum, short = 'C', long, default_value_t = Cache::Disabled)]
        cache: Cache,
    },
    /// Actualiza las bases de datos
    Sync {
        /// Fuerza la actualización incluso cuando la base de datos no está vacía.
        #[arg(long, default_value = "false")]
        force: bool,
        /// Determina la estrategia para actualizar la base de datos.
        #[arg(value_enum, short = 'S', long, default_value_t = SyncStrategy::Fts)]
        sync_strat: SyncStrategy,

        /// Determina si utilizar un modelo local (actualmente es distilBERT) o remoto (Actualmente solo es "text-embedding-3-small").
        #[arg(value_enum, short = 'M', long, default_value_t = Model::OpenAI)]
        model: Model,
    },

    /// Genera un embedding en base a una input
    Embed {
        /// Input que transformar a un embedding
        #[arg(long)]
        input: String,
        /// Determina si utilizar un modelo local (actualmente es distilBERT) o remoto (actualmente solo es "text-embedding-3-small").
        #[arg(value_enum, long, default_value_t = Model::OpenAI)]
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
    #[cfg(feature = "local")]
    Local,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Cache {
    Enabled,
    Disabled,
}
