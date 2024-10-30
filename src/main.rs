use clap::Parser;
use querysense::{
    cli::{Cli, Commands, SyncStrategy},
    configuration, openai, sqlite, startup,
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

    let template = std::env::var("TEMPLATE").map_err(|err| {
        anyhow::anyhow!("Hubo un error al leer la variable de entorno `TEMPLATE` {err}.")
    })?;

    let template = configuration::Template::try_from(template)
        .map_err(|err| anyhow::anyhow!("Hubo un error al parsear el template {err}"))?;

    match cli.command {
        Commands::Serve {
            interface,
            port,
            cache,
        } => {
            let configuration = configuration::ApplicationSettings::new(port, interface, cache);

            tracing::debug!("{:?}", &configuration);
            let rt = tokio::runtime::Runtime::new()?;

            match rt.block_on(startup::run_server(configuration)) {
                Ok(()) => (),
                Err(err) => return Err(err),
            }
        }
        Commands::Sync {
            sync_strat,
            force: hard,
            model,
        } => {
            let db = sqlite::init_sqlite()?;

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
            sqlite::insert_base_data(&db, &template)?;

            match sync_strat {
                SyncStrategy::Fts => sqlite::sync_fts_tnea(&db),
                SyncStrategy::Vector => {
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(sqlite::sync_vec_tnea(&db, model))?;
                }
                SyncStrategy::All => {
                    sqlite::sync_fts_tnea(&db);
                    let rt = tokio::runtime::Runtime::new()?;
                    rt.block_on(sqlite::sync_vec_tnea(&db, model))?;
                }
            }

            tracing::info!(
                "Sincronización finalizada, tomó {} ms",
                start.elapsed().as_millis()
            );
        }
        Commands::Embed { input, model } => match model {
            querysense::cli::Model::OpenAI => {
                let client = reqwest::Client::new();
                let rt = tokio::runtime::Runtime::new()?;
                let output = rt.block_on(openai::embed_single(input, &client))?;
                println!("{output:?}");
            }

            #[cfg(feature = "local")]
            querysense::cli::Model::Local => {
                todo!()
            }
        },
    }

    Ok(())
}
