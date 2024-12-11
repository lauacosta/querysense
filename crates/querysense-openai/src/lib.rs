use std::{
    env::var,
    time::{Duration, Instant},
};

use eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::instrument;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodingFormat {
    Float,
    Base64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseBody {
    #[serde(rename = "data")]
    pub embeddings: Vec<EmbeddingObject>,
    // pub usage: TokenUsage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmbeddingObject {
    embedding: Vec<f32>,
}

impl EmbeddingObject {
    pub fn embeddings_iter(
        objects: impl IntoIterator<Item = Self>,
    ) -> impl Iterator<Item = Vec<f32>> {
        objects.into_iter().map(|obj| obj.embedding)
    }
}

#[derive(Serialize, Deserialize)]
pub struct RequestBody {
    pub input: Vec<String>,
    pub model: String,
    pub encoding_format: Option<EncodingFormat>,
    pub dimensions: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Request falló: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Rate limit excecido")]
    RateLimit,
    #[error("Maximo número de intentos excedido")]
    MaxRetriesExceeded,
}

async fn request_embeddings(
    client: &Client,
    token: &str,
    request: &RequestBody,
    attempt: u32,
    max_retries: u32,
    time_backoff: u64,
) -> Result<reqwest::Response, EmbeddingError> {
    if attempt > 0 {
        tracing::warn!("Intento {} of {}", attempt, max_retries);
        let delay = Duration::from_millis(1000 * time_backoff.pow(attempt - 1));
        tokio::time::sleep(delay).await;
    }

    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await?;

    match response.status() {
        status if status.is_success() => Ok(response),
        status if status.as_u16() == 429 => {
            if attempt >= max_retries {
                tracing::error!("El maximo numero de intentos fue excedido bajo rate limit");
                Err(EmbeddingError::MaxRetriesExceeded)
            } else {
                tracing::error!("Rate limit excedido, volviendo a intentar...");
                Err(EmbeddingError::RateLimit)
            }
        }
        status => {
            tracing::error!("El request ha fallado con status: {status}");
            Err(EmbeddingError::RequestError(
                response.error_for_status().unwrap_err(),
            ))
        }
    }
}

// https://community.openai.com/t/does-the-index-field-on-an-embedding-response-correlate-to-the-index-of-the-input-text-it-was-generated-from/526099
#[instrument(name = "Generando Embeddings", skip(input, client, indices))]
pub async fn embed_vec(
    indices: Vec<u64>,
    input: Vec<String>,
    client: &Client,
    proc_id: usize,
    time_backoff: u64,
) -> Result<Vec<(u64, Vec<f32>)>> {
    let global_start = Instant::now();

    let request = RequestBody {
        input,
        model: "text-embedding-3-small".to_string(),
        encoding_format: Some(EncodingFormat::Float),
        dimensions: Some(1536),
    };

    let token = var("OPENAI_KEY").expect("`OPENAI_KEY debería estar definido en el .env");

    const MAX_INTENTOS: u32 = 3;
    let mut intento = 0;
    let mut response = None;

    while intento <= MAX_INTENTOS {
        let req_start = Instant::now();
        tracing::info!("Enviando request a Open AI...");
        match request_embeddings(
            client,
            &token,
            &request,
            intento,
            MAX_INTENTOS,
            time_backoff,
        )
        .await
        {
            Ok(resp) => {
                tracing::info!("El request tomó {} ms", req_start.elapsed().as_millis());
                response = Some(resp);
                break;
            }
            Err(EmbeddingError::RateLimit) => {
                intento += 1;
            }
            Err(e) => return Err(e.into()),
        }
    }

    let response = response.ok_or(EmbeddingError::MaxRetriesExceeded)?;

    let start = Instant::now();

    let response: ResponseBody = response.json().await?;

    tracing::info!(
        "Deserializar la response a ResponseBody tomó {} ms",
        start.elapsed().as_millis()
    );

    let start = Instant::now();
    let embedding = std::iter::zip(
        indices,
        EmbeddingObject::embeddings_iter(response.embeddings),
    )
    .collect();
    tracing::info!(
        "La conversión de Vec<EmbeddingObject> a Vec<Vec<f32>> tomó {} ms",
        start.elapsed().as_millis()
    );

    tracing::info!(
        "Embedding generado correctamente! en total tomó {} ms",
        global_start.elapsed().as_millis()
    );
    Ok(embedding)
}

#[instrument(name = "Generando embedding del query", skip(input, client))]
pub async fn embed_single(input: String, client: &Client) -> Result<Vec<f32>> {
    let global_start = Instant::now();

    #[derive(Serialize, Deserialize)]
    pub struct RequestBody {
        pub input: String,
        pub model: String,
        pub encoding_format: Option<EncodingFormat>,
        pub dimensions: Option<u64>,
    }

    let request = RequestBody {
        input,
        model: "text-embedding-3-small".to_string(),
        encoding_format: Some(EncodingFormat::Float),
        dimensions: Some(1536),
    };

    let token = var("OPENAI_KEY").expect("`OPENAI_KEY debería estar definido en el .env");
    let req_start = Instant::now();
    tracing::info!("Enviando request a Open AI...");
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    tracing::info!("El request tomó {} ms", req_start.elapsed().as_millis());

    let start = Instant::now();
    let response: ResponseBody = response.json().await?;
    tracing::info!(
        "Deserializar la response a ResponseBody tomó {} ms",
        start.elapsed().as_millis()
    );

    let embedding = response
        .embeddings
        .into_iter()
        .next()
        .expect("Deberia tener minimo un elemento")
        .embedding;

    tracing::info!(
        "Embedding generado correctamente! en total tomó {} ms",
        global_start.elapsed().as_millis()
    );

    Ok(embedding)
}

// TODO: Implementar las interfaces para poder realizar batch requests y ahorrar gastos.
// pub async fn batch_embed(input: [&str]) -> eyre::Result<Vec<Vec<f32>>> {
// }

#[cfg(test)]
mod tests {
    use super::*;

    // #[test]
    // fn it_works() {
    //     let result = add(2, 2);
    //     assert_eq!(result, 4);
    // }
}
