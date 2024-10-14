use serde::{Deserialize, Serialize};
use serde_aux::prelude::deserialize_number_from_string;

pub mod configuration;
pub mod routes;
pub mod startup;
pub mod templates;

#[derive(Deserialize, Debug, Serialize, Clone, Default)]
pub struct TneaData {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    id: usize,
    email: Option<String>,
    nombre: Option<String>,
    sexo: Option<String>,
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
    ranking_score: Option<f64>,
}
