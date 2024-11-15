use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::StreamExt;
use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use zerocopy::IntoBytes;

use crate::{
    cli::{self, Model},
    configuration, openai,
    routes::ReportError,
    templates::Historial,
    utils::{self, TneaData},
};

#[cfg(feature = "local")]
use crate::embeddings;

pub async fn sync_vec_tnea(db: &Connection, model: cli::Model) -> eyre::Result<()> {
    let mut statement = db.prepare("select id, template from tnea")?;

    let templates: Vec<(u64, String)> = match statement.query_map([], |row| {
        let id: u64 = row.get(0)?;
        let template: String = row.get::<_, String>(1)?;
        Ok((id, template))
    }) {
        Ok(rows) => rows
            .map(|v| v.expect("Deberia tener un template"))
            .collect(),
        Err(err) => return Err(eyre::eyre!(err)),
    };

    let inserted = Arc::new(Mutex::new(0));
    let chunk_size = 2048;

    tracing::info!("Generando embeddings...");

    let client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(5))
        .build()?;

    let jh = templates.chunks(chunk_size).map(|chunk| match model {
        #[cfg(feature = "local")]
        cli::Model::Local => async { Err(eyre!("Local model is unimplemented")) },
        cli::Model::OpenAI => {
            let indices: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
            let templates: Vec<String> =
                chunk.iter().map(|(_, template)| template.clone()).collect();

            openai::embed_vec(indices, templates, &client)
        }
    });

    let stream = futures::stream::iter(jh);

    let start = std::time::Instant::now();
    tracing::info!("Insertando nuevas columnas en vec_tnea...");

    stream.for_each_concurrent(Some(5), |future| {
        let inserted = Arc::clone(&inserted);
        async move {
            match future.await {
                Ok(data) => {
                    let mut statement =
                        db.prepare("insert into vec_tnea(row_id, template_embedding) values (?,?)").unwrap();
                    db.execute("BEGIN TRANSACTION", []).expect(
                        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
                    );
                    for (id, embedding) in data {
                        tracing::debug!("{id} - {embedding:?}");
                        statement.execute(
                            rusqlite::params![id, embedding.as_bytes()],
                        ).expect("Error inserting into vec_tnea");
                        *inserted.lock().unwrap() += 1;
                    }
                    db.execute("COMMIT", []).expect(
                        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
                    );
                }
                Err(err) => eprintln!("Error processing chunk: {}", err),
            }
        }
    }).await;

    tracing::info!(
        "Insertando nuevos registros en vec_tnea... se insertaron {} registros, en {} ms",
        inserted.lock().unwrap(),
        start.elapsed().as_millis()
    );

    tracing::info!("Generando embeddings... listo!");

    Ok(())
}

pub fn sync_fts_tnea(db: &Connection) {
    let start = std::time::Instant::now();
    tracing::info!("Insertando nuevos registros en fts_tnea...");
    db.execute_batch(
        "
        insert into fts_tnea(rowid, email, provincia, ciudad, edad, sexo, template)
        select rowid, email, provincia, ciudad, edad, sexo, template
        from tnea;

        insert into fts_tnea(fts_tnea) values('optimize');
        ",
    )
    .map_err(|err| eyre::eyre!(err))
    .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");

    tracing::info!(
        "Insertando nuevos registros en fts_tnea... listo!. tomó {} ms",
        start.elapsed().as_millis()
    );
}

pub fn init_sqlite() -> eyre::Result<rusqlite::Connection> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    let path = std::env::var("DATABASE_URL").map_err(|err| {
        eyre::eyre!(
            "La variable de ambiente `DATABASE_URL` no fue encontrada. {}",
            err
        )
    })?;
    Ok(rusqlite::Connection::open(path)?)
}
pub fn setup_sqlite(db: &rusqlite::Connection, model: &Model) -> eyre::Result<()> {
    let (sqlite_version, vec_version): (String, String) =
        db.query_row("select sqlite_version(), vec_version()", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

    tracing::debug!("sqlite_version={sqlite_version}, vec_version={vec_version}");

    let statement = format!(
        "
        create table if not exists tnea_raw(
            id integer primary key,
            email text,
            nombre text,
            sexo text,
            fecha_nacimiento text,
            edad integer not null,
            provincia text,
            ciudad text,
            descripcion text,
            estudios text,
            experiencia text,
            estudios_mas_recientes text
        );

        create table if not exists historial(
            id integer primary key,
            query text not null unique,
            timestamp datetime default current_timestamp
        );

        create table if not exists tnea(
            id integer primary key,
            email text,
            provincia text,
            ciudad text,
            edad integer not null,
            sexo text,
            template text
        );

        create virtual table if not exists fts_tnea using fts5(
            email, edad, provincia, ciudad, sexo, template,
            content='tnea', content_rowid='id'
        );

        create virtual table if not exists fts_historial using fts5(
            query,
            content='historial', content_rowid='id'
        );

        create trigger if not exists after_insert_historial
        after insert on historial
        begin
            insert into fts_historial(rowid, query) values (new.id, new.query);
        end;

        create trigger if not exists after_update_historial
        after update on historial
        begin
            update fts_historial set query = new.query where rowid = old.id;
        end;

        create trigger if not exists after_delete_historial
        after delete on historial
        begin
            delete from fts_historial where rowid = old.id;
        end;

        {}
        ",
        match model {
            Model::OpenAI => {
                "create virtual table if not exists vec_tnea using vec0(
                    row_id integer primary key,
                    template_embedding float[1536]
                );"
            }

            #[cfg(feature = "local")]
            Model::Local => {
                "create virtual table if not exists vec_tnea using vec0(
                    row_id integer primary key,
                    template_embedding float[512]
                );"
            }
        }
    );

    db.execute_batch(&statement)
        .map_err(|err| eyre::eyre!(err))
        .expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

    Ok(())
}

pub fn insert_base_data(
    db: &rusqlite::Connection,
    template: &configuration::Template,
) -> eyre::Result<()> {
    let num: usize = db.query_row("select count(*) from tnea", [], |row| row.get(0))?;

    // TODO: Añadir la condicion de que caduquen los datos.
    if num != 0 {
        tracing::info!("La tabla `tnea` existe y tiene {num} registros.");
        return Ok(());
    }

    let tnea_data: Vec<TneaData> = utils::parse_and_embed("./csv/", template)?;

    tracing::info!("Abriendo transacción para insertar datos en la tabla `tnea_raw` y `tnea`!");

    db.execute("BEGIN TRANSACTION", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

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
            let clean_html = |str: &str| -> String {
                if ammonia::is_html(str) {
                    ammonia::clean(str)
                } else {
                    str.to_string()
                }
            };

            let normalize = |str: &str| -> String {
                str.trim_matches(|c| !char::is_ascii_alphabetic(&c))
                    .to_lowercase()
                    .replace("province", "")
            };

            let descripcion = clean_html(&data.descripcion);
            let estudios = clean_html(&data.estudios);
            let estudios_mas_recientes = clean_html(&data.estudios_mas_recientes);
            let experiencia = clean_html(&data.experiencia);

            statement.execute((
                &data.email,
                &data.nombre,
                &data.sexo,
                &data.fecha_nacimiento,
                &data.edad,
                normalize(&data.provincia),
                normalize(&data.ciudad),
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
        let sql_statement = &template.template;
        let mut statement = db.prepare(&format!(
            "
            insert into tnea (email, provincia, ciudad, edad, sexo, template)
            select email, provincia, ciudad, edad, sexo, {sql_statement} as template
            from tnea_raw;
            "
        ))?;

        let inserted = statement.execute(rusqlite::params![])
                .map_err(|err| eyre::eyre!(err))
                .expect("deberia poder ser convertido a un string compatible con c o hubo un error en sqlite");

        tracing::info!(
            "Se insertaron {inserted} columnas en tnea! en {} ms",
            start.elapsed().as_millis()
        );
    }

    db.execute("COMMIT", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

    Ok(())
}

pub fn update_historial(db: &Connection, query: &str) -> eyre::Result<(), ReportError> {
    match db.execute(
        "insert or replace into historial(query) values (?)",
        [query],
    ) {
        Ok(updated) => {
            tracing::info!("{} registros fueron añadidos al historial!", updated);
        }
        Err(err) => {
            tracing::error!("{}", err);
            return Err(ReportError(err.into()));
        }
    }

    Ok(())
}

pub fn get_historial(db: &Connection) -> eyre::Result<Vec<Historial>, ReportError> {
    let mut statement = match db.prepare("select id, query from historial order by timestamp desc")
    {
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::error!("{}", err);
            return Err(ReportError(err.into()));
        }
    };

    let rows = match statement.query_map([], |row| {
        let id: u64 = row.get(0).unwrap_or_default();
        let query: String = row.get(1).unwrap_or_default();

        let data = Historial::new(id, query);

        Ok(data)
    }) {
        Ok(rows) => rows
            .collect::<Result<Vec<Historial>, _>>()
            .unwrap_or_default(),
        Err(err) => {
            tracing::error!("{}", err);
            return Err(ReportError(err.into()));
        }
    };

    Ok(rows)
}
