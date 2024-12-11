// // Implementado en base a los ejemplos en:
// // https://github.com/huggingface/candle/tree/main/candle-examples/examples/t5

// use std::path::PathBuf;
// use std::sync::{Arc, Mutex};

// use candle_core::Result;
// use candle_core::{
//     utils::{cuda_is_available, metal_is_available},
//     DType, Device, Error, Tensor,
// };
// use candle_nn::VarBuilder;
// use candle_transformers::models::t5;
// use clap::ValueEnum;
// use eyre::{eyre, Error as E};
// use hf_hub::{api::sync::Api, Repo, RepoType};
// use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
// use tokenizers::Tokenizer;
// use tracing::instrument;

// const DTYPE: DType = DType::F32;

// #[derive(Clone, Debug, Copy, ValueEnum)]
// pub enum Model {
//     T5Small,
//     T5Base,
//     T5Large,
//     T5_3B,
// }

// pub struct Args {
//     // Ejecutarlo en la CPU en vez de la GPU.
//     cpu: bool,
//     /// El repositorio del modelo en `HuggingFace`.
//     model_id: Option<String>,
//     revision: Option<String>,
//     model_file: Option<String>,
//     tokenizer_file: Option<String>,
//     config_file: Option<String>,
//     /// El modelo a usar.
//     which: Model,
// }

// impl Args {
//     #[must_use]
//     pub fn new(
//         cpu: bool,
//         model_id: Option<String>,
//         revision: Option<String>,
//         model_file: Option<String>,
//         tokenizer_file: Option<String>,
//         config_file: Option<String>,
//         which: Model,
//     ) -> Self {
//         Self {
//             cpu,
//             model_id,
//             revision,
//             model_file,
//             tokenizer_file,
//             config_file,
//             which,
//         }
//     }
// }

// struct T5ModelBuilder {
//     device: Device,
//     config: t5::Config,
//     weights_filename: Vec<PathBuf>,
// }

// impl T5ModelBuilder {
//     pub fn load(args: &Args) -> eyre::Result<(Self, Tokenizer)> {
//         let device = device(args.cpu)
//             .map_err(|err| eyre!("Ocurrio un error al determinar el dispositivo {err}"))?;
//         let (default_model, default_revision) = match args.which {
//             Model::T5Small => ("t5-small", "refs/pr/15"),
//             _ => return Err(eyre!("Este modelo no se reconoce.")),
//         };
//         let default_model = default_model.to_string();
//         let default_revision = default_revision.to_string();
//         let (model_id, revision) = match (args.model_id.clone(), args.revision.clone()) {
//             (Some(model_id), Some(revision)) => (model_id, revision),
//             (Some(model_id), None) => (model_id, "main".to_string()),
//             (None, Some(revision)) => (default_model, revision),
//             (None, None) => (default_model, default_revision),
//         };

//         let repo = Repo::with_revision(model_id.clone(), RepoType::Model, revision);
//         let api =
//             Api::new().map_err(|err| eyre!("Ocurrio un error al crear la Default Api: {err}"))?;

//         let repo = api.repo(repo);
//         let config_filename = match &args.config_file {
//             None => repo
//                 .get("config.json")
//                 .map_err(|err| eyre!("Ocurrio un error al obtener 'config.json': {err}"))?,
//             Some(f) => f.into(),
//         };
//         let tokenizer_filename = match &args.tokenizer_file {
//             None => repo
//                 .get("tokenizer.json")
//                 .map_err(|err| eyre!("Ocurrio un error al obtener 'tokenizer.json': {err}"))?,
//             Some(f) => f.into(),
//         };
//         let weights_filename = match &args.model_file {
//             Some(f) => f
//                 .split(',')
//                 .map(std::convert::Into::into)
//                 .collect::<Vec<_>>(),
//             None => {
//                 if model_id == "google/flan-t5-xxl" || model_id == "google/flan-ul2" {
//                     hub_load_safetensors(&repo, "model.safetensors.index.json").map_err(|err| {
//                         eyre!("Ocurrio un error al obtener 'model.safetensors.index.json': {err}")
//                     })?
//                 } else {
//                     vec![repo.get("model.safetensors").map_err(|err| {
//                         eyre!("Ocurrio un error al obtener 'model.safetensors': {err}")
//                     })?]
//                 }
//             }
//         };
//         let config = std::fs::read_to_string(config_filename)?;
//         let config: t5::Config = serde_json::from_str(&config)?;
//         let tokenizer = Tokenizer::from_file(tokenizer_filename).map_err(E::msg)?;
//         Ok((
//             Self {
//                 device,
//                 config,
//                 weights_filename,
//             },
//             tokenizer,
//         ))
//     }

//     pub fn build_encoder(&self) -> eyre::Result<t5::T5EncoderModel> {
//         let vb = unsafe {
//             VarBuilder::from_mmaped_safetensors(&self.weights_filename, DTYPE, &self.device)?
//         };
//         Ok(t5::T5EncoderModel::load(vb, &self.config)?)
//     }
// }

// // INFO: Version multi-hilo usando rayon
// #[instrument(name = "Generando Embeddings", skip(templates, args))]
// pub fn create_embeddings(
//     templates: Vec<(u64, String)>,
//     args: Args,
// ) -> eyre::Result<Vec<(u64, Vec<f32>)>> {
//     let start = std::time::Instant::now();

//     tracing::info!("Cargando el modelo...");
//     let (builder, mut tokenizer) = T5ModelBuilder::load(&args)?;
//     let device = &builder.device;
//     let tokenizer = tokenizer
//         .with_padding(None)
//         .with_truncation(None)
//         .map_err(E::msg)?;

//     tracing::info!("Cargando el modelo... listo!");

//     let chunk_size = templates.len() / 10;

//     let embeddings = Arc::new(Mutex::new(Vec::with_capacity(templates.len())));

//     let times_millis = Arc::new(Mutex::new(Vec::with_capacity(templates.len() / chunk_size)));

//     tracing::info!(
//         "Procesando {} templates en bloques de {}...",
//         templates.len(),
//         chunk_size,
//     );

//     let chunks: Vec<_> = templates.chunks(chunk_size).collect();

//     // TODO: Este es el hotpath de la función, tendría que encontrar los cambios para que sea lo mas rápido posible.

//     chunks.par_iter().for_each(|templates_chunk| {
//         let start = std::time::Instant::now();

//         let mut local_embeddings: Vec<(u64, Vec<f32>)> = Vec::with_capacity(templates_chunk.len());

//         for (id, str) in *templates_chunk {
//             let tokens = tokenizer
//                 .encode(str.to_owned(), true)
//                 .map_err(E::msg)
//                 .unwrap()
//                 .get_ids()
//                 .to_vec();

//             let input_token_ids = Tensor::new(&tokens[..], device)
//                 .unwrap()
//                 .unsqueeze(0)
//                 .unwrap();

//             let mut model = builder.build_encoder().unwrap();

//             let embedding: Vec<f32> = {
//                 let emb = &model.forward(&input_token_ids).unwrap();
//                 let emb = normalize_l2(emb).unwrap();
//                 let emb = emb
//                     .mean_keepdim(1)
//                     .unwrap()
//                     .squeeze(0)
//                     .unwrap()
//                     .squeeze(0)
//                     .unwrap();

//                 emb.to_vec1().unwrap()
//             };

//             local_embeddings.push((*id, embedding));
//         }

//         let time = start.elapsed().as_millis();

//         let mut embeddings_guard = embeddings.lock().unwrap();
//         embeddings_guard.extend(local_embeddings);

//         let mut times_guard = times_millis.lock().unwrap();
//         times_guard.push(time);

//         tracing::info!(
//             "Procesar {} templates tomó {:?} ms",
//             templates_chunk.len(),
//             time
//         );
//     });
//     tracing::info!(
//         "Procesar {} templates, en bloques de {} tomó {:?} ms",
//         templates.len(),
//         chunk_size,
//         start.elapsed().as_millis()
//     );

//     let sum_millis: u128 = times_millis.lock().unwrap().iter().sum();
//     let mean = sum_millis as f64 / times_millis.lock().unwrap().len() as f64;

//     tracing::info!(
//         "La media de tiempo por cada {} templates es de: {} ms",
//         chunk_size,
//         mean
//     );
//     let final_embeddings = Arc::try_unwrap(embeddings).unwrap().into_inner().unwrap();

//     Ok(final_embeddings)
// }
// pub fn normalize_l2(v: &Tensor) -> eyre::Result<Tensor> {
//     Ok(v.broadcast_div(&v.sqr()?.sum_keepdim(1)?.sqrt()?)?)
// }

// pub fn device(cpu: bool) -> eyre::Result<Device> {
//     if cpu {
//         Ok(Device::Cpu)
//     } else if cuda_is_available() {
//         Ok(Device::new_cuda(0)?)
//     } else if metal_is_available() {
//         Ok(Device::new_metal(0)?)
//     } else {
//         Ok(Device::Cpu)
//     }
// }

// pub fn hub_load_safetensors(
//     repo: &hf_hub::api::sync::ApiRepo,
//     json_file: &str,
// ) -> eyre::Result<Vec<std::path::PathBuf>> {
//     let json_file = repo.get(json_file).map_err(Error::wrap)?;
//     let json_file = std::fs::File::open(json_file)?;
//     let json: serde_json::Value = serde_json::from_reader(&json_file).map_err(Error::wrap)?;
//     let weight_map = match json.get("weight_map") {
//         None => eyre::bail!("no weight map in {json_file:?}"),
//         Some(serde_json::Value::Object(map)) => map,
//         Some(_) => eyre::bail!("weight map in {json_file:?} is not a map"),
//     };
//     let mut safetensors_files = std::collections::HashSet::new();
//     for value in weight_map.values() {
//         if let Some(file) = value.as_str() {
//             safetensors_files.insert(file.to_string());
//         }
//     }
//     let safetensors_files = safetensors_files
//         .iter()
//         .map(|v| repo.get(v).map_err(Error::wrap))
//         .collect::<Result<Vec<_>>>()?;
//     Ok(safetensors_files)
// }
