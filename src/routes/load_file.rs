use actix_web::{web, get, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_web::http::header::{ContentDisposition, CONTENT_TYPE};
use tera::{Tera, Context};
use mobc_redis::redis::AsyncCommands;
use uuid::Uuid;

use crate::utils::{e500, derive_error_chain_fmt};
use crate::routes::errors::{TeraError, RedisQueryError};
use crate::{RedisPool, PENDING};


// The name of a rendered file
const FILE_NAME: &str = "backdrop.mp4";

// GET endpoint to download any pending file from redis.
#[get("/load/{fileId}")]
pub async fn load_file(
    redis_pool: web::Data<RedisPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFileError> {
    let file_id = path.into_inner().to_string();

    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;

    let data: Vec<u8> = conn.get(&file_id).await
        .map_err(|e| RedisQueryError(e))?;

    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, "video/mp4"))
        .insert_header(ContentDisposition::attachment(FILE_NAME))
        .body(data)
    )
}

// TODO: Error propagation if rendering fails.

// Page to download a rendered backdrop video.
#[get("/done/{videoId}")]
pub async fn load_file_page(
    tera: web::Data<Tera>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFilePageError>  {
    let video_id = path.into_inner().to_string();
    
    let mut ctx = Context::new();
    // Endpoint to download form with the ID of the video file to download
    ctx.insert("file_id", &video_id);
    ctx.insert("filename", FILE_NAME);  // Name of the video file.
    ctx.insert("pending_msg", PENDING);

    let html = tera.render("file_load.html", &ctx)
        .map_err(|e| TeraError(e))?;
    Ok(HttpResponse::Ok().body(html))
}

// Check if a resource is ready to download.
#[get("/done/ready/{progressId}")]
async fn check_resource_state(
    redis_pool: web::Data<RedisPool>,
    path: web::Path<Uuid>,
) -> actix_web::Result<impl actix_web::Responder> {
    let progress_id = path.into_inner().to_string();
    let mut conn = redis_pool.get().await
        .map_err(|e| e500(e))?;

    let progress: String = conn.get(&progress_id).await
        .map_err(|e| e500(e))?;

    Ok(web::Json(progress))
}


// Error returned by `load_file` endpoint.
#[derive(thiserror::Error)]
pub enum LoadFileError {
    #[error("Requested unavailable resource: id: {0}")]
    ResourceError(String),
    #[error(transparent)]
    QueryError(#[from] RedisQueryError),
    #[error(transparent)]
    WebError(#[from] actix_web::Error),
}

derive_error_chain_fmt!(LoadFileError);

impl ResponseError for LoadFileError {
    fn status_code(&self) -> StatusCode {
        match self {
            LoadFileError::ResourceError(_) => StatusCode::NOT_FOUND,
            LoadFileError::QueryError(e) => e.status_code(),
            LoadFileError::WebError(e) => {
                e.as_response_error().status_code()
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            LoadFileError::ResourceError(_) => {
                HttpResponse::NotFound()
                    .body("The requested resouce is not available")
            }
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
