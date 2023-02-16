use actix_web::{web, HttpResponse, Error};
use tera::{Tera, Context};

use crate::utils::e500;

pub async fn save_file_page(tera: web::Data<Tera>) -> Result<HttpResponse, Error> {
    let mut ctx = Context::new();
    ctx.insert("endpoint", "/save");

    let html = tera.render("file_save.html", &ctx).map_err(|_| {
        e500("Failed to render requested page")
    })?;
    Ok(HttpResponse::Ok().body(html))
}
