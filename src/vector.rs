use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RequestBody {
    pub input: String,
    pub model: String,
    pub encoding_format: Option<EncodingFormat>,
    pub dimensions: Option<u64>,
}

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

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

pub async fn sync_embed(input: String) -> anyhow::Result<Vec<f32>> {
    let client = reqwest::Client::new();
    let request = RequestBody {
        input: input.to_string(),
        model: "text-embedding-3-small".to_string(),
        encoding_format: Some(EncodingFormat::Float),
        dimensions: Some(1536),
    };

    let token = std::env::var("OPENAI_KEY").expect("`OPENAI_KEY debería estar definido en el .env");
    let req_start = std::time::Instant::now();
    tracing::debug!("Enviando request a Open AI...");
    let response = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(token)
        .json(&request)
        .send()
        .await?;

    assert_eq!(response.status().as_u16(), 200);
    tracing::debug!("El request tomó {} ms", req_start.elapsed().as_millis());

    let response: ResponseBody = response.json().await?;

    let embedding = response
        .data
        .first()
        .map(|obj| obj.embedding.clone())
        .ok_or_else(|| anyhow::anyhow!("No se encontró un embedding en la respuesta"));

    tracing::debug!("Embedding generado correctamente!");

    embedding
}

// TODO: Implementar las interfaces para poder realizar batch requests y ahorrar gastos.
// pub async fn batch_embed(input: [&str]) -> anyhow::Result<Vec<Vec<f32>>> {

// }
