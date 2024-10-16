use std::str::FromStr;

use clap::{Parser, Subcommand};
use tnea_gestion::{
    configuration::from_configuration, init_sqlite, parse_and_embed, print_title, setup_sqlite,
    startup::Application, Template, TneaData,
};
use tokio::runtime::Runtime;
use tracing::Level;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "INFO")]
    debug: Option<String>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Serve,
    Sync {
        #[arg(short, long, default_value = "false")]
        hard: bool,
    },
}

fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;

    let cli = Cli::parse();
    let level = match cli.debug.unwrap().to_lowercase().trim() {
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
    tracing_subscriber::fmt()
        .with_max_level(level)
        // .pretty()
        .init();

    let configuration = from_configuration().expect("Fallo al leer la configuración");

    match cli.command {
        Commands::Serve => {
            print_title();

            dbg!("{:?}", &configuration);
            let rt = Runtime::new()?;
            match rt.block_on(run_server(configuration)) {
                Ok(_) => (),
                Err(err) => return Err(err),
            }
        }
        Commands::Sync { hard } => {
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

            let data: Vec<TneaData> = parse_and_embed("./csv/", &template)?;

            tracing::info!(
                "Abriendo transacción para insertar datos en la tabla `tnea_raw` y `tnea`!"
            );

            db.execute("BEGIN TRANSACTION", []).expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

            let mut inserted: usize = 0;
            {
                let mut statement = db.prepare(
                    "
                    insert into tnea_raw (
                        id,
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
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )?;

                for d in &data {
                    let TneaData {
                        id,
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
                    } = d;

                    statement.execute((
                        id,
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
                tracing::info!("Se insertaron {inserted} columnas en tnea_raw!");
            }

            let mut inserted: usize = 0;
            {
                let mut statement = db.prepare(
                    "
                    insert into tnea (
                        id,
                        email,
                        edad,
                        sexo,
                        template
                    ) VALUES (?, ?, ?, ?, ?)",
                )?;

                for d in data {
                    let TneaData {
                        id,
                        email,
                        sexo,
                        fecha_nacimiento,
                        edad,
                        provincia,
                        ciudad,
                        descripcion,
                        estudios,
                        estudios_mas_recientes,
                        experiencia,
                        ..
                    } = d;

                    let template = template
                        .template
                        .replace("{{fecha_nacimiento}}", &fecha_nacimiento)
                        .replace("{{edad}}", &edad.to_string())
                        .replace("{{provincia}}", &provincia)
                        .replace("{{ciudad}}", &ciudad)
                        .replace("{{descripcion}}", &descripcion)
                        .replace("{{estudios}}", &estudios)
                        .replace("{{estudios_mas_recientes}}", &estudios_mas_recientes)
                        .replace("{{experiencia}}", &experiencia);
                    let clean_template = ammonia::clean(&template);

                    statement.execute((id, email, edad, sexo, clean_template))?;

                    inserted += 1;
                }

                tracing::info!("Se insertaron {inserted} columnas en tnea!");
            }

            db.execute("COMMIT", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
            );

            tracing::info!("Insertando nuevas columnas en fts_tnea...");

            db.execute_batch(
                format!(
                    "
                    insert into fts_tnea(rowid, email, edad, sexo, template)
                    select rowid, email, edad, sexo, template
                    from tnea;

                    insert into fts_tnea(fts_tnea) values('optimize');
                "
                )
                .as_str(),
            )
            .map_err(|err| anyhow::anyhow!(err))
            .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

            tracing::info!("Insertando nuevas columnas en fts_tnea... listo!");

            // let num: usize = db.query_row("select count(*) from vec_tnea", [], |row| row.get(0))?;

            // // TODO: Añadir la condicion de que caduquen los datos.
            // if num == 0 {
            //     tracing::info!("Insertando nuevas columnas en vec_tnea...");

            // let mut statement = db
            // .prepare("INSERT INTO vec_tnea(userid, template_embedding) VALUES (?, ?)")
            // .map_err(|err| anyhow::anyhow!(err))
            // .expect(
            // "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
            // );

            // for d in data {
            //     statement.execute(rusqlite::params![d.id,[0.2,0.3]])

            // .map_err(|err| anyhow!(err))
            // .expect(
            //     "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
            // );
            // }
            // tracing::info!("Insertando nuevas columnas en vec_tnea... listo!");
            // }

            // let data: Vec<TneaData> = parse_and_embed("./csv/", &template)?;
        }
    }

    Ok(())
}

async fn run_server(configuration: tnea_gestion::configuration::Settings) -> anyhow::Result<()> {
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
            return Err(e.into());
        }
    }
    Ok(())
}
