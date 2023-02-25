use std::time::Duration;
use anyhow::Context;
use redis::AsyncCommands;
use std::process::Command;
use tokio::io::AsyncWriteExt;
use tokio::fs::File;
use uuid::Uuid;
use std::ops::DerefMut;
use std::path::PathBuf;

use crate::configuration::{Settings, RenderWorkerSettings};
use crate::startup::get_redis_pool;
use crate::{RedisPool, RENDER_QUEUE_KEY};
use crate::routes::RenderTask;
use crate::utils::spawn_blocking_with_tracing;
use crate::{RedisConn, REDIS_DISCARD};

const ASSETS_DIR: &str = "tmp_assets";

pub async fn run_until_stopped(configuration: Settings) -> anyhow::Result<()> {
    let render_config = configuration.render_worker;
    let redis_pool = get_redis_pool(configuration.redis_uri).await?;

    // Make sure buffer file directory for assets exists.
    tokio::fs::create_dir_all(ASSETS_DIR).await?;

    tracing::info!("Set up render worker; Now entering working loop.");
    worker_loop(redis_pool, render_config).await
}

async fn worker_loop(
    redis_pool: RedisPool,
    render_config: RenderWorkerSettings,
) -> anyhow::Result<()> {
    let laziness = render_config.laziness.into();
    let lifetime = render_config.lifetime;
    loop {
        let task = match get_next_task(redis_pool.clone()).await {
            Ok(QueueQueryOutcome::NewTask(t)) => t,
            Ok(QueueQueryOutcome::EmptyQueue) => {
                // Wait for queue to fill up.
                tokio::time::sleep(Duration::from_secs(laziness)).await;
                continue;
            },
            Err(e) => {
                tracing::error!("Render queue error: {e:?}");
                continue;  // try again
            },
        };

        let mut conn = redis_pool.get().await
            .context("failed to acquire redis connection")?;

        match try_render_task(&mut conn, &task).await {
            Ok(data) => {
                // Store finished video in redis and delete its assets.
                try_save_render(&mut conn, task, &data, lifetime).await?;
            },
            Err(e) => {
                tracing::error!("Render worker error: {e:?}");

                // Abort changes to assets.
                redis::cmd(REDIS_DISCARD)
                    .query_async(conn.deref_mut()).await
                    .context("failed to discard redis transaction after render error")?;

                // Queue the task again.
                // TODO: Store a counter in the task so erroneous tasks are deleted eventually.
                match task.queue(&mut conn).await {
                    Ok(_) => {},  // the target ID returned by `queue` is not interesting here.
                    Err(e) => {
                        tracing::warn!("failed to re-queue previously failed task; \
                            deleting task. Re-queue error: {e:?}");
                    },
                }
                
                // Wait and try again.
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

// Save the given render result data in redis and delete its assets.
// Saving is wrapped in a transaction to ensure the progress key
// is updated along with the video data in any case where the
// video data is saved.
// The assets are deleted because they were only used to render the
// video once.
async fn try_save_render(
    conn: &mut RedisConn,
    task: RenderTask,
    data: &[u8],
    lifetime_mins: u16,
) -> anyhow::Result<()> {
    let video_key = Uuid::new_v4().to_string();

    redis::cmd("MULTI").query_async(conn.deref_mut()).await
        .context("failed to start transaction to save render")?;

    // Store video data
    let _: () = match conn.set(&video_key, data).await {
        Ok(_r) => _r,  // this passing around is required to satisfy `set`s generics.
        Err(e) => {
            redis::cmd(REDIS_DISCARD).query_async(conn.deref_mut()).await
                .context("failed to abort transaction to save render")?;
            return Err(anyhow::anyhow!("failed to store video data in redis: {e:?}"));
        },
    };

    // Set expiration of video data
    let lifetime_secs: usize = (lifetime_mins * 60).into();
    let _: () = match conn.expire(&video_key, lifetime_secs).await {
        Ok(_r) => _r,
        Err(e) => {
            redis::cmd(REDIS_DISCARD).query_async(conn.deref_mut()).await
                .context("failed to abort transaction to save render")?;
            return Err(anyhow::anyhow!("failed set video data expiration in redis: {e:?}"));
        },
    };

    // Store key of video in progress key to access video data again from `GET /load`
    let _: () = match conn.set(&task.target.to_string(), &video_key).await {
        Ok(_r) => _r,
        Err(e) => {
            redis::cmd(REDIS_DISCARD).query_async(conn.deref_mut()).await
                .context("failed to abort transaction to save render")?;
            return Err(anyhow::anyhow!("failed to store video key in redis: {e:?}"));
        },
    };

    // Delete image
    let _: () = match conn.del(&task.image.to_string()).await {
        Ok(_r) => _r,
        Err(e) => {
            redis::cmd(REDIS_DISCARD).query_async(conn.deref_mut()).await
                .context("failed to abort transaction to save render")?;
            return Err(anyhow::anyhow!("failed to delete image asset in redis: {e:?}"));
        }
    };

    // Delete audio
    let _: () = match conn.del(&task.audio.to_string()).await {
        Ok(_r) => _r,
        Err(e) => {
            redis::cmd(REDIS_DISCARD).query_async(conn.deref_mut()).await
                .context("failed to abort transaction to save render")?;
            return Err(anyhow::anyhow!("failed to delete audio asset in redis: {e:?}"));
        }
    };

    redis::cmd("EXEC").query_async(conn.deref_mut()).await
        .context("failed to finish transaction to save render")?;

    tracing::trace!("Successfully updated video in redis {video_key}. \
        Deleted task {task:?} and all its assets");

    Ok(())
}

// Try to get the next task from the render task queue. This function
// will pop the task from the queue. If rendering fails the task has
// to be pushed to the queue again manually or it is lost.
async fn get_next_task(
    redis_pool: RedisPool,
) -> anyhow::Result<QueueQueryOutcome> {
    // Acquire own connection to ensure the connection is not inside a
    // transaction. The `conn.rpop` call is required to return the value
    // directly, because of this a transaction would break this check.
    let mut conn = redis_pool.get().await
        .context("failed to acquire redis connection to check queue")?;

    let task: RenderTask = {
        // Pop the next task entry from the queue.
        // Return if the queue is empty to wait for the queue to fill up.
        let raw_task: String = match conn.rpop(RENDER_QUEUE_KEY, None).await {
            Err(_) => return Ok(QueueQueryOutcome::EmptyQueue),
            Ok(raw) => raw,
        };

        serde_json::from_str(&raw_task)
            // If we return an error here, the task is deleted
            // because it cannot be queued again. This is OK as a task
            // which cannot be deserialized cannot ever be used anyways.
            .context("failed to deserialize task; task deleted")?
    };

    tracing::trace!("Received render task: {task:?}");

    Ok(QueueQueryOutcome::NewTask(task))
}

// > I had to use this double-"que" name!
enum QueueQueryOutcome {
    NewTask(RenderTask),
    EmptyQueue,
}

async fn try_render_task(
    conn: &mut RedisConn,
    task: &RenderTask,
) -> anyhow::Result<Vec<u8>> {
    // Buffer audio data in file.
    let audio_data: Vec<u8> = conn.get(task.audio.to_string()).await
        .context("failed to query audio data")?;
    let mut audio_buf = FfmpegAssetBuffer::new(
        FfmpegBufferName::new_audio(task.audio)
    ).await.context("failed to create audio buffer file")?;
    audio_buf.add_data(&audio_data).await?;

    // Buffer image  data in file.
    let image_data: Vec<u8> = conn.get(task.image.to_string()).await
        .context("failed to query image data")?;
    let mut image_buf = FfmpegAssetBuffer::new(
        FfmpegBufferName::new_image(task.image)
    ).await.context("failed to create image buffer file")?;
    image_buf.add_data(&image_data).await?;

    // Render the video
    tracing::trace!("Starting rendering {0}", task.target);
    let video_data = render_video(
        image_buf.get_path(),
        audio_buf.get_path(),
    ).await?;
    tracing::info!("Finished rendering {0}", task.target);

    Ok(video_data)
}

// Render the video using the given files a assets.
// This function will also delete the files again.
async fn render_video(
    image_path: PathBuf,
    audio_path: PathBuf,
) -> anyhow::Result<Vec<u8>> {
    let output = spawn_blocking_with_tracing(move || {
        let image_path = image_path.to_str()
            .expect("ffmpeg can't use invalid image path");
        let audio_path = audio_path.to_str()
            .expect("ffmpeg can't use invalid audio path");

        // Create a video from the assets.
        Command::new("ffmpeg")
            // Loop the  image with a tiny frame rate (1FPS)
            .args(["-r", "1", "-loop", "1"])
            // Use the given image and audio files as inputs.
            .args(["-i", image_path, "-i", audio_path])
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

// Buffering asset data in files so `ffmpeg` can use the data
// brings the danger of dandling files which will never be used again.
// This type therefore wrapps creating and deleting such buffer files
// to avoid ever forgetting to delete any of them.

// new
// name
// drop

// TODO mod

struct FfmpegAssetBuffer {
    file: tokio::fs::File,
    path: PathBuf,
}

impl FfmpegAssetBuffer {
    // Create a new buffer file from a given name.
    async fn new(name: FfmpegBufferName)-> anyhow::Result<FfmpegAssetBuffer> {
        let path = PathBuf::from(format!("{ASSETS_DIR}/{name}"));
        let file = File::create(&path).await
            .context("failed to create file")?;
        Ok(Self { file, path })
    }

    // Store the given data in the buffer file.
    async fn add_data(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.file
            .write_all(data).await
            .context(format!("failed to write data to buffer {}", self.path.display()))
    }

    // Return the file path of this buffer.
    fn get_path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl Drop for FfmpegAssetBuffer {
    fn drop(&mut self) {
        let err_msg = format!("failed to remove ffmpeg asset buffer file: {}", self.path.display());
        // WARNING: This is blocking I/O : (
        // Sadly there doesn't seem to be a quick method to
        // use async in `drop`. Maybe come back to this later.
        std::fs::remove_file(&self.path).expect(&err_msg)
    }
}

// This type is used to wrap file names of
// ffmpeg buffers to further secure their use
// (e.g. a programmer (me) accidentally passing a file
// name with the wrong extension to `FfmpegAssetBuffer::new`).
enum FfmpegBufferName {
    Image(String),
    Audio(String),
}

impl FfmpegBufferName {
    fn new_audio(audio_key: Uuid) -> Self {
        Self::Audio(format!("{audio_key}.mp3"))
        
    }

    fn new_image(image_key: Uuid) -> Self {
        Self::Image(format!("{image_key}.jpg"))
    }
}

impl std::fmt::Display for FfmpegBufferName {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self  {
            FfmpegBufferName::Image(name) => write!(f, "{name}"),
            FfmpegBufferName::Audio(name) => write!(f, "{name}"),
        }
    }
}
