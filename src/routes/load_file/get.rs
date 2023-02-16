use actix_web::{web, HttpResponse};
use actix_web::http::StatusCode;
use tera::{Tera, Context};
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::AsyncCommands;

use crate::utils::{e500, derive_error_chain_fmt};
use crate::routes::errors::{TeraError, RedisQueryError};

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

impl actix_web::ResponseError for LoadFilePageError {
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

pub async fn load_file_page(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    tera: web::Data<Tera>,
) -> Result<HttpResponse, LoadFilePageError>  {
    let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
    let file_names: Vec<String> = conn.keys("*").await
        .map_err(|e| RedisQueryError(e))?;

    let mut ctx = Context::new();
    ctx.insert("endpoint", "/load");
    ctx.insert("files", &file_names);

    let html = tera.render("file_load.html", &ctx)
        .map_err(|e| TeraError(e))?;
    Ok(HttpResponse::Ok().body(html))
}
