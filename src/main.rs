use tnea_gestion::{configuration::from_configuration, startup::Application};
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;
    tracing_subscriber::fmt::init();

    let span = tracing::span!(Level::INFO, "main");
    let _guard = span.enter();
    let configuration = from_configuration().expect("Fallo al leer la configuración");
    let config = configuration.clone();

    let (app, meili_bin) = Application::build(configuration).await?;

    tracing::info!(
        "El servidor está funcionando en http://{}:{} !",
        app.host(),
        app.port()
    );

    dbg!("{:?}", config);

    // FIXME: Spawnear Meilisearch como un subproceso es una mala idea.
    let _ = app.run_until_stopped(meili_bin).await;

    Ok(())
}
