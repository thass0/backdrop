use std::time::Duration;
use anyhow::Context;
use redis::AsyncCommands;
use std::process::Command;
use tokio::io::AsyncWriteExt;
use tokio::fs::File;
use uuid::Uuid;

use crate::configuration::Settings;
use crate::startup::get_redis_pool;
use crate::{RedisPool, RENDER_QUEUE_KEY};
use crate::routes::RenderTask;
use crate::utils::spawn_blocking_with_tracing;

const ASSETS_DIR: &str = "tmp_assets";

pub async fn run_until_stopped(configuration: Settings) -> anyhow::Result<()> {
    let redis_pool = get_redis_pool(configuration.redis_uri).await?;
    tokio::fs::create_dir_all(ASSETS_DIR).await?;
    tracing::info!("Set up render worker; Now entering working loop.");
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

    // Both the audio and the image data need to be copied to
    // a file so ffmpeg can ready them.
    // The workaround to avoid copying could be to only ever store them in a file.
    // This has the drawback that assets become local to a single container.
    // Maybe docker volumes could be used but redis seems file for now ...

    // Buffer audio data in file.
    let audio_data: Vec<u8> = conn.get(task.audio.to_string()).await
        .context("failed to query audio data")?;
    let audio_file_name = format!("{ASSETS_DIR}/{}.mp3", task.audio);
    File::create(&audio_file_name).await
        .context("failed to create audio file")?
        .write_all(&audio_data).await
        .context("failed to write audio")?;

    // Buffer image  data in file.
    let image_data: Vec<u8> = conn.get(task.image.to_string()).await
        .context("failed to query image data")?;
    let image_file_name = format!("{ASSETS_DIR}/{}.jpg", task.image);
    File::create(&image_file_name).await
        .context("failed to create image file")?
        .write_all(&image_data).await
        .context("failed to write image")?;

    // Render the video
    tracing::trace!("Starting rendering {0}", task.target);
    let video_data = render_video(
        image_file_name.clone(),  // render_video has to own file names to spawn new thread.
        audio_file_name.clone(),
    ).await?;
    tracing::info!("Finished rendering {0}", task.target);
    
    // Store finished video in redis.
    let video_id = Uuid::new_v4().to_string();
    conn.set(&video_id, &video_data).await
        .context("failed to set video data in db")?;
    // Store key of video in target.
    conn.set(&task.target.to_string(), &video_id).await
        .context("failed to set video target id")?;

    tracing::trace!("Successfully updated video in redis {0}", task.target);

    // Remove file buffers.
    tokio::fs::remove_file(image_file_name).await
        .context("failed to remove image file")?;
    tokio::fs::remove_file(audio_file_name).await
        .context("failed to remove audio file")?;

    Ok(ExecutionOutcome::TaskCompleted)
}

// Render the video using the given files a assets.
// This function will also delete the files again.
async fn render_video(
    image_file_name: String,
    audio_file_name: String,
) -> anyhow::Result<Vec<u8>> {
    let output = spawn_blocking_with_tracing(move || {
        // Create a video from the assets.
        Command::new("ffmpeg")
            // Loop the  image with a tiny frame rate (1FPS)
            .args(["-r", "1", "-loop", "1"])
            // Use the given image and audio files as inputs.
            .args(["-i", &image_file_name, "-i", &audio_file_name])
            // Stop the video when the audio stops.
            .args(["-shortest", "-fflags", "shortest", "-max_interleave_delta", "100M"])
            // Enable piped MP4.
            .args(["-movflags", "frag_keyframe+empty_moov"])
            // Copy audio codec and use libx264 for video.
            .args(["-acodec", "copy", "-vcodec", "libx264"])
            // More rendering speedups for still image videos
            .args(["-tune", "stillimage", "-preset", "ultrafast"])
            // Save result encoded as MP4 to stdout.
            .args(["-f", "mp4", "-"])
            .output()
    })
    .await?  // bubble up `JoinError`s
    .context("failed to spawn video rendering process")?;

    tracing::trace!("render stderr: {0}", String::from_utf8_lossy(&output.stderr));

    Ok(output.stdout)
}
