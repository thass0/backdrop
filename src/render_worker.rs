use std::time::Duration;
use anyhow::Context;
use mobc_redis::redis::AsyncCommands;
use std::process::{Command, Stdio};
use std::fs;
use std::io::Read;
use uuid::Uuid;

use crate::configuration::Settings;
use crate::startup::get_redis_pool;
use crate::{RedisPool, RENDER_QUEUE_KEY};
use crate::routes::RenderTask;
use crate::utils::spawn_blocking_with_tracing;

pub async fn run_until_stopped(configuration: Settings) -> anyhow::Result<()> {
    let redis_pool = get_redis_pool(configuration.redis_uri).await?;
    worker_loop(redis_pool).await
}

async fn worker_loop(redis_pool: RedisPool) -> anyhow::Result<()> {
    loop {
        match try_render_task(&redis_pool).await {
            Ok(ExecutionOutcome::TaskCompleted) => {},
            Ok(ExecutionOutcome::EmptyQueue) => {
                // Wait for queue to fill up.
                tokio::time::sleep(Duration::from_secs(10)).await;
            },
            Err(e) => {
                // Wait and try again.
                tracing::error!("Render worker error: {e:?}");
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

enum ExecutionOutcome {
    EmptyQueue,
    TaskCompleted,
}

const AUDIO_FILE: &str = "audio.mp3";
const IMAGE_FILE: &str = "image.jpg";
const VIDEO_FILE: &str = "video.mp4";

async fn try_render_task(
    redis_pool: &RedisPool,
) -> anyhow::Result<ExecutionOutcome> {
    let mut conn = redis_pool.get().await
        .context("render worker failed to acquire redis connection")?;

    // Get the the task which was inserted first from the queue.
    let task: RenderTask = {
        let task_ser: String = match conn.rpop(RENDER_QUEUE_KEY, None).await {
            Ok(ser) => ser,
            // Make caller wait while the queue is empty.
            Err(_) => return Ok(ExecutionOutcome::EmptyQueue),
        };
        serde_json::from_str(&task_ser)
            .context("failed to deserialized task")
    }?;

    tracing::trace!("Received render task: {task:?}");

    // TODO: Remove copying data between redis and files.

    let audio_data: Vec<u8> = conn.get(task.audio.to_string()).await
        .context("failed to query audio data")?;
    // For some reason ffprobe does not  accept the data when
    // it's piped. Therefore it's written to a file first.
    fs::write(AUDIO_FILE, &audio_data)
        .context("failed to write audio")?;    

    let image_data: Vec<u8> = conn.get(task.image.to_string()).await
        .context("failed to query image data")?;
    fs::write(IMAGE_FILE, &image_data)
        .context("failed to write image")?;

    tracing::trace!("Starting rendering");

    let audio_len = audio_len().await?;
    tracing::trace!("audio length: {audio_len}");
    render_video(audio_len).await?;

    // Write the finished video to the target ID's location.
    let mut video_file = tokio::task::spawn_blocking(move || {
        fs::File::open(VIDEO_FILE)
    })
    .await?
    .context("failed to reopen video file")?;

    tracing::info!("Finished rendering {0}", task.target);
    
    // Allocate 1MB;
    let mut video_data: Vec<u8> = Vec::with_capacity(1<<20); 
    video_file.read_to_end(&mut video_data)
        .context("failed to read video file")?;

    let video_id = Uuid::new_v4().to_string();
    conn.set(&video_id, &video_data).await
        .context("failed to set video data in db")?;
    conn.set(&task.target.to_string(), &video_id).await
        .context("failed to set video target id")?;

    tracing::trace!("Set to not pending");

    Ok(ExecutionOutcome::TaskCompleted)
}

async fn audio_len() -> anyhow::Result<String> {
    let output = spawn_blocking_with_tracing(move || {
        // Get the length of the input audio file.
        Command::new("ffprobe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .arg("-show_entries")
            .arg("format=duration")
            .arg("-of")
            .arg("default=noprint_wrappers=1:nokey=1")
            .arg(AUDIO_FILE)
            .output()
    })
    .await?
    .context("failed to spawn audio length process")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn render_video(audio_len: String) -> anyhow::Result<()> {
    let output = spawn_blocking_with_tracing(move || {
        // Create a video from the assets.
        Command::new("ffmpeg")
            .arg("-r").arg("1").arg("-loop").arg("1")
            .arg("-i").arg(IMAGE_FILE)
            .arg("-i").arg(AUDIO_FILE)
            .arg("-acodec").arg("copy")
            .arg("-vcodec").arg("libx264")
            .arg("-tune").arg("stillimage")
            .arg("-preset").arg("ultrafast")
            .arg("-ss").arg("0").arg("-t").arg(audio_len.trim())
            .arg(VIDEO_FILE).arg("-y")
            .output()
    })
    .await?  // bubble up `JoinError`s
    .context("failed to spawn video rendering process")?;

    tracing::info!("render stdout: {0}", String::from_utf8_lossy(&output.stdout));
    tracing::info!("render stderr: {0}", String::from_utf8_lossy(&output.stderr));
    Ok(())
}
