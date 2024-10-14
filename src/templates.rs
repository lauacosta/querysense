use askama_axum::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
    pub msg: String,
    pub table: Vec<TneaDisplay>,
}

impl Default for Index {
    fn default() -> Self {
        println!("Aca?");
        Self {
            msg: "".to_string(),
            table: vec![TneaDisplay::default()],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TneaDisplay {
    id: usize,
    email: String,
    nombre: String,
    sexo: String,
    fecha_nacimiento: String,
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
        id: usize,
        email: String,
        nombre: String,
        sexo: String,
        fecha_nacimiento: String,
        edad: usize,
        provincia: String,
        ciudad: String,
        descripcion: String,
        experiencia: String,
        estudios: String,
        estudios_mas_recientes: String,
    ) -> Self {
        Self {
            id,
            email,
            nombre,
            sexo,
            fecha_nacimiento,
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
