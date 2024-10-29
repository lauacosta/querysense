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
    pub object: String,
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: TokenUsage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmbeddingObject {
    object: String,
    index: u64,
    embedding: Vec<f32>,
}

impl EmbeddingObject {
    pub fn embeddings_iter(
        objects: impl IntoIterator<Item = Self>,
    ) -> impl Iterator<Item = Vec<f32>> {
        objects.into_iter().map(|obj| obj.embedding)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Serialize, Deserialize)]
pub struct RequestBody {
    pub input: Vec<String>,
    pub model: String,
    pub encoding_format: Option<EncodingFormat>,
    pub dimensions: Option<u64>,
}

#[instrument(name = "Generando Embeddings", skip(input, client))]
pub async fn embed_vec(
    input: Vec<String>,
    client: &reqwest::Client,
) -> anyhow::Result<Vec<Vec<f32>>> {
    let global_start = std::time::Instant::now();

    let request = RequestBody {
        input,
        model: "text-embedding-3-small".to_string(),
        encoding_format: Some(EncodingFormat::Float),
        dimensions: Some(1536),
    };

    let token = std::env::var("OPENAI_KEY").expect("`OPENAI_KEY debería estar definido en el .env");
    let req_start = std::time::Instant::now();
    tracing::info!("Enviando request a Open AI...");
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    tracing::info!("El request tomó {} ms", req_start.elapsed().as_millis());

    let start = std::time::Instant::now();
    let response: ResponseBody = response.json().await?;
    tracing::info!(
        "Deserializar la response a ResponseBody tomó {} ms",
        start.elapsed().as_millis()
    );

    let start = std::time::Instant::now();
    let embedding = EmbeddingObject::embeddings_iter(response.data).collect();
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
pub async fn embed_single(input: String, client: &reqwest::Client) -> anyhow::Result<Vec<f32>> {
    let global_start = std::time::Instant::now();

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

    let token = std::env::var("OPENAI_KEY").expect("`OPENAI_KEY debería estar definido en el .env");
    let req_start = std::time::Instant::now();
    tracing::info!("Enviando request a Open AI...");
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    tracing::info!("El request tomó {} ms", req_start.elapsed().as_millis());

    let start = std::time::Instant::now();
    let response: ResponseBody = response.json().await?;
    tracing::info!(
        "Deserializar la response a ResponseBody tomó {} ms",
        start.elapsed().as_millis()
    );

    let embedding = response
        .data
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
// pub async fn batch_embed(input: [&str]) -> anyhow::Result<Vec<Vec<f32>>> {
// }
