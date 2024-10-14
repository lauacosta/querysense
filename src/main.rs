use tnea_gestion::{configuration::from_configuration, startup::Application};
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;
    let span = setup_tracing();
    let _guard = span.enter();

    let configuration = from_configuration().expect("Fallo al leer la configuración");
    dbg!("{:?}", &configuration);

    let app = Application::build(configuration).await?;

    tracing::info!(
        "El servidor está funcionando en http://{}:{} !",
        app.host(),
        app.port()
    );
    let _ = app.run_until_stopped().await;

    Ok(())
}

pub fn setup_tracing() -> tracing::Span {
    tracing_subscriber::fmt::init();
    tracing::span!(Level::INFO, "main")
}
