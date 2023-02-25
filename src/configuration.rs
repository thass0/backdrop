use serde_aux::field_attributes::deserialize_number_from_string;
use secrecy::Secret;

#[derive(Clone, serde::Deserialize)]
pub struct Settings {
    pub application: ApplicationSettings,
    pub render_worker: RenderWorkerSettings,
    pub redis_uri: Secret<String>,
}

#[derive(Clone, serde::Deserialize)]
pub struct ApplicationSettings {
    pub host: String,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
}

#[derive(Clone, serde::Deserialize)]
pub struct RenderWorkerSettings {
    // Amount of time (in seconds) the render worker spends waiting if the render
    // queue is empty. This is used to decrease the amount of CPU
    // consumtion if the app stays unused on end. The drawback
    // is longer waiting times when starting the first render.
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub laziness: u16,
    // Amount of time (in minutes) until a finished render
    // is deleted again.
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub lifetime: u16,
}

pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String>  for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{} is not a supported enviroment. \
                Use either `local` or `production`.", other
            ))
        }
    }
}


pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let base_path = std::env::current_dir()
        .expect("Failed to determine  the current directory");
    let configuration_directory = base_path.join("configuration");
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT");
    let environment_filename = format!("{}.yaml", environment.as_str());

    let settings = config::Config::builder()
        .add_source(
            config::File::from(configuration_directory.join("base.yaml"))
        )
        .add_source(
            config::File::from(configuration_directory.join(&environment_filename))
        )
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__")
        )
        .build()?;
    settings.try_deserialize::<Settings>()
}
