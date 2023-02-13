use uuid::Uuid;
use once_cell::sync::Lazy;
use sqlx::{PgPool, PgConnection, Connection, Executor};

use api::startup::{Application, get_connection_pool};
use api::configuration::{DatabaseSettings, get_configuration};
use api::telemetry::*;

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_name = "test".to_owned();
    let default_level = "info".to_owned();

    if std::env::var("TEST_LOG").is_ok() {
        init_subscriber(get_subscriber(
            default_name,
            default_level,
            std::io::stdout,
        ));
    } else {
        init_subscriber(get_subscriber(
            default_name,
            default_level,
            std::io::sink
        ));
    }
});


pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub db: PgPool,
    api_client: reqwest::Client,
}

impl TestApp {
    pub async fn spawn() -> Self {
        Lazy::force(&TRACING);

        let configuration = {
            let mut c = get_configuration().expect("Failed to read configuratoin");
            c.database.database_name = Uuid::new_v4().to_string();
            c.application.port = 0;
            c
        };

        Self::configure_database(&configuration.database).await;

        let application = Application::build(configuration.clone())
            .await
            .expect("Failed to build application");
        let application_port = application.port();
        let _ = tokio::spawn(application.run_until_stopped());

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();

        Self {
            address: format!("http://127.0.0.1:{}", application_port),
            port: application_port,
            db: get_connection_pool(&configuration.database).await,
            api_client: client,
        }
    }

    async fn configure_database(db_config: &DatabaseSettings) -> PgPool {
        let mut connection = PgConnection::connect_with(&db_config.without_db())
            .await
            .expect("Failed to connect to postgres");
        connection
            .execute(format!(r#"CREATE DATABASE "{}";"#, db_config.database_name).as_str())
            .await
            .expect("Failed to create database");
        let db_pool = PgPool::connect_with(db_config.with_db())
            .await
            .expect("Failed to connect to postgres");
        sqlx::migrate!("./migrations")
            .run(&db_pool)
            .await
            .expect("Failed to migrate the database");
        db_pool
    }

    pub async fn get_route(&self, r: &str) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/{}", &self.address, r))
            .send()
            .await
            .expect("Failed to execute request")
    }
}
