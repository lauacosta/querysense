use askama_axum::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index;

#[derive(Template)]
#[template(path = "table.html")]
pub struct Table {
    pub msg: String,
    pub table: Vec<TneaDisplay>,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            msg: "No se encontraron ningun registro.".to_string(),
            table: vec![TneaDisplay::default()],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TneaDisplay {
    email: String,
    edad: usize,
    sexo: String,
    template: String,
    pub rank: f32,
}

impl TneaDisplay {
    pub fn new(email: String, edad: usize, sexo: String, template: String, rank: f32) -> Self {
        Self {
            email,
            template,
            edad,
            sexo,
            rank,
        }
    }
}
