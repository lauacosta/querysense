use askama_axum::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub msg: String,
    pub table: Vec<TneaDisplay>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            msg: "".to_string(),
            table: vec![TneaDisplay::default()],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TneaDisplay {
    email: String,
    sexo: String,
    edad: usize,
    provincia: String,
    ciudad: String,
    descripcion: String,
    estudios: String,
    experiencia: String,
    estudios_mas_recientes: String,
}

impl TneaDisplay {
    pub fn new(
        email: String,
        sexo: String,
        edad: usize,
        provincia: String,
        ciudad: String,
        descripcion: String,
        experiencia: String,
        estudios: String,
        estudios_mas_recientes: String,
    ) -> Self {
        Self {
            email,
            sexo,
            edad,
            provincia,
            ciudad,
            descripcion,
            estudios,
            experiencia,
            estudios_mas_recientes,
        }
    }
}
