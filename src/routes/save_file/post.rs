use actix_web::{web, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_web::http::header::LOCATION;
use actix_multipart::{Multipart, Field};
use futures_util::TryStreamExt as _;
use uuid::Uuid;
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::AsyncCommands;
use serde::{Serialize, Deserialize};

use crate::utils::{derive_error_chain_fmt, e500};
use crate::routes::errors::RedisQueryError;

/*
TODO
Finish the journey to a finish video.
- `save` endpoint starts rendering worker. It receives:
    1. the ID of an image
    2. the ID of an audio
    3. the ID of the `pending` indicator

- `load_file_page` received the `pending` ID when `save` exits 
- `load_file` uses the pending ID to provide the video.

- the rendering worker sets the pending ID to the finished video when its done.
*/

// Content of entries which are still unfinished.
pub const PENDING: &str = "pending";
// Redis key for the render queue
pub const RENDER_QUEUE_KEY: &str = "render-worker-queue";

type Conn = mobc::Connection<RedisConnectionManager>;

// POST endpoint to upload any file to redis.
pub async fn save_file(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    mut payload: Multipart,
) -> Result<HttpResponse, SaveFileError> {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let mut render_task = RenderTaskBuilder::new(&mut conn).await?;

    // Store each file in the stream in redis.
    while let Some(field) = payload.try_next().await? {
        // Create and save a storage ID for the new file.
        // `render_task.set` checks that we receive the correct amount of files.
        let file_id = render_task.set(field.content_type())?;
        
        // Receive and store the file.
        let data = receive_field(field).await?;
        conn.set(file_id.to_string(), data).await
            .map_err(|e| RedisQueryError(e))?;
    }

    let queued_target_id = render_task
        .build()?
        .queue(&mut conn).await?;
    /*
    TODO
    - redis list which holds all pending `target_file_id`s for the worker
    - asynchronous worker thread rendering videos

    - flash messages
    */

    let redirect_url = format!("/load/{queued_target_id}");
    Ok(HttpResponse::SeeOther()
        .insert_header((LOCATION, redirect_url))
        .finish()
    )
}

// Helper using in `save_file` to stream a field.
async fn receive_field<'a>(mut field: Field) -> Result<String, SaveFileError> {
    let mut buf = String::new();
    while let Some(chunk) = field.try_next().await? {
        buf.push_str(&String::from_utf8_lossy(&chunk));
    }
    Ok(buf)
}

// Builder to construct a valid `RenderTask`.
// If the build fails this builder will free all resources.
pub struct RenderTaskBuilder {
    target: Uuid, // redis key of target entry
    audio: Option<Uuid>,  // redis key of audio file
    image: Option<Uuid>,  // redis key of image file
}

impl RenderTaskBuilder {
    // Create new instance with target_id entry in redis.
    async fn new(conn: &mut Conn) -> Result<Self, SaveFileError> {
        // Key of the redis entry to later store the finished video.
        let target_id = Uuid::new_v4();
        conn.set(target_id.to_string(), PENDING).await
            .map_err(|e| RedisQueryError(e))?;

        Ok(Self {
            target: target_id,
            audio: None,
            image: None,
        })
    }

    // Set the redis entry ID for a received file.
    fn set(&mut self, mime_opt: Option<&mime::Mime>) -> Result<Uuid, SaveFileError> {
        let mime_type = match mime_opt {
            Some(mt) => mt,
            None => return Err(SaveFileError::MissingMime),
        };

        match mime_type.type_() {
            mime::IMAGE => {
                match self.image {
                    Some(_) => Err(SaveFileError::UnexpectedMime(
                        "received more than one image".to_owned()
                    )),
                    None => {
                        let image_id = Uuid::new_v4();
                        self.image = Some(image_id);
                        Ok(image_id)
                    }
                }
            },
            mime::AUDIO => {
                match self.audio {
                    // TODO: Remove access files in this dtor
                    Some(_) => Err(SaveFileError::UnexpectedMime(
                        "received more than one audio".to_owned()
                    )),
                    None => {
                        let audio_id = Uuid::new_v4();
                        self.audio = Some(audio_id);
                        Ok(audio_id)
                    }
                }
            },
            _ => {  // Error: the received mime type was unexpected.
                let mime_string = mime_type.essence_str().to_owned();
                Err(SaveFileError::UnexpectedMime(mime_string))
            },
        }
    }

    fn build(self) -> Result<RenderTask, SaveFileError> {
        let audio_id = self.audio
            .ok_or(SaveFileError::MissingFile("audio"))?;
        let image_id = self.image
            .ok_or(SaveFileError::MissingFile("image"))?;

        Ok(RenderTask {
            target: self.target,
            audio: audio_id,
            image: image_id,
        })
    }
}

// Render task for the render worker to process.
#[derive(Serialize, Deserialize)]
pub struct RenderTask {
    target: Uuid,
    audio: Uuid,
    image: Uuid,
}

impl RenderTask {
    // Add `self` to the render task queue.
    async fn queue(self, conn: &mut Conn) -> Result<String, SaveFileError> {
        let ser = serde_json::to_string(&self).map_err(|e| e500(e))?;
        conn.lpush(RENDER_QUEUE_KEY, ser).await
            .map_err(|e| RedisQueryError(e))?;
        Ok(self.target.to_string())
    }
}

// Internal errors raised when calling the `save_file` endpoint.
#[derive(thiserror::Error)]
pub enum SaveFileError {
    /// A received file either contained an unexpected mime type or the mime was missing.
    #[error("Unexpected mime type: {0}")]
    UnexpectedMime(String),  // the mime type
    #[error("Missing mime type")]
    MissingMime,
    /// The render task was missing a file entry when trying to add it to the queue.
    #[error("Missing file for render: {0}")]
    MissingFile(&'static str),  // type of the file
    /// Error for all errors raised while receiving the mutlipart payload.
    #[error(transparent)]
    ReceiveError(#[from] actix_multipart::MultipartError),
    #[error(transparent)]
    QueryError(#[from] RedisQueryError),
    #[error(transparent)]
    WebError(#[from] actix_web::Error),
}

derive_error_chain_fmt!(SaveFileError);

impl ResponseError for SaveFileError {
    fn status_code(&self) -> StatusCode {
        match self {
            SaveFileError::UnexpectedMime(_) => {
                StatusCode::UNSUPPORTED_MEDIA_TYPE
            },
            SaveFileError::MissingMime => StatusCode::BAD_REQUEST,
            SaveFileError::MissingFile(_) => StatusCode::BAD_REQUEST,
            SaveFileError::ReceiveError(multipart_err) => {
                multipart_err.status_code()
            },
            SaveFileError::QueryError(e) => e.status_code(),
            SaveFileError::WebError(e) => {
                e.as_response_error().status_code()
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Rn all of the errors are internal so the user does not
        // need to know about them. `match` is still used to raise
        // a compiler error, if a new error is added but not convered here.
        match self {
            SaveFileError::UnexpectedMime(mime) => {
                HttpResponse::UnsupportedMediaType()
                    .body(format!("Media type not supported: {mime}"))
            },
            SaveFileError::MissingMime => {
                HttpResponse::BadRequest()
                    .body("Request is missing mime type(s)")
            },
            SaveFileError::MissingFile(file_type) => {
                HttpResponse::BadRequest()
                    .body(format!("Reqest is missing a file: {file_type}"))
            },
            SaveFileError::ReceiveError(_)
            | SaveFileError::WebError(_)
            | SaveFileError::QueryError(_)
            => HttpResponse::new(self.status_code()),
        }
    }
}
