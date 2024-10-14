use std::panic;
use tnea_gestion::{configuration::from_configuration, startup::Application};
use tokio::{runtime::Runtime, sync::Mutex, task::futures};
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
    let meili_bin = std::sync::Arc::new(Mutex::new(meili_bin));

    panic::set_hook(Box::new(move |_info| {
        let meili_clone = &meili_bin.clone();
        std::thread::spawn(async move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let child = rt.block_on(meili_clone.clone().lock().await);
            let _ = child.kill();
            let _ = child.wait();
            println!("Subprocess terminated due to panic");
        });
    }));

    tracing::info!(
        "El servidor está funcionando en http://{}:{} !",
        app.host(),
        app.port()
    );

    dbg!("{:?}", config);
    let _ = app.run_until_stopped(meili_bin.clone()).await;

    Ok(())
}
