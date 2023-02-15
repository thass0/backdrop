use actix_web::{web, HttpResponse, Error};
use actix_multipart::{Multipart, Field};
use futures_util::TryStreamExt as _;
use uuid::Uuid;
use mobc::Pool;
use mobc_redis::{RedisConnectionManager, redis::AsyncCommands};

pub async fn save_file(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    mut payload: Multipart,
) -> Result<HttpResponse, Error> {
    // Iterate over mutlipart stream.
    while let Some(field) = payload.try_next().await? {
        // multipart/form-data steam field has to contain `content_disposition`.
        let content_disposition = field.content_disposition();
        let file_name = content_disposition
            .get_filename()
            .map_or_else(|| Uuid::new_v4().to_string(), sanitize_filename::sanitize);

        let chunk = receive_field(field).await?;
        let mut conn = redis_pool.get().await.unwrap();
        let _: () = conn.set(file_name, chunk).await.unwrap();
    }

    Ok(HttpResponse::Ok().finish())
}

async fn receive_field<'a>(mut field: Field) -> Result<String, Error> {
    use std::str::from_utf8;
    let mut buf = String::new();
    while let Some(chunk) = field.try_next().await? {
        buf.push_str(from_utf8(&chunk).unwrap());
    }
    Ok(buf)
}

// TODO: Error handling
// TODO: Integration tests
//     Write test to connect to redis!


pub async fn save_file_page() -> HttpResponse {
    let html = r#"
        <html>
        <head><title>Upload Test</title></head>
        <body>
            <form target="/" method="post" enctype="multipart/form-data">
                <input type="file" multiple name="file"/>
                <button type="submit">Submit</button>
            </form>
        </body>
        </html>
    "#;

    HttpResponse::Ok().body(html)
}