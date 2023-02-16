use actix_web::{web, get, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_web::http::header::{ContentDisposition, ContentType};
use tera::{Tera, Context};
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::AsyncCommands;

use crate::utils::{e500, derive_error_chain_fmt};
use crate::routes::errors::{TeraError, RedisQueryError};

// GET endpoint to download any file from redis.
#[get("/load/{filename}")]
pub async fn load_file(
    path: web::Path<String>,
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
) -> Result<HttpResponse, LoadFileError> {
    let file_name = path.into_inner();  // the file name might also be sent in a URL-encoded form

    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let file_contents: String = conn.get(&file_name).await
        .map_err(|e| RedisQueryError(e))?;

    Ok(HttpResponse::Ok()
        .content_type(ContentType::plaintext())  // <-- Changed to `video/mp4 once the videos are ready.
        .insert_header(ContentDisposition::attachment(file_name))
        .body(file_contents))
}

// Page to download available files from redis.
pub async fn load_file_page(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    tera: web::Data<Tera>,
) -> Result<HttpResponse, LoadFilePageError>  {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let file_names: Vec<String> = conn.keys("*").await
        .map_err(|e| RedisQueryError(e))?;

    let mut ctx = Context::new();
    ctx.insert("download_endpoint", "/load");
    ctx.insert("files", &file_names);

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
