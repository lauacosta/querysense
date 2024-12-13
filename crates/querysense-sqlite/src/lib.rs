use std::{
    fs::File,
    io::BufReader,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use futures::StreamExt;
use querysense_common::{parse_sources, DataSources, HttpError, TneaData};
use querysense_configuration::Template;
use querysense_openai::embed_vec;
use querysense_ui::Historial;
use rusqlite::{Connection, ffi::sqlite3_auto_extension};
use sqlite_vec::sqlite3_vec_init;
use zerocopy::IntoBytes;

pub async fn sync_vec_tnea(
    db: &Connection,
    // model: Model,
    base_delay: u64,
) -> eyre::Result<()> {
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

    let chunk_size = 2048;

    tracing::info!("Generando embeddings...");

    let client = reqwest::ClientBuilder::new()
        .deflate(true)
        .gzip(true)
        .build()?;

    let jh = templates
        .chunks(chunk_size)
        .enumerate()
        .map(|(proc_id, chunk)| 
        //     match model {
            // Model::OpenAI =>
            {
                let indices: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
                let templates: Vec<String> =
                    chunk.iter().map(|(_, template)| template.clone()).collect();
                embed_vec(indices, templates, &client, proc_id, base_delay)
            // }
            // Model::Local => todo!(),
        });

    let stream = futures::stream::iter(jh);

    let start = std::time::Instant::now();
    tracing::info!("Insertando nuevas columnas en vec_tnea...");

    let total_inserted = Arc::new(AtomicUsize::new(0));

    stream.for_each_concurrent(Some(5), |future| {
        let total_inserted = total_inserted.clone();
        async move {
            match future.await {
                Ok(data) => {
                    let mut statement =
                        db.prepare("insert into vec_tnea(row_id, template_embedding) values (?,?)").unwrap();

                    db.execute("BEGIN TRANSACTION", []).expect(
                        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
                    );
                    let mut insertions = 0;
                    for (id, embedding) in data {
                        // tracing::debug!("{id} - {embedding:?}");
                        insertions += statement.execute(
                            rusqlite::params![id, embedding.as_bytes()],
                        ).expect("Error insertando en vec_tnea");

                    }
                    db.execute("COMMIT", []).expect(
                        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
                    );

                    total_inserted.fetch_add(insertions, Ordering::Relaxed);
                }
                Err(err) => tracing::error!("Error procesando el chunk: {}", err),
            }
        }
    }).await;

    tracing::info!(
        "Insertando nuevos registros en vec_tnea... se insertaron {} registros, en {} ms",
        total_inserted.load(Ordering::Relaxed),
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

pub fn init_sqlite() -> eyre::Result<String> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    let path = std::env::var("DATABASE_URL").map_err(|err| {
        eyre::eyre!(
            "La variable de ambiente `DATABASE_URL` no fue encontrada. {}",
            err
        )
    })?;
    Ok(path)
}

pub fn setup_sqlite(
    db: &rusqlite::Connection,
    // model: &Model
) -> eyre::Result<()> {
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
        // match model {
        //     Model::OpenAI => {
        "create virtual table if not exists vec_tnea using vec0(
                    row_id integer primary key,
                    template_embedding float[1536]
                );" // }

                    // Model::Local => {
                    //     // todo!()
                    //     // "create virtual table if not exists vec_tnea using vec0(
                    //     //     row_id integer primary key,
                    //     //     template_embedding float[512]
                    //     // );"
                    // }
                    // }
    );

    db.execute_batch(&statement)
        .map_err(|err| eyre::eyre!(err))
        .expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

    Ok(())
}

pub fn insert_base_data(db: &rusqlite::Connection, template: &Template) -> eyre::Result<()> {
    let num: usize = db.query_row("select count(*) from tnea", [], |row| row.get(0))?;
    if num != 0 {
        tracing::info!("La tabla `tnea` existe y tiene {num} registros.");
        return Ok(());
    }

    let start = std::time::Instant::now();
    let inserted = parse_and_insert("./datasources/", template, db)?;
    tracing::info!(
        "Se insertaron {inserted} columnas en tnea_raw! en {} ms",
        start.elapsed().as_millis()
    );

    let start = std::time::Instant::now();
    db.execute("BEGIN TRANSACTION", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

    let sql_statement = &template.template;
    let mut statement = db.prepare(&format!(
        "
        insert into tnea (email, provincia, ciudad, edad, sexo, template)
        select email, provincia, ciudad, edad, sexo, {sql_statement} as template
        from tnea_raw;
        "
    ))?;

    let inserted = statement
        .execute(rusqlite::params![])
        .map_err(|err| eyre::eyre!(err))
        .expect(
            "deberia poder ser convertido a un string compatible con c o hubo un error en sqlite",
        );

    tracing::info!(
        "Se insertaron {inserted} columnas en tnea! en {} ms",
        start.elapsed().as_millis()
    );

    db.execute("COMMIT", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

    Ok(())
}

fn parse_and_insert(
    path: impl AsRef<Path>,
    template: &Template,
    db: &Connection,
) -> eyre::Result<usize> {
    let mut inserted = 0;
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

    let datasources = parse_sources(path)?;
    for (source, ext) in datasources {
        tracing::info!("Leyendo {source:?}...");

        let data = match ext {
            DataSources::Csv => {
                let mut reader_config = csv::ReaderBuilder::new();
                let mut reader = reader_config
                    .flexible(true)
                    .trim(csv::Trim::All)
                    .has_headers(true)
                    .quote(b'"')
                    .from_path(&source)?;

                let headers: Vec<String> = reader
                    .headers()?
                    .into_iter()
                    .map(std::string::ToString::to_string)
                    .collect();

                for field in &template.fields {
                    if !headers.contains(field) {
                        return Err(eyre::eyre!(
                            "El archivo {source:?} no tiene el header {field}.",
                        ));
                    }
                }

                reader
                .deserialize()
                .collect::<Result<Vec<TneaData>, csv::Error>>()
                .map_err(|err| eyre::eyre!("{source:?} no pudo ser deserializado. Hay que controlar que tenga los headers correctos. Err: {err}"))?
            }
            DataSources::Json => {
                let file = File::open(&source)?;
                let reader = BufReader::new(file);

                serde_json::from_reader::<_, Vec<TneaData>>(reader)?
            }
        };
        let total_registros = data.len();

        tracing::info!("Abriendo transacción para insertar datos en la tabla `tnea_raw` y `tnea`!");
        db.execute("BEGIN TRANSACTION", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        for d in data.into_iter() {
            statement.execute((
                &d.email,
                &d.nombre,
                &d.sexo,
                &d.fecha_nacimiento,
                &d.edad,
                normalize(&d.provincia),
                normalize(&d.ciudad),
                clean_html(d.descripcion),
                clean_html(d.estudios),
                clean_html(d.estudios_mas_recientes),
                clean_html(d.experiencia),
            ))?;

            inserted += 1;
        }

        db.execute("COMMIT", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        tracing::info!(
            "Leyendo {source:?}... listo! - {} nuevos registros",
            total_registros,
        );
    }

    Ok(inserted)
}

pub fn update_historial(db: &Connection, query: &str) -> eyre::Result<(), HttpError> {
    let updated = db.execute("insert or replace into historial(query) values (?)", [
        query,
    ])?;
    tracing::info!("{} registros fueron añadidos al historial!", updated);

    Ok(())
}

pub fn get_historial(db: &Connection) -> eyre::Result<Vec<Historial>, HttpError> {
    let mut statement = db.prepare("select id, query from historial order by timestamp desc")?;

    let rows = statement.query_map([], |row| {
        let id: u64 = row.get(0).unwrap_or_default();
        let query: String = row.get(1).unwrap_or_default();

        let data = Historial::new(id, query);

        Ok(data)
    })?.collect::<Result<Vec<Historial>, _>>()?;

    Ok(rows)
}

#[inline]
pub fn normalize(str: &str) -> String {
    str.trim_matches(|c| !char::is_ascii_alphabetic(&c))
        .trim()
        .to_lowercase()
        .replace("province", "")
}

#[inline]
pub fn clean_html(str: String) -> String {
    if ammonia::is_html(&str) {
        ammonia::clean(&str)
    } else {
        str
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn it_works() {
    //     let result = add(2, 2);
    //     assert_eq!(result, 4);
    // }
}
