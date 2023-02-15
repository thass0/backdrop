use once_cell::sync::Lazy;
use mobc::Pool;
use mobc_redis::{RedisConnectionManager, redis};
use secrecy::{Secret, ExposeSecret};

use backdrop::startup::Application;
use backdrop::configuration::get_configuration;
use backdrop::telemetry::*;

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
    api_client: reqwest::Client,
}

impl TestApp {
    pub async fn spawn() -> Self {
        Lazy::force(&TRACING);

        let configuration = {
            let mut c = get_configuration().expect("Failed to read configuratoin");
            c.application.port = 0;
            c
        };

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
            api_client: client,
        }
    }

    pub async fn get_route(&self, r: &str) -> reqwest::Response {
        self.api_client
            .get(&format!("{}/{}", &self.address, r))
            .send()
            .await
            .expect("Failed to execute request")
    }
}

lazy_static::lazy_static! {
    static ref REDIS_URI: Secret<String> = {
        let configuration = get_configuration().expect("Failed to read configuration");
        configuration.redis_uri
    };
}

pub fn get_redis_pool() -> Pool<RedisConnectionManager> {
    let client = redis::Client::open(REDIS_URI.expose_secret().as_ref()).unwrap();
    let manager = RedisConnectionManager::new(client);
    Pool::builder().max_open(50).build(manager)
}
