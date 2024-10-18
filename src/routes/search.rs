use axum::extract::{Query, State};
use serde::Deserialize;

use crate::{
    configuration::FeatureState,
    startup::AppState,
    templates::{Table, TneaDisplay},
};

#[derive(Deserialize, Debug)]
pub struct Params {
    query: String,
    // filtros: Option<Vec<String>>,
}

#[axum::debug_handler]
pub async fn search(Query(params): Query<Params>, State(app): State<AppState>) -> Table {
    // Intento encontrar el resultado en el caché si no es más antiguo que un mes.
    // TODO: Escribir el código para el caso en donde tenga que actualizar un registro.
    match app.cache {
        FeatureState::Enabled => {
            todo!();
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };
    let db = app.db.lock().await;
    let mut statement = match db.prepare(
        "select
            rank, 
            email, 
            edad, 
            sexo, 
            highlight(fts_tnea, 3, '<b style=\"color: green;\">', '</b>') as template
        from fts_tnea
        where template match :query
        order by rank 
        ",
    ) {
        //     where rank > ?;
        Ok(stmt) => stmt,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let rows = match statement.query_map(&[(":query", &params.query)], |row| {
        let rank = row.get(0).unwrap_or_default();
        let email = row.get(1).unwrap_or_default();
        let edad = row.get(2).unwrap_or_default();
        let sexo = row.get(3).unwrap_or_default();
        let template = row.get(4).unwrap_or_default();

        let data = TneaDisplay::new(email, sexo, edad, template, rank);

        Ok(data)
    }) {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!("{}", err);
            return Table::default();
        }
    };

    let mut table = Vec::new();
    for row in rows {
        match row {
            Ok(r) => table.push(r),
            Err(err) => {
                tracing::warn!("{}", err);
                return Table::default();
            }
        };
    }

    tracing::info!(
        "Busqueda para el query: `{}`, exitosa! de {} registros, el mejor puntaje fue: `{}` y el peor fue: `{}` (umbral: {})",
        params.query,
        table.len(),
        table.first().unwrap().rank,
        table.last().unwrap().rank,
        -1.0
    );

    match app.cache {
        FeatureState::Enabled => {
            todo!();
            //     let json_string =
            //         serde_json::to_string(&json).expect("Fallo en serializar Vec<TneaData> a String");

            //     if let Err(err) = sqlx::query!(
            //         "insert into historial (query, result) values (?,?)",
            //         params.query,
            //         json_string
            //     )
            //     .execute(&app.db)
            //     .await
            //     {
            //         tracing::warn!("Fallo al insertar nuevo registro en historial!, {}", err);
            //         return Json(vec![TneaData::default()]);
            //     };

            //     tracing::info!("Registro almacenado en el caché exitosamente!");
        }
        FeatureState::Disabled => tracing::debug!("El caché se encuentra desactivado!"),
    };

    Table {
        msg: format!("Hay un total de {} resultados.", table.len()),
        table,
    }
}

// async fn send_request(
//     query: &str,
//     doc: &str,
//     meili_client: meilisearch_sdk::client::Client,
//     client: reqwest::Client,
//     request_config: RequestConfig,
// ) -> SearchResults<TneaData> {
//     // TODO: Ver como puedo evitar hacer esto.

//     #[derive(Serialize, Debug)]
//     #[serde(rename_all = "camelCase")]
//     struct HybridBody {
//         semantic_ratio: f64,
//         embedder: String,
//     }

//     #[derive(Serialize, Debug)]
//     #[serde(rename_all = "camelCase")]
//     struct RequestBody {
//         #[serde(rename = "q")]
//         query: String,
//         pub hybrid: HybridBody,
//         #[serde(skip_serializing_if = "Option::is_none")]
//         pub limit: Option<usize>,
//         #[serde(skip_serializing_if = "Option::is_none")]
//         pub show_ranking_score: Option<bool>,
//         pub ranking_score_threshold: f64,
//         pub show_ranking_score_details: Option<bool>,
//     }

//     let request = RequestBody {
//         query: query.to_string(),
//         hybrid: HybridBody {
//             semantic_ratio: request_config.hybrid.semantic_ratio,
//             embedder: request_config.hybrid.embedder,
//         },
//         limit: request_config.limit,
//         show_ranking_score: request_config.show_ranking_score,
//         ranking_score_threshold: request_config.ranking_score_threshold,
//         show_ranking_score_details: request_config.show_ranking_score_details,
//     };

//     dbg!("{:?}", &request);

//     let response = client
//         .post(format!("{}/indexes/{doc}/search", meili_client.get_host()))
//         .header(
//             "Authorization",
//             format!(
//                 "Bearer {}",
//                 meili_client
//                 .get_api_key()
//                 .expect("Fallo al retirar la API KEY")
//             ),
//         )
//         .header("Content-Type", "application/json")
//         .json(&request)
//         .send()
//         .await
//         .expect("Fallo al realizar una búsqueda. Asegurate de que el servidor de Meilli esté funcionando.");

//     assert_eq!(response.status().as_u16(), 200);

//     response
//         .json()
//         .await
//         .expect("Fallo la deserialización de la respuesta a SearchResults<TneaData>")
// }
