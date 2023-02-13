use std::net::TcpListener;
use actix_web::{web, App, HttpServer};
use actix_web::dev::Server;
use tracing_actix_web::TracingLogger;
use crate::routes;
use crate::configuration::Settings;
use std::fs;
use std::path::PathBuf;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let file_dir_path = PathBuf::from(configuration.application.file_dir);
        fs::create_dir_all(&file_dir_path)?;

        let address = format!(
            "{}:{}",
            configuration.application.host,
            configuration.application.port,
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr().unwrap().port();
        let server = run(listener, file_dir_path).await?;

        Ok(Self{ port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}



pub async fn run(
    listener: TcpListener,
    file_dir: PathBuf,
) -> Result<Server, anyhow::Error> {
    let file_dir = web::Data::new(file_dir);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(routes::health_check))
            .service(
                web::resource("/")
                .route(web::get().to(routes::save_file_page))
                .route(web::post().to(routes::save_file))
            )
            .app_data(file_dir.clone())
    })
    .listen(listener)?
    .run();

    Ok(server)
}
