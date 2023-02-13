use backdrop::startup::Application;
use backdrop::configuration::get_configuration;
use backdrop::telemetry::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_subscriber(get_subscriber(
        "backdrop".into(),
        "info".into(),
        std::io::stdout,
    ));

    let configuration = get_configuration().expect("Failed to read configuration");
    let application = Application::build(configuration).await?;
    application.run_until_stopped().await?;
    Ok(())
}
