use actix_web::{web, HttpResponse, Error};
use tera::{Tera, Context};
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::AsyncCommands;

use crate::utils::e500;

pub async fn save_file_page(tera: web::Data<Tera>) -> Result<HttpResponse, Error> {
    let mut ctx = Context::new();
    ctx.insert("endpoint", "/upload");

    let html = tera.render("file_upload.html", &ctx).map_err(|_| {
        e500("Failed to render requested page")
    })?;
    Ok(HttpResponse::Ok().body(html))
}

// Display all files in redis.
pub async fn get_files_page(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    tera: web::Data<Tera>,
) -> Result<HttpResponse, Error> {
    let mut conn = redis_pool.get().await.unwrap();
    let files: Vec<String> = conn.keys("*").await.unwrap();
    
    let mut ctx = Context::new();
    ctx.insert("files", &files);

    // TODO: Render error.
    let html = tera.render("get_saved.html", &ctx).map_err(|_| {
        e500("Failed to render requested page")
    })?;
    Ok(HttpResponse::Ok().body(html))
}