use std::{error::Error, path::PathBuf, process::Child, time::Duration};
use tnea_gestion::{configuration::from_configuration, startup::Application};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv()?;
    tracing_subscriber::fmt::init();

    let configuration = from_configuration().expect("Fallo al leer la configuración");
    dbg!("{}", &configuration);

    let meili_bin = match start_meili() {
        Ok(bin) => bin,
        Err(err) => {
            tracing::error!(err);
            std::process::exit(1);
        }
    };

    let app = Application::build(configuration).await?;
    tracing::info!(
        "El servidor está funcionando en http://{}:{} !",
        app.host(),
        app.port()
    );
    let _ = app.run_until_stopped(meili_bin).await;

    Ok(())
}

fn start_meili() -> Result<Child, Box<dyn Error>> {
    let meili_master_key = std::env::var("MEILI_MASTER_KEY")
        .expect("Fallo en encontrar la variable de ambiente `MEILI_MASTER_KEY`");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let meili_path = manifest_dir.join("meilisearch");

    let meilisearch_bin = std::process::Command::new(meili_path)
        .stderr(std::process::Stdio::inherit())
        .args([
            format!("--master-key={meili_master_key}").as_str(),
            "--dump-dir",
            "meili_data/dumps/",
            "--no-analytics",
        ])
        .spawn()?;

    let client = reqwest::blocking::Client::new();
    let tries = 10;

    for i in 0..tries {
        let response = client.get("http://127.0.0.1:7700/health").send();
        match response {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("Meilisearch inició correctamente!");
                    return Ok(meilisearch_bin);
                }
            }
            Err(err) => {
                tracing::error!("{}", err);
                return Err(err.into());
            }
        };
        tracing::info!("Esperando a que Meilisearch inicie... ({i}/{tries})");
        std::thread::sleep(Duration::from_secs(2));
    }

    Err("Meilisearch no inició correctamente".into())
}
