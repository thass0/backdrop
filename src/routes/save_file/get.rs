use actix_web::{web, HttpResponse};
use tera::{Tera, Context};

use crate::routes::errors::TeraError;

// Page with form to upload a file.
pub async fn save_file_page(tera: web::Data<Tera>) -> Result<HttpResponse, TeraError> {
    let mut ctx = Context::new();
    ctx.insert("endpoint", "/save");

    let html = tera.render("file_save.html", &ctx)?;
    Ok(HttpResponse::Ok().body(html))
}
