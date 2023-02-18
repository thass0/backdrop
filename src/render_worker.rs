use std::time::Duration;
use anyhow::Context;
use mobc_redis::redis::AsyncCommands;

use crate::configuration::Settings;
use crate::startup::get_redis_pool;
use crate::{RedisPool, RENDER_QUEUE_KEY};
use crate::routes::RenderTask;

pub async fn run_until_stopped(configuration: Settings) -> anyhow::Result<()> {
    let redis_pool = get_redis_pool(configuration.redis_uri).await?;
    worker_loop(redis_pool).await
}

async fn worker_loop(redis_pool: RedisPool) -> anyhow::Result<()> {
    loop {
        match try_render_task(&redis_pool).await {
            Ok(()) => {},
            Err(_) => {
                // Wait and try again.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn try_render_task(
    redis_pool: &RedisPool,
) -> anyhow::Result<()> {
    // Get the the task which was inserted first from the queue.
    let task: RenderTask = {
        let mut conn = redis_pool.get().await
            .context("Render worker failed to acquire redis connection")?;
        let task_ser: String = conn.rpop(RENDER_QUEUE_KEY, None).await
            .context("Render worker failed to query task from redis")?;
        serde_json::from_str(&task_ser)
            .context("Render worker failed to deserialized task")
    }?;

    // TODO: Spawn a process to render the task.
    
    Ok(())
}
