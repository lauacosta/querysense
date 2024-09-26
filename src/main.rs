use tnea_gestion::{configuration::from_configuration, startup::Application};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct Movie {
    id: i64,
    title: String,
    poster: String,
    overview: String,
    release_date: i64,
    genres: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // dotenvy::dotenv()?;
    tracing_subscriber::fmt::init();

    let configuration = from_configuration().expect("Fallo al leer la configuración");
    dbg!("{}", &configuration);

    let app = Application::build(configuration).await?;
    tracing::info!(
        "El servidor está funcionando en http://{}:{} !",
        app.host(),
        app.port()
    );
    let _ = app.run_until_stopped().await;

    Ok(())
}
