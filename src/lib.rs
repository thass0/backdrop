pub mod startup;
pub mod telemetry;
pub mod routes;
pub mod utils;
pub mod configuration;
pub mod render_worker;
pub mod content_length_limit;

pub type RedisPool = mobc::Pool<mobc_redis::RedisConnectionManager>;
pub type RedisConn = mobc::Connection<mobc_redis::RedisConnectionManager>;

// Content of entries which are still unfinished.
pub const PENDING: &str = "pending";
// Redis key for the render queue
pub const RENDER_QUEUE_KEY: &str = "render-worker-queue";
// Redis discard command name (I am afraid I will misspell it otherwise).
const REDIS_DISCARD: &str = "DISCARD";

