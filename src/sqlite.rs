use std::iter::zip;

use rusqlite::{ffi::sqlite3_auto_extension, Connection};
use sqlite_vec::sqlite3_vec_init;
use zerocopy::IntoBytes;

use crate::{
    cli::{self, Model},
    configuration, embeddings, openai,
    utils::{self, TneaData},
};

pub async fn sync_vec_tnea(db: &Connection, model: cli::Model) -> anyhow::Result<()> {
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

    let mut statement =
        db.prepare("insert into vec_tnea(row_id, template_embedding) values (?,?)")?;
    let mut inserted = 0;

    let chunk_size = 2048;

    let templates_chunks: Vec<_> = templates.chunks(chunk_size).map(|v| v.to_vec()).collect();

    tracing::info!("Generando embeddings...");
    for chunk in templates_chunks {
        let results = match model {
            cli::Model::Local => embeddings::create_embeddings(
                chunk,
                embeddings::Args::new(
                    true,
                    Some("t5-small".to_string()),
                    None,
                    None,
                    None,
                    None,
                    embeddings::Model::T5Small,
                ),
            )
            .map_err(|err| {
                anyhow::anyhow!("Algo ocurrió durante la creación de los embeddings: {err}")
            })?,
            cli::Model::OpenAI => {
                let client = reqwest::Client::new();
                let indices: Vec<u64> = chunk.iter().map(|(id, _)| *id).collect();
                let templates: Vec<String> =
                    chunk.into_iter().map(|(_, template)| template).collect();

                let result = openai::embed_vec(templates, &client).await?;

                // https://community.openai.com/t/does-the-index-field-on-an-embedding-response-correlate-to-the-index-of-the-input-text-it-was-generated-from/526099
                zip(indices, result).collect()
            }
        };

        let start = std::time::Instant::now();
        tracing::info!("Insertando nuevas columnas en vec_tnea...");

        db.execute("BEGIN TRANSACTION", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        for (id, embedding) in results {
            tracing::debug!("{id} - {embedding:?}");
            statement.execute(rusqlite::params![id, embedding.as_bytes()])
                .map_err(|err| anyhow::anyhow!(err))
                .expect("Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite");
            inserted += 1;
        }

        db.execute("COMMIT", []).expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

        tracing::info!(
            "Insertando nuevos registros en vec_tnea... se insertaron {} registros, en {} ms",
            inserted,
            start.elapsed().as_millis()
        );
    }

    tracing::info!("Generando embeddings... listo!");

    Ok(())
}

pub fn sync_fts_tnea(db: &Connection) {
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

pub fn init_sqlite() -> anyhow::Result<rusqlite::Connection> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
    let path = std::env::var("DATABASE_URL").map_err(|err| {
        anyhow::anyhow!(
            "La variable de ambiente `DATABASE_URL` no fue encontrada. {}",
            err
        )
    })?;
    Ok(rusqlite::Connection::open(path)?)
}
pub fn setup_sqlite(db: &rusqlite::Connection, model: &Model) -> anyhow::Result<()> {
    let (sqlite_version, vec_version): (String, String) =
        db.query_row("select sqlite_version(), vec_version()", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

    tracing::debug!("sqlite_version={sqlite_version}, vec_version={vec_version}");

    let statement = format!(
        "
        create table if not exists historial (
            id integer primary key,
            query text not null unique,
            result text not null,
            timestamp datetime default current_timestamp
        );

        create index if not exists idx_query_timestamp on historial(query, timestamp);

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

        create table if not exists tnea(
            id integer primary key,
            email text,
            edad integer not null,
            sexo text,
            template text
        );

        create virtual table if not exists fts_tnea using fts5(
            email, edad, sexo, template,
            content='tnea', content_rowid='id'
        );

        {}
        ",
        match model {
            Model::OpenAI => {
                "create virtual table if not exists vec_tnea using vec0(
            row_id integer primary key,
            template_embedding float[1536]
        );"
            }
            Model::Local => {
                "create virtual table if not exists vec_tnea using vec0(
            row_id integer primary key,
            template_embedding float[512]
        );"
            }
        }
    );

    db.execute_batch(&statement)
        .map_err(|err| anyhow::anyhow!(err))
        .expect(
            "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
        );

    Ok(())
}

pub fn insert_base_data(
    db: &rusqlite::Connection,
    template: configuration::Template,
) -> anyhow::Result<()> {
    let num: usize = db.query_row("select count(*) from tnea", [], |row| row.get(0))?;

    // TODO: Añadir la condicion de que caduquen los datos.
    if num != 0 {
        tracing::info!("La tabla `tnea` existe y tiene {num} registros.");
        return Ok(());
    }

    let tnea_data: Vec<TneaData> = utils::parse_and_embed("./csv/", &template)?;

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
        let sql_statement = &template.template;
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

    db.execute("COMMIT", []).expect(
        "Deberia poder ser convertido a un string compatible con C o hubo un error en SQLite",
    );

    Ok(())
}
