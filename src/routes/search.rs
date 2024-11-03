use axum::extract::{Query, State};
use serde::Deserialize;
use tracing::instrument;
use zerocopy::IntoBytes;

use crate::{
    cli::Cache,
    openai,
    routes::SearchStrategy,
    startup::AppState,
    templates::{DisplayableContent, ReRankDisplay, RrfTable, Sexo, Table, TableData, TneaDisplay},
};

#[derive(Deserialize, Debug)]
pub struct Params {
    query: String,
    strategy: SearchStrategy,
    sexo: Sexo,
    edad_min: u64,
    edad_max: u64,
}

#[axum::debug_handler]
#[instrument(name = "Realizando la búsqueda", skip(app))]
pub async fn search(
    Query(params): Query<Params>,
    State(app): State<AppState>,
) -> DisplayableContent {
    match app.cache {
        Cache::Enabled => {
            todo!();
        }
        Cache::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };
    let db = app.db.lock().await;

    let table = match params.strategy {
        SearchStrategy::Fts => {
            let mut statement = match db.prepare(
                "select
                    rank as score, 
                    email, 
                    edad, 
                    sexo, 
                    highlight(fts_tnea, 3, '<b style=\"color: green;\">', '</b>') as template,
                    'fts' as match_type
                from fts_tnea
                where template match :query
                order by rank 
                ",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::Common(Table::default());
                }
            };

            let mut rows = match statement.query_map(&[(":query", &params.query)], |row| {
                let score = row.get::<_, f32>(0).unwrap_or_default() * -1.;
                let email: String = row.get(1).unwrap_or_default();
                let edad: u64 = row.get(2).unwrap_or_default();
                let sexo: Sexo = row.get(3).unwrap_or_default();
                let template: String = row.get(4).unwrap_or_default();
                let match_type: String = row.get(5).unwrap_or_default();

                let data = TneaDisplay::new(email, edad, sexo, template, score, match_type);
                Ok(data)
            }) {
                Ok(rows) => rows
                    .collect::<Result<Vec<TneaDisplay>, _>>()
                    .unwrap_or_default(),
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::Common(Table::default());
                }
            };

            match params.sexo {
                Sexo::U => rows.retain(|x| (params.edad_min..params.edad_max).contains(&x.edad)),
                Sexo::M => rows.retain(|x| {
                    x.sexo == Sexo::M && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
                Sexo::F => rows.retain(|x| {
                    x.sexo == Sexo::F && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
            };

            TableData::Standard(rows)
        }
        SearchStrategy::Semantic => {
            let client = reqwest::Client::new();
            let query_emb = openai::embed_single(params.query.clone(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let mut statement = match db.prepare(
                "
                select
                    vec_tnea.distance,
                    tnea.email,
                    tnea.edad,
                    tnea.sexo,
                    tnea.template,
                    'vec' as match_type
                from vec_tnea
                left join tnea on tnea.id = vec_tnea.row_id
                where template_embedding match :embedding
                and k = 1000
                ",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::Common(Table::default());
                }
            };

            let mut rows =
                match statement.query_map(&[(":embedding", query_emb.as_bytes())], |row| {
                    let score = row.get::<_, f32>(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let edad: u64 = row.get(2).unwrap_or_default();
                    let sexo: Sexo = row.get(3).unwrap_or_default();
                    let template: String = row.get(4).unwrap_or_default();
                    let match_type: String = row.get(5).unwrap_or_default();

                    let data = TneaDisplay::new(email, edad, sexo, template, score, match_type);

                    Ok(data)
                }) {
                    Ok(rows) => rows
                        .collect::<Result<Vec<TneaDisplay>, _>>()
                        .unwrap_or_default(),
                    Err(err) => {
                        tracing::warn!("{}", err);
                        return DisplayableContent::Common(Table::default());
                    }
                };
            rows.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());

            match params.sexo {
                Sexo::U => rows.retain(|x| (params.edad_min..params.edad_max).contains(&x.edad)),
                Sexo::M => rows.retain(|x| {
                    x.sexo == Sexo::M && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
                Sexo::F => rows.retain(|x| {
                    x.sexo == Sexo::F && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
            };
            TableData::Standard(rows)
        }
        SearchStrategy::HybridRrf => {
            let client = reqwest::Client::new();
            let query_emb = openai::embed_single(params.query.clone(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let k: i64 = 1000;
            let weight_vec: f32 = 1.0;
            let weight_fts: f32 = 1.0;
            let rrf_k: i64 = 60;

            let mut statement = match db.prepare(
                "
                with vec_matches as (
                select
                    row_id,
                    row_number() over (order by distance) as rank_number,
                    distance
                from vec_tnea
                where
                    template_embedding match :embedding
                    and k = :k
                ),

                fts_matches as (
                select
                    rowid as row_id,
                    row_number() over (order by rank) as rank_number,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                final as (
                select
                    tnea.template,
                    tnea.email,
                    tnea.edad,
                    tnea.sexo,
                    vec_matches.rank_number as vec_rank,
                    fts_matches.rank_number as fts_rank,
                    (
                        coalesce(1.0 / (:rrf_k + fts_matches.rank_number), 0.0) * :weight_fts +
                        coalesce(1.0 / (:rrf_k + vec_matches.rank_number), 0.0) * :weight_vec
                    ) as combined_rank,
                    vec_matches.distance as vec_distance,
                    fts_matches.score as fts_score
                from fts_matches
                full outer join vec_matches on vec_matches.row_id = fts_matches.row_id
                join tnea on tnea.id = coalesce(fts_matches.row_id, vec_matches.row_id)
                order by combined_rank desc
                )
                select * from final;                
            ",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::RrfTable(RrfTable::default());
                }
            };

            let mut rows = match statement.query_map(
                rusqlite::named_params! { ":embedding": query_emb.as_bytes(), ":query": params.query, ":k": k, ":weight_fts":weight_fts, ":weight_vec":weight_vec ,":rrf_k":rrf_k },
                |row| {
                    let template: String = row.get(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let edad: u64 = row.get(2).unwrap_or_default();
                    let sexo: Sexo = row.get(3).unwrap_or_default();
                    let fts_rank: i64= row.get(4).unwrap_or_default();
                    let vec_rank: i64= row.get(5).unwrap_or_default();
                    let combined_rank: f32 = row.get(6).unwrap_or_default();
                    let vec_score: f32= row.get(7).unwrap_or_default();
                    let fts_score = row.get::<_, f32>(8).unwrap_or_default() * -1.;


                    let data = ReRankDisplay::new(template,email, edad, sexo, fts_rank, vec_rank, combined_rank, vec_score, fts_score);
                    Ok(data)
                },
            ) {
                Ok(rows) => rows
                    .collect::<Result<Vec<ReRankDisplay>, _>>()
                    .unwrap_or_default(),
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::RrfTable(RrfTable::default());
                }
            };
            match params.sexo {
                Sexo::U => rows.retain(|x| (params.edad_min..params.edad_max).contains(&x.edad)),
                Sexo::M => rows.retain(|x| {
                    x.sexo == Sexo::M && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
                Sexo::F => rows.retain(|x| {
                    x.sexo == Sexo::F && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
            };
            TableData::Rrf(rows)
        }
        SearchStrategy::HybridKf => {
            let client = reqwest::Client::new();
            let query_emb = openai::embed_single(params.query.clone(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let k: i64 = 1000;

            let mut statement = match db.prepare(
                "
                with fts_matches as (
                select
                    rowid as row_id,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                vec_matches as (
                select
                    row_id,
                    distance as score
                from vec_tnea
                where
                    template_embedding match :embedding
                    and k = :k
                order by distance
                ),

                combined as (
                select 'fts' as match_type, * from fts_matches
                union all
                select 'vec' as match_type, * from vec_matches
                ),

                final as (
                select distinct
                    tnea.template,
                    tnea.email,
                    tnea.edad,
                    tnea.sexo,
                    combined.score,
                    combined.match_type
                from combined
                left join tnea on tnea.id = combined.row_id
                )
                select * from final;
                ",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::Common(Table::default());
                }
            };

            let mut rows = match statement.query_map(
                rusqlite::named_params! { ":embedding": query_emb.as_bytes(), ":query": params.query, ":k": k},
                |row| {
                    let template: String = row.get(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let edad: u64 = row.get(2).unwrap_or_default();
                    let sexo: Sexo= row.get(3).unwrap_or_default();
                    let score: f32 = row.get(4).unwrap_or_default();
                    let match_type: String = row.get(5).unwrap_or_default();

                    let data = TneaDisplay::new(email, edad, sexo, template, score, match_type);
                    Ok(data)
                },
            ) {
                Ok(rows) => rows
                    .collect::<Result<Vec<TneaDisplay>, _>>()
                    .unwrap_or_default(),
                Err(err) => {
                    tracing::warn!("{}", err);
                        return DisplayableContent::Common(Table::default());
                }
            };
            match params.sexo {
                Sexo::U => rows.retain(|x| (params.edad_min..params.edad_max).contains(&x.edad)),
                Sexo::M => rows.retain(|x| {
                    x.sexo == Sexo::M && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
                Sexo::F => rows.retain(|x| {
                    x.sexo == Sexo::F && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
            };
            TableData::Standard(rows)
        }
        SearchStrategy::HybridReRank => {
            let client = reqwest::Client::new();
            let query_emb = openai::embed_single(params.query.clone(), &client)
                .await
                .map_err(|err| tracing::error!("{err}"))
                .expect("Fallo al crear un embedding del query");

            let k: i64 = 1000;

            let mut statement = match db.prepare(
                "
                with fts_matches as (
                select
                    rowid,
                    rank as score
                from fts_tnea
                where template match :query
                limit :k
                ),

                embeddings AS (
                    SELECT
                        row_id as rowid,
                        template_embedding
                    FROM vec_tnea
                    WHERE row_id IN (SELECT rowid FROM fts_matches)
                ),

                final as (
                select
                    tnea.template,
                    tnea.email,
                    tnea.edad,
                    tnea.sexo,
                    fts_matches.score,
                    'fts' as match_type
                from fts_matches
                left join tnea on tnea.id = fts_matches.rowid
                left join embeddings on embeddings.rowid = fts_matches.rowid
                order by vec_distance_cosine(:embedding, embeddings.template_embedding)
                )
                select * from final;
                ",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    tracing::warn!("{}", err);
                    return DisplayableContent::Common(Table::default());
                }
            };

            let mut rows = match statement.query_map(
                rusqlite::named_params! { ":embedding": query_emb.as_bytes(), ":query": params.query, ":k": k},
                |row| {
                    let template: String = row.get(0).unwrap_or_default();
                    let email: String = row.get(1).unwrap_or_default();
                    let edad: u64= row.get(2).unwrap_or_default();
                    let sexo:Sexo = row.get(3).unwrap_or_default();
                let score = row.get::<_, f32>(4).unwrap_or_default() * -1.;
                    let match_type: String = row.get(5).unwrap_or_default();

                    let data = TneaDisplay::new(email, edad, sexo, template, score, match_type);
                    Ok(data)
                },
            ) {
                Ok(rows) => rows
                    .collect::<Result<Vec<TneaDisplay>, _>>()
                    .unwrap_or_default(),
                Err(err) => {
                    tracing::warn!("{}", err);
                        return DisplayableContent::Common(Table::default());
                }
            };
            match params.sexo {
                Sexo::U => rows.retain(|x| (params.edad_min..params.edad_max).contains(&x.edad)),
                Sexo::M => rows.retain(|x| {
                    x.sexo == Sexo::M && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
                Sexo::F => rows.retain(|x| {
                    x.sexo == Sexo::F && (params.edad_min..params.edad_max).contains(&x.edad)
                }),
            };

            TableData::Standard(rows)
        }
    };

    match table {
        TableData::Standard(vec) => {
            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}` (umbral: {})",
                params.query,
                vec.len(),
                vec.first().map_or_else(Default::default, |d| d.score),
                vec.last().map_or_else(Default::default, |d| d.score),
                -1.0
            );

            DisplayableContent::Common(Table {
                msg: format!("Hay un total de {} resultados.", vec.len()),
                table: vec,
            })
        }
        TableData::Rrf(vec) => {
            tracing::info!(
                "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}` (umbral: {})",
                params.query,
                vec.len(),
                vec.first().map_or_else(Default::default, |d| d.combined_rank),
                vec.last().map_or_else(Default::default, |d| d.combined_rank),
                -1.0
            );

            DisplayableContent::RrfTable(RrfTable {
                msg: format!("Hay un total de {} resultados.", vec.len()),
                table: vec,
            })
        }
    }
}
