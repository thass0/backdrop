use actix_web::{web, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_web::http::header::LOCATION;
use actix_multipart::{Multipart, Field};
use futures_util::TryStreamExt as _;
use uuid::Uuid;
use redis::AsyncCommands;
use std::ops::DerefMut;
use serde::{Serialize, Deserialize};

use crate::utils::{derive_error_chain_fmt, e500};
use crate::routes::errors::RedisQueryError;
use crate::{RedisPool, RedisConn, PENDING, RENDER_QUEUE_KEY};
use crate::REDIS_DISCARD;

// POST endpoint to upload any file to redis.
pub async fn save_file(
    redis_pool: web::Data<RedisPool>,
    payload: Multipart,
) -> Result<HttpResponse, SaveFileError> {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;

    // Start redis transaction to save the assets.
    redis::cmd("MULTI")
        .query_async(conn.deref_mut()).await
        .map_err(|e| RedisQueryError(e))?;

    // Receive and store the assets in the multipart form.
    let render_task = match RenderTask::build_from_form(
        &mut conn,
        payload,
    ).await {
        Ok(render_task) => render_task,
        Err(e) => {
            redis::cmd(REDIS_DISCARD)
                .query_async(conn.deref_mut()).await
                .map_err(|e| RedisQueryError(e))?;
            return Err(e);
        },
    };

    // Add a render task for the received assets to the render queue.
    let queued_target_id = match render_task.queue(&mut conn).await {
        Ok(queued_id) => queued_id,
        Err(e) => {
            redis::cmd(REDIS_DISCARD)
                .query_async(conn.deref_mut()).await
                .map_err(|e| RedisQueryError(e))?;
            return Err(e);
        },
    };

    // Commit the assets and the task to redis now the request
    // has been processed without any errors.
    redis::cmd("EXEC")
        .query_async(conn.deref_mut()).await
        .map_err(|e| RedisQueryError(e))?;

    // Redirect the caller to the download page for the queued video render.
    let redirect_url = format!("/done/{queued_target_id}");
    Ok(HttpResponse::SeeOther()
        .insert_header((LOCATION, redirect_url))
        .finish()
    )
}

// Render task used by the render worker to create a
// video form an audio and an image file.
#[derive(Serialize, Deserialize, Debug)]
pub struct RenderTask {
    pub target: Uuid,
    pub audio: Uuid,
    pub image: Uuid,
}

impl RenderTask {
    // Receive a multipart form and store it in redis.
    // Create a new instance of self using the received assets.
    async fn build_from_form(
        conn: &mut RedisConn,
        mut payload: Multipart,
    ) -> Result<Self, SaveFileError> {
        let mut builder = RenderTaskBuilder::new(conn).await?;

        while let Some(field) = payload.try_next().await? {
            // Check for a valid mime type in the current context before starting to receive.
            // If the mime is valid the redis key to store the data is returned.
            let asset_id = builder.validate_type(field.content_type())?;

            // Receive and store the data in self.
            let data = Self::receive_field(field).await?;
            conn.set(asset_id.to_string(), data).await
                .map_err(|e| RedisQueryError(e))?;
        }

        // Build asserts that all required assets are present
        builder.build()
    }

    // Stream a single multipart form field and store it in a `Vec<u8>` buffer.
    async fn receive_field(mut field: Field) -> Result<Vec<u8>, SaveFileError> {
        use std::io::Write;
        let mut buf: Vec<u8> = Vec::with_capacity(1<<19);  // 500kB buffer
        while let Some(chunk) = field.try_next().await? {
            buf.write_all(&chunk).map_err(|e| e500(e))?;
        }
        Ok(buf)
    }

    // Add `self` to the render worker task queue.
    pub async fn queue(self, conn: &mut RedisConn) -> Result<String, SaveFileError> {
        let ser = serde_json::to_string(&self).map_err(|e| e500(e))?;
        conn.lpush(RENDER_QUEUE_KEY, ser).await
            .map_err(|e| RedisQueryError(e))?;
        Ok(self.target.to_string())
    }
}

// Builder to help build a valid `RenderTask` instance.
pub struct RenderTaskBuilder {
    target: Uuid, // redis key of target entry
    audio: Option<Uuid>,  // redis key of audio file
    image: Option<Uuid>,  // redis key of image file
}

impl RenderTaskBuilder {
    // Create new instance with target_id entry in redis.
    async fn new(conn: &mut RedisConn) -> Result<Self, RedisQueryError> {
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

    // Check the given mime type is valid in the current state of the
    // task builder and return the type of the receiving assets.
    // The uuid returned by this function is meant to be used as the key
    // to the piece of data which is received along with the mime type
    // passed to the function call.
    fn validate_type(
        &mut self,
        mime_opt: Option<&mime::Mime>,
    ) -> Result<Uuid, SaveFileError> {
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
    
    // Create a `RenderTask` instance from the assets keys
    // collected in self. This method will never fail if
    // `validate_type` was called *twice* successfully before
    // calling this method.
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
