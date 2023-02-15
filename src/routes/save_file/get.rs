use actix_web::HttpResponse;

pub async fn save_file_form() -> HttpResponse {
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