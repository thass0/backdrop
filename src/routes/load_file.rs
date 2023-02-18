use actix_web::{web, get, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_web::http::header::{ContentDisposition, ContentType};
use tera::{Tera, Context};
use mobc_redis::redis::AsyncCommands;
use uuid::Uuid;

use crate::utils::{e500, derive_error_chain_fmt};
use crate::routes::errors::{TeraError, RedisQueryError};
use crate::{RedisPool, PENDING};


// The name of a rendered file
const FILE_NAME: &str = "backdrop.mp4";

// GET endpoint to download any pending file from redis.
#[get("/load/{fileProgressId}")]
pub async fn load_file(
    redis_pool: web::Data<RedisPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFileError> {
    // Redis entry of `file_progress_id` indicates whether the
    // file is ready for download.
    let file_progress_id = path.into_inner().to_string();

    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;

    // Check if the video is done rendering.
    let video_id = loop {
        let done: String = conn.get(&file_progress_id).await
            .map_err(|e| RedisQueryError(e))?;

        if done.as_str() != PENDING {
            break done;  // `done` now holds the ID of the finished video.
        }
    };

    let data = conn.get(&video_id).await
        .map_err(|e| RedisQueryError(e))?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::plaintext())  // <-- Changed to `video/mp4 once the videos are ready.
        .insert_header(ContentDisposition::attachment(FILE_NAME))
        .body(data)
    )
}

// Page to download a rendered backdrop video.
#[get("/load/{videoId}")]
pub async fn load_file_page(
    tera: web::Data<Tera>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFilePageError>  {
    let video_id = path.into_inner().to_string();
    
    let mut ctx = Context::new();
    ctx.insert("download_endpoint", "/load");  // The endpoint to download from
    ctx.insert("video_id", &video_id);  // ID of the video file to download
    ctx.insert("filename", FILE_NAME);  // Name of the video file.

    let html = tera.render("file_load.html", &ctx)
        .map_err(|e| TeraError(e))?;
    Ok(HttpResponse::Ok().body(html))
}


// Error returned by `load_file` endpoint.
#[derive(thiserror::Error)]
pub enum LoadFileError {
    #[error(transparent)]
    QueryError(#[from] RedisQueryError),
    #[error(transparent)]
    WebError(#[from] actix_web::Error),
}

derive_error_chain_fmt!(LoadFileError);

impl ResponseError for LoadFileError {
    fn status_code(&self) -> StatusCode {
        match self {
            LoadFileError::QueryError(e) => e.status_code(),
            LoadFileError::WebError(e) => {
                e.as_response_error().status_code()
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            LoadFileError::QueryError(e) => e.error_response(),
            LoadFileError::WebError(e) => e.error_response(),
        }
    }
}

// Error returned by `load_file_page` endpoint.
#[derive(thiserror::Error)]
pub enum LoadFilePageError {
    #[error(transparent)]
    RenderError(#[from] TeraError),
    #[error(transparent)]
    QueryError(#[from] RedisQueryError),
    #[error(transparent)]
    WebError(#[from] actix_web::Error),
}

derive_error_chain_fmt!(LoadFilePageError);

impl ResponseError for LoadFilePageError {
    fn status_code(&self) -> StatusCode {
        match self {
            LoadFilePageError::RenderError(e) => e.status_code(),
            LoadFilePageError::QueryError(e) => e.status_code(),
            LoadFilePageError::WebError(e) => {
                e.as_response_error().status_code()
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            LoadFilePageError::RenderError(e) => e.error_response(),
            LoadFilePageError::QueryError(e) => e.error_response(),
            LoadFilePageError::WebError(e) => e.error_response(),
        }
    }
}
