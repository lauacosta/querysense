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
    },
}

#[derive(Clone, ValueEnum)]
pub enum SyncStrategy {
    Fts,
    Vector,
    All,
}
