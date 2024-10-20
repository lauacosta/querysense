use clap::{Parser, Subcommand, ValueEnum};
use futures::{stream::FuturesUnordered, StreamExt};
use querysense::{
    configuration::from_configuration, init_sqlite, parse_and_embed, print_title, setup_sqlite,
    startup::Application, vector::sync_embed, Template, TneaData,
};
use rusqlite::Connection;
use std::{str::FromStr, sync::Arc};
use tokio::sync::Semaphore;
use tracing::Level;
use zerocopy::IntoBytes;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "INFO")]
    loglevel: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve,
    Sync {
        #[arg(short, long, default_value = "false")]
        hard: bool,
        #[arg(value_enum, short, long, default_value_t = SearchStrategy::All)]
        search_strat: SearchStrategy,
    },
}

#[derive(Clone, ValueEnum)]
enum SearchStrategy {
    Fts,
    Vector,
    All,
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;

    let cli = Cli::parse();
    let level = match cli.loglevel.unwrap().to_lowercase().trim() {
        "trace" | "t" => Level::TRACE,
        "debug" | "d" => Level::DEBUG,
        "info" | "i" => Level::INFO,
        _ => {
            eprintln!(
                "Log Level desconocido, utiliza `INFO`, `DEBUG` o `TRACE`. Usando `INFO` como predeterminado."
            );
            Level::INFO
        }
    };
    tracing_subscriber::fmt().with_max_level(level).init();

    let configuration = from_configuration().expect("Fallo al leer la configuración");

    match cli.command {
        Commands::Serve => {
            print_title();

            tracing::debug!("{:?}", &configuration);
            let rt = tokio::runtime::Runtime::new()?;
            match rt.block_on(run_server(configuration)) {
                Ok(_) => (),
                Err(err) => return Err(err),
            }
        }
        Commands::Sync { search_strat, hard } => {
            let db = init_sqlite()?;
            let template = Template::from_str(&configuration.application.template)?;

            if hard {
                let exists: String = db.query_row(
                    "select name from sqlite_master where type='table' and name=?",
                    ["tnea"],
                    |row| row.get(0),
                )?;

                if !exists.is_empty() {
                    db.execute("drop table tnea", [])?;
                    db.execute("drop table tnea_raw", [])?;
                }
            }
            setup_sqlite(&db)?;

            let num: usize = db.query_row("select count(*) from tnea", [], |row| row.get(0))?;

            // TODO: Añadir la condicion de que caduquen los datos.
            if num != 0 {
                tracing::info!("La tabla `tnea` existe y tiene {num} registros.");
                return Ok(());
            }

            let start = std::time::Instant::now();

            let tnea_data: Vec<TneaData> = parse_and_embed("./csv/", &template)?;

            tracing::info!(
                "Abriendo transacción para insertar datos en la tabla `tnea_raw` y `tnea`!"
            );

            db.execute("BEGIN TRANSACTION", []).expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

            let mut inserted: usize = 0;
            {
                let start = std::time::Instant::now();
                let mut statement = db.prepare(
                    "
                    insert into tnea_raw (
                        email,
                        nombre,
                        sexo,
                        fecha_nacimiento,
                        edad,
                        provincia,
                        ciudad,
                        descripcion,
                        estudios,
                        estudios_mas_recientes,
                        experiencia
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )?;

                for data in &tnea_data {
                    let TneaData {
                        email,
                        nombre,
                        sexo,
                        fecha_nacimiento,
                        edad,
                        provincia,
                        ciudad,
                        descripcion,
                        estudios,
                        estudios_mas_recientes,
                        experiencia,
                    } = data;

                    let clean_html = |str: &str| -> String {
                        if ammonia::is_html(str) {
                            ammonia::clean(str)
                        } else {
                            str.to_string()
                        }
                    };

                    let descripcion = clean_html(descripcion);
                    let estudios = clean_html(estudios);
                    let estudios_mas_recientes = clean_html(estudios_mas_recientes);
                    let experiencia = clean_html(experiencia);

                    statement.execute((
                        email,
                        nombre,
                        sexo,
                        fecha_nacimiento,
                        edad,
                        provincia,
                        ciudad,
                        descripcion,
                        estudios,
                        estudios_mas_recientes,
                        experiencia,
                    ))?;

                    inserted += 1;
                }
                tracing::info!(
                    "Se insertaron {inserted} columnas en tnea_raw! en {} ms",
                    start.elapsed().as_millis()
                );
            }

            {
                let start = std::time::Instant::now();
                let sql_statement = template.template;
                let mut statement = db.prepare(&format!(
                    "
                    insert into tnea (email, edad, sexo, template)
                    select email, edad, sexo, {sql_statement} as template
                    from tnea_raw;
                    "
                ))?;

                let inserted = statement.execute(rusqlite::params![])
                .map_err(|err| anyhow::anyhow!(err))
                .expect("deberia poder ser convertido a un string compatible con c o hubo un error en sqlite");

                tracing::info!(
                    "Se insertaron {inserted} columnas en tnea! en {} ms",
                    start.elapsed().as_millis()
                );
            }

            db.execute("COMMIT", []).expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

            match search_strat {
                SearchStrategy::Fts => sync_fts_tnea(&db),
                SearchStrategy::Vector => {
                    let rt = tokio::runtime::Runtime::new()?;
                    match rt.block_on(sync_vec_tnea(&db)) {
                        Ok(_) => (),
                        Err(err) => return Err(err),
                    }
                }
                SearchStrategy::All => {
                    sync_fts_tnea(&db);
                    let rt = tokio::runtime::Runtime::new()?;
                    match rt.block_on(sync_vec_tnea(&db)) {
                        Ok(_) => (),
                        Err(err) => return Err(err),
                    }
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

async fn sync_vec_tnea(db: &Connection) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let mut statement = db.prepare("select id, template from tnea")?;
    let templates: Vec<(u64, String)> = match statement.query_map([], |row| {
        let id: u64 = row.get(0)?;
        let template: String = row.get::<_, String>(1)?;
        Ok((id, template))
    }) {
        Ok(rows) => rows
            .map(|v| v.expect("Deberia tener un template"))
            .collect(),
        Err(err) => return Err(anyhow::anyhow!(err)),
    };

    let mut template_iter = templates.into_iter();

    tracing::info!("Insertando nuevas columnas en vec_tnea...");

    let mut statement =
        db.prepare("insert into vec_tnea(row_id, template_embedding) values (?,?)")?;

    let semaphore = Arc::new(Semaphore::new(20));
    let batch_size = 49;
    let mut inserted = 0;

    loop {
        let start = std::time::Instant::now();
        let batch = template_iter.by_ref().take(batch_size).collect::<Vec<_>>();

        if batch.is_empty() {
            break;
        }

        db.execute("BEGIN TRANSACTION", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        let mut tasks: FuturesUnordered<_> = batch
            .into_iter()
            .map(|(id, template_str)| {
                let semaphore = semaphore.clone();
                tokio::spawn(async move {
                    let permit = semaphore.acquire().await.unwrap();
                    let result = sync_embed_with_id(template_str, id).await;
                    drop(permit);
                    result
                })
            })
            .collect();

        while let Some(result) = tasks.next().await {
            match result {
                Ok(Ok((embedding, id))) => {
                    statement.execute(rusqlite::params![id, embedding.as_bytes()])
                        .map_err(|err| anyhow::anyhow!(err))
                        .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");
                    inserted += 1;
                    tracing::info!("Insert programado!");
                }
                Ok(Err(err)) => tracing::warn!("Embedding error: {err}"),
                Err(err) => tracing::warn!("Task join error: {err}"),
            }
        }

        db.execute("COMMIT", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        tracing::info!(
            "{} registros tardo {} ms",
            batch_size,
            start.elapsed().as_millis()
        );
    }

    tracing::info!(
        "Insertando nuevos registros en vec_tnea... listo!. Se insertaron {} registros, tomó {} ms",
        inserted,
        start.elapsed().as_millis()
    );

    Ok(())
}

async fn sync_embed_with_id(input: String, id: u64) -> anyhow::Result<(Vec<f32>, u64)> {
    let output = sync_embed(input).await?;
    Ok((output, id))
}

// async fn sync_vec_tnea(db: &Connection, tnea_data: &[TneaData]) -> anyhow::Result<()> {
//     let start = std::time::Instant::now();
//     let mut tasks = Vec::with_capacity(tnea_data.len());

//     tracing::info!("Insertando nuevas columnas en vec_tnea...");
//     let mut inserted = 0;
//     {
//         let mut statement = db.prepare("select id, template from tnea limit 10")?;
//         let templates: Vec<(u64, String)> = match statement.query_map([], |row| {
//             let id: u64 = row.get(0)?;
//             let template: String = row.get::<_, String>(1)?;
//             Ok((id, template))
//         }) {
//             Ok(rows) => rows
//                 .map(|v| v.expect("Deberia tener un template"))
//                 .collect(),
//             Err(err) => return Err(anyhow::anyhow!(err)),
//         };

//         let total = templates.len();

//         let mut iteration = 0;
//         for template in &templates {
//             tracing::debug!("Obteniendo el embedding ({iteration}/{total})");
//             let task = sync_embed(template.1.clone());
//             tasks.push(task);
//             iteration += 1;
//         }

//         tracing::debug!("Ejecutando los requests!");

//         let mut statement =
//             db.prepare("insert into vec_tnea(row_id, template_embedding) values (?,?)")?;

//         let results = future::join_all(tasks).await;

//         for (result, temp) in zip(results, templates) {
//             let id = temp.0;

//             match result {
//                 Ok(res) => {
//                     statement.execute(rusqlite::params![id, res.as_bytes()])
//                     .map_err(|err| anyhow::anyhow!(err))
//                     .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");
//                     inserted += 1;
//                     tracing::debug!("Se ha insertado un registro! ({inserted}/{total})");
//                 }
//                 Err(err) => tracing::warn!("{err}"),
//             }
//         }
//     }
//     tracing::info!(
//         "Insertando nuevos registros en vec_tnea... listo!. Se insertaron {} registros, tomó {} ms",
//         inserted,
//         start.elapsed().as_millis()
//     );
//     Ok(())
// }

fn sync_fts_tnea(db: &Connection) {
    let start = std::time::Instant::now();
    tracing::info!("Insertando nuevos registros en fts_tnea...");
    db.execute_batch(
        "
        insert into fts_tnea(rowid, email, edad, sexo, template)
        select rowid, email, edad, sexo, template
        from tnea;

        insert into fts_tnea(fts_tnea) values('optimize');
        ",
    )
    .map_err(|err| anyhow::anyhow!(err))
    .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

    tracing::info!(
        "Insertando nuevos registros en fts_tnea... listo!. tomó {} ms",
        start.elapsed().as_millis()
    );
}

async fn run_server(configuration: querysense::configuration::Settings) -> anyhow::Result<()> {
    tracing::info!("Iniciando el servidor...");
    match Application::build(configuration).await {
        Ok(app) => {
            tracing::info!(
                "La aplicación está funcionando en http://{}:{} !",
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
