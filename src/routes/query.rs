use axum::{
    extract::{Query, State},
    Json,
};
use meilisearch_sdk::search::SearchResults;
use serde::{Deserialize, Serialize};
use serde_aux::prelude::deserialize_number_from_string;

use crate::{
    configuration::{FeatureState, MAX_HITS},
    startup::AppState,
};

#[derive(Deserialize, Debug)]
pub struct Params {
    query: String,
    doc: String,
}

#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct TneaData {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    id: usize,
    email: Option<String>,
    nombre: Option<String>,
    fecha_nacimiento: Option<String>,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    edad: usize,
    provincia: Option<String>,
    ciudad: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    descripcion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estudios: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    experiencia: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    estudios_mas_recientes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // #[serde(rename = "_rankingScore")]
    ranking_score: Option<f64>,
}

#[axum::debug_handler]
pub(crate) async fn search(
    Query(params): Query<Params>,
    State(app): State<AppState>,
) -> Json<Vec<TneaData>> {
    // Intento encontrar el resultado en el caché si no es más antiguo que un mes.
    // TODO: Escribir el código para el caso en donde tenga que actualizar un registro.
    match app.cache {
        FeatureState::Enabled => {
            if let Ok(record) = sqlx::query!(
        "select result from historial where query=$1 and timestamp > datetime('now', '-1 month');",
        params.query
        )
            .fetch_one(&app.db)
            .await
            {
                let json: Vec<TneaData> = serde_json::from_str(&record.result).unwrap();
                tracing::info!(
                    "Se han extraido el query: `{}` del caché exitosamente!",
                    params.query
                );
                return Json(json);
            }
        }
        FeatureState::Disabled => tracing::info!("El caché se encuentra desactivado!"),
    };
    let client = reqwest::Client::new();
    let response = send_request(
        &params.query,
        &params.doc,
        app.search_client.clone(),
        client.clone(),
    )
    .await;

    let mut max_score: Option<f64> = Some(0.0);
    let json: Vec<TneaData> = response
        .hits
        .into_iter()
        .filter(|v| {
            if v.ranking_score > max_score {
                max_score = v.ranking_score;
            }
            v.ranking_score >= Some(app.ranking_score_threshold)
        })
        .map(|v| {
            let mut result = v.result;
            result.ranking_score = v.ranking_score;
            result
        })
        .collect();

    let json_string =
        serde_json::to_string(&json).expect("Fallo en serializar Vec<TneaData> a String");

    match app.cache {
        FeatureState::Enabled => {
            if let Err(err) = sqlx::query!(
                "insert into historial (query, result) values (?,?)",
                params.query,
                json_string
            )
            .execute(&app.db)
            .await
            {
                tracing::warn!("Fallo al insertar nuevo registro en historial!, {}", err);
                return Json(vec![TneaData::default()]);
            };

            tracing::info!("Registro almacenado en el caché exitosamente!");
        }
        FeatureState::Disabled => tracing::info!("El caché se encuentra desactivado!"),
    };

    tracing::info!(
        "Busqueda para el query: `{}`, exitosa! de {} registros, el mayor puntaje fue: `{}`",
        params.query,
        json.len(),
        max_score.unwrap()
    );

    Json(json)
}

async fn send_request(
    query: &str,
    doc: &str,
    meili_client: meilisearch_sdk::client::Client,
    client: reqwest::Client,
) -> SearchResults<TneaData> {
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct HybridSettings {
        semantic_ratio: f32,
        embedder: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Request {
        #[serde(rename = "q")]
        query: String,
        hybrid: HybridSettings,
        #[serde(skip_serializing_if = "Option::is_none")]
        limit: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        show_ranking_score: Option<bool>,
    }

    let query = Request {
        query: query.into(),
        hybrid: HybridSettings {
            semantic_ratio: 0.5,
            embedder: "default".into(),
        },
        limit: Some(MAX_HITS),
        show_ranking_score: Some(true),
    };

    let response = client
        .post(format!("{}/indexes/{doc}/search", meili_client.get_host()))
        .header(
            "Authorization",
            format!(
                "Bearer {}",
                meili_client
                .get_api_key()
                .expect("Fallo al retirar la API KEY")
            ),
        )
        .header("Content-Type", "application/json")
        .json(&query)
        .send()
        .await
        .expect("Fallo al realizar una búsqueda. Asegurate de que el servidor de Meilli esté funcionando.");

    assert_eq!(response.status().as_u16(), 200);

    let json: SearchResults<TneaData> = response
        .json()
        .await
        .expect("Fallo la deserialización de la respuesta a SearchResults<TneaData>");

    json
}
