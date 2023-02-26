use actix_web::{web, get, HttpResponse, HttpRequest, ResponseError, Responder};
use actix_web::http::StatusCode;
use actix_web::http::header::{ContentDisposition, CONTENT_TYPE};
use tera::{Tera, Context};
use redis::AsyncCommands;
use uuid::Uuid;
use serde::Serialize;

use crate::utils::{e500, derive_error_chain_fmt};
use crate::routes::errors::{TeraError, RedisQueryError};
use crate::{RedisPool, PENDING, GONE, READY, REDIS_TTL_EXPIRED};

// The name of a rendered file
const FILE_NAME: &str = "backdrop.mp4";

// GET endpoint to download any pending file from redis.
// The `GET /done/ready` endpoint will return a vaild key to the
// video data for a given process ID, once a video is done rendering.
#[get("/load/{videoKey}")]
pub async fn load_file(
    redis_pool: web::Data<RedisPool>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFileError> {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let video_key = path.into_inner().to_string();

    let data: Vec<u8> = conn.get(&video_key).await
        .map_err(|e| e500(e))?;  // opaque error to make it harder to use
                                 // this endpoint for random queries.
                                 // TODO: Add auth to this endpoint.

    Ok(HttpResponse::Ok()
        .insert_header((CONTENT_TYPE, "video/mp4"))
        .insert_header(ContentDisposition::attachment(FILE_NAME))
        .body(data))
}

// TODO: Error propagation if rendering fails or if query fails.

// Page to download a rendered backdrop video.
// The handler sends the progress ID to the page frontend
// so the page can check whether the video is ready or not dynamically.
#[get("/done/{progressId}")]
pub async fn load_file_page(
    tera: web::Data<Tera>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, LoadFilePageError>  {
    let progress_id = path.into_inner().to_string();
    
    let mut ctx = Context::new();
    // Endpoint to download form with the ID of the video file to download
    ctx.insert("progress_id", &progress_id);
    // Name of the video file to download.
    ctx.insert("filename", FILE_NAME);  
    // `ready_`, `gone_` and `pending_msg` are used to evaluate the responses
    // from `GET /done/ready`. This endpont will responsd with the same constants (`READY`, ...) 
    // depending on the progress of the video.
    ctx.insert("pending_msg", PENDING);
    ctx.insert("gone_msg", GONE);
    ctx.insert("ready_msg", READY);
    // The following headings and info elements are used to switch up
    // the content displayed on the page at different steps in the rendering progress.
    ctx.insert("pending_heading", "Your video is being rendered!");
    ctx.insert("pending_info", "This might take a few seconds. You can download the result once it is ready.");
    ctx.insert("ready_heading", "Download your backdrop video!");
    ctx.insert("ready_info", "Your video has successfully finished rendering. It will be deleted from the \
        server in a few minutes");
    ctx.insert("gone_heading", "Assets are deleted");
    ctx.insert("gone_info", "The requested video and all assets used to create this video have been deleted.");

    let html = tera.render("file_load.html", &ctx)
        .map_err(|e| TeraError(e))?;
    Ok(HttpResponse::Ok().body(html))
}

// Check whether a task is done rendering and the
// video is ready for download.
#[get("/done/ready/{progressId}")]
async fn check_resource_state(
    redis_pool: web::Data<RedisPool>,
    path: web::Path<Uuid>,
) -> actix_web::Result<impl actix_web::Responder> {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let progress_id = path.into_inner().to_string();

    let progress: String = conn.get(&progress_id).await
        .map_err(|e| e500(e))?;

    // If `progress` is set to `PENDING`, the video has not yet finished
    // rendering. The client should wait and try again.
    if progress == PENDING {
        return Ok(VideoProgress::Pending);
    }

    // If `progress` is not set to `PENDING` it contains the key of the
    // finished video.
    let video_key = progress;
    // Now we can check if the video is still available.
    // We return the video key if this is the case. Otherwise, the `GONE`
    // message is returned to the client to indicate that the video is deleted now.
    let video_lifetime: i32 = conn.ttl(&video_key).await
        .map_err(|e| e500(e))?;

    if video_lifetime == REDIS_TTL_EXPIRED {
        // This endpoint should not be called anymore after the `GONE`
        // response was sent once. Because of this the progress key should
        // now get deleted too.
        conn.del(&progress_id).await
            .map_err(|e| e500(e))?;
        // Indicate to the client that the video is no longer available.
        Ok(VideoProgress::Gone)
    } else {
        Ok(VideoProgress::Ready(video_key))
    }
}

#[derive(Debug)]
enum VideoProgress {
    Pending,
    Gone,
    Ready(String),
}

impl Responder for VideoProgress {
    type Body = actix_web::body::EitherBody<String>;

    fn respond_to(self, req: &HttpRequest) -> HttpResponse<Self::Body> {
        match self {
            VideoProgress::Pending => web::Json(ProgressResponse {
                progress: PENDING.to_owned(),
                video_key: None,
            }).respond_to(req),
            VideoProgress::Gone => web::Json(ProgressResponse {
                progress: GONE.to_owned(),
                video_key: None,
            }).respond_to(req),
            VideoProgress::Ready(key) => web::Json(ProgressResponse {
                progress: READY.to_owned(),
                video_key: Some(key),
            }).respond_to(req),
        }
    }
}

// Struct used by `VideoProgress` to create JSON responses.
#[derive(Debug, Serialize)]
struct ProgressResponse {
    progress: String,
    video_key: Option<String>,
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
