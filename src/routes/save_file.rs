use actix_web::{web, HttpResponse, Error};
use actix_multipart::Multipart;
use futures_util::TryStreamExt as _;
use uuid::Uuid;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub async fn save_file(
    file_dir: web::Data<PathBuf>,
    mut payload: Multipart,
) -> Result<HttpResponse, Error> {
    // Iterate over mutlipart stream.
    while let Some(mut field) = payload.try_next().await? {
        // multipart/form-data steam field has to contain `content_disposition`.
        let content_disposition = field.content_disposition();

        let file_name = content_disposition
            .get_filename()
            .map_or_else(|| Uuid::new_v4().to_string(), sanitize_filename::sanitize);
        let file_path = file_dir.join(file_name);

        // `File::create` is blocking so move it to seperate thread.
        let mut file_handle = web::block(|| File::create(file_path)).await??;

        // Write all bytes from the field the file.
        while let Some(chunk) = field.try_next().await? {
            // Move blocking file I/O to seperate thread again.
            file_handle = web::block(move ||
                // The () result of `write_all` is mapped to the file handle itself
                // so we can return the file handle back from the closure.
                // This way the loops keeps ownership of the file handle despite the `move`!!!
                file_handle.write_all(&chunk).map(|_| file_handle)
            ).await??;
        }
    }

    Ok(HttpResponse::Ok().finish())
}


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