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

// POST endpoint to upload any file to redis.
pub async fn save_file(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    mut payload: Multipart,
) -> Result<HttpResponse, SaveFileError> {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let mut files = vec![];  // The keys where the received files are stored in redis.

    // Store each file in the stream in redis.
    while let Some(field) = payload.try_next().await? {
        // Get a media file ID to access the file in redis later.
        let file_id = MediaFileId::new(field.content_type())?;
        
        // Receive and store the file.
        let data = receive_field(field).await?;
        conn.set(file_id.id_string(), data).await
            .map_err(|e| RedisQueryError(e))?;

        files.push(file_id);
    }

    // Key of the redis entry to later store the finished video.
    let target_file_id = Uuid::new_v4().to_string();
    conn.set(&target_file_id, PENDING).await
        .map_err(|e| RedisQueryError(e))?;

    /*
    TODO
    - redis list which holds all pending `target_file_id`s for the worker
    - asynchronous worker thread rendering videos

    - flash messages
    */

    let redirect_url = format!("/load/{target_file_id}");
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

// The ID of a media file stored in redis.
#[derive(Serialize, Deserialize)]
enum MediaFileId {
    Audio(Uuid),
    Image(Uuid),
}

impl MediaFileId {
    fn new(mime_opt: Option<&mime::Mime>) -> Result<Self, SaveFileError> {
        let mime_type = match mime_opt {
            Some(mt) => mt,
            None => return Err(SaveFileError::MissingMime),
        };

        match mime_type.type_() {
            mime::IMAGE => Ok(Self::Image(Uuid::new_v4())),
            mime::AUDIO => Ok(Self::Audio(Uuid::new_v4())),
            _ => {  // Error: the received mime type was unexpected.
                let mime_string = mime_type.essence_str().to_owned();
                Err(SaveFileError::UnexpectedMime(mime_string))
            },
        }
    }

    fn id_string(&self) -> String {
        match self {
            Self::Audio(uuid) => uuid.to_string(),
            Self::Image(uuid) => uuid.to_string(),
        }
    }
}

// Internal errors raised when calling the `save_file` endpoint.
#[derive(thiserror::Error)]
pub enum SaveFileError {
    /// A received file either contained an unexpected mime type or the mime was missing.
    #[error("Unexpected mime type: {0}")]
    UnexpectedMime(String),  // The mime type
    #[error("Missing mime type")]
    MissingMime,
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
            SaveFileError::ReceiveError(_)
            | SaveFileError::WebError(_)
            | SaveFileError::QueryError(_)
            => HttpResponse::new(self.status_code()),
        }
    }
}
