use actix_web::{web, HttpResponse};
use tera::{Tera, Context};
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::{AsyncCommands, RedisError};

use crate::utils::e500;

pub async fn load_file_page(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    tera: web::Data<Tera>,
) -> actix_web::Result<HttpResponse> {
    let mut conn = redis_pool.get().await.unwrap();
    let file_names: Vec<String> = conn.keys("*").await.map_err(|e: RedisError| {
        e500(e.to_string())
    })?;
    
    let mut ctx = Context::new();
    ctx.insert("endpoint", "/load");
    ctx.insert("files", &file_names);

    let html = tera.render("file_load.html", &ctx).map_err(|err| {
        e500(format!("Failed to render requested page: {err:?}"))
    })?;
    Ok(HttpResponse::Ok().body(html))
}

// TODO: Tera Error
// TODO: Pool Error
// TODO: Query Error
