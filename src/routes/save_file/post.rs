use actix_web::{web, HttpResponse, ResponseError};
use actix_web::http::StatusCode;
use actix_multipart::{Multipart, Field};
use futures_util::TryStreamExt as _;
use uuid::Uuid;
use mobc::Pool;
use mobc_redis::RedisConnectionManager;
use mobc_redis::redis::AsyncCommands;

use crate::utils::{derive_error_chain_fmt, e500};
use crate::routes::errors::RedisQueryError;

// Internal errors raised when calling the `save_file` endpoint.
#[derive(thiserror::Error)]
pub enum SaveFileError {
    /// Error for all errors raised while receiving the mutlipart payload.
    #[error(transparent)]
    ReceiveError(#[from] actix_multipart::MultipartError),
    #[error(transparent)]
    QueryError(#[from] RedisQueryError),
    #[error(transparent)]
    WebError(#[from] actix_web::Error),
}

derive_error_chain_fmt!(SaveFileError);

impl ResponseError for SaveFileError {
    fn status_code(&self) -> StatusCode {
        match self {
            SaveFileError::ReceiveError(multipart_err) => {
                multipart_err.status_code()
            },
            SaveFileError::QueryError(e) => e.status_code(),
            SaveFileError::WebError(e) => {
                e.as_response_error().status_code()
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        // Rn all of the errors are internal so the user does not
        // need to know about them. `match` is still used to raise
        // a compiler error, if a new error is added but not convered here.
        match self {
            SaveFileError::ReceiveError(_)
            | SaveFileError::WebError(_)
            | SaveFileError::QueryError(_)
            => HttpResponse::new(self.status_code()),
        }
    }
}

pub async fn save_file(
    redis_pool: web::Data<Pool<RedisConnectionManager>>,
    mut payload: Multipart,
) -> Result<HttpResponse, SaveFileError> {
    // Iterate over mutlipart stream.
    while let Some(field) = payload.try_next().await? {
        // multipart/form-data steam field has to contain `content_disposition`.
        let content_disposition = field.content_disposition();
        let file_name = content_disposition
            .get_filename()
            .map_or_else(|| Uuid::new_v4().to_string(), sanitize_filename::sanitize);

        let chunk = receive_field(field).await?;
        let mut conn = redis_pool.get().await.map_err(|e| e500(e))?;
        let _: () = conn.set(file_name, chunk).await.map_err(|e| RedisQueryError(e))?;
    }

    Ok(HttpResponse::Ok().finish())
}

async fn receive_field<'a>(mut field: Field) -> Result<String, SaveFileError> {
    let mut buf = String::new();
    while let Some(chunk) = field.try_next().await? {
        buf.push_str(&String::from_utf8_lossy(&chunk));
    }
    Ok(buf)
}

// TODO: Endpoint integration tests
