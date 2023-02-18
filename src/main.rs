use backdrop::startup::Application;
use backdrop::configuration::get_configuration;
use backdrop::telemetry::*;
use backdrop::render_worker;

use tokio::task::JoinError;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_subscriber(get_subscriber(
        "backdrop".into(),
        "info".into(),
        std::io::stdout,
    ));

    let configuration = get_configuration().expect("Failed to read configuration");
    let application = Application::build(configuration.clone()).await?;
    let application_task = tokio::spawn(application.run_until_stopped());
    let worker_task= tokio::spawn(render_worker::run_until_stopped(configuration));

    tokio::select!(
        o = application_task => report_exit("API", o),
        o = worker_task => report_exit("Render worker", o),
    );
    
    Ok(())
}

fn report_exit(
    task_name: &str,
    outcome: Result<Result<(), impl std::fmt::Debug + std::fmt::Display>, JoinError>,
) {
    match outcome {
        Ok(Ok(())) => tracing::info!("{task_name} has exited"),
        Ok(Err(e)) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{task_name} failed",
            )
        },
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "'{task_name}' task failed to complete",
            )
        }
    }
}
