use clap::Parser;
use querysense::{
    cli::{Cli, Commands, SyncStrategy},
    configuration, sqlite, startup,
};
use tracing::Level;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    dotenvy::dotenv()?;

    let level = match cli.loglevel.to_lowercase().trim() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        _ => {
            return Err(anyhow::anyhow!(
                "Log Level desconocido, utiliza `INFO`, `DEBUG` o `TRACE`."
            ));
        }
    };

    tracing_subscriber::fmt().with_max_level(level).init();

    let configuration =
        configuration::from_configuration().expect("Fallo al leer la configuración");

    match cli.command {
        Commands::Serve => {
            tracing::debug!("{:?}", &configuration);
            let rt = tokio::runtime::Runtime::new()?;
            match rt.block_on(startup::run_server(configuration)) {
                Ok(_) => (),
                Err(err) => return Err(err),
            }
        }
        Commands::Sync {
            sync_strat,
            force: hard,
            model,
        } => {
            let db = sqlite::init_sqlite()?;
            let template = configuration.application.template;

            if hard {
                let exists: String = db.query_row(
                    "select name from sqlite_master where type='table' and name=?",
                    ["tnea"],
                    |row| row.get(0),
                )?;

                if !exists.is_empty() {
                    db.execute("drop table tnea", [])?;
                    db.execute("drop table tnea_raw", [])?;
                    db.execute("drop table vec_tnea", [])?;
                }
            }

            let start = std::time::Instant::now();

            sqlite::setup_sqlite(&db, &model)?;
            sqlite::insert_base_data(&db, template)?;

            match sync_strat {
                SyncStrategy::Fts => sqlite::sync_fts_tnea(&db),
                SyncStrategy::Vector => {
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(sqlite::sync_vec_tnea(&db, model))?
                }
                SyncStrategy::All => {
                    sqlite::sync_fts_tnea(&db);
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(sqlite::sync_vec_tnea(&db, model))?
                }
            }

            tracing::info!(
                "Sincronización finalizada, tomó {} ms",
                start.elapsed().as_millis()
            );
        }
    }

    Ok(())
}
