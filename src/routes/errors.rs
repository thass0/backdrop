// Errors raised by routes, all of which should implement actix_web::ResponseError.

// Any wrappers are used to display a uniform error message to the user.
// If say actix_web::error::ErrorInternalServerError were used, the internal
// `Display` error message would be leaked to the user.

// Each error returns an opaque 500 with a short message to the client.
// Internally, the entire error source is logged.

macro_rules! internal_error_wrapper {
    ($name:ident, $wrapped:path, $msg:literal) => {
        #[derive(thiserror::Error)]
        #[error(transparent)]
        pub struct $name (
            #[from] pub $wrapped  // pub to allow easy initialization
        );

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                use crate::utils::error_chain_fmt;
                error_chain_fmt(&self.0, f)
            }
        }

        impl actix_web::ResponseError for $name {
            fn status_code(&self) -> actix_web::http::StatusCode {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            }

            fn error_response(&self) -> actix_web::HttpResponse {
                actix_web::HttpResponse::InternalServerError()
                    .body($msg)
            }
        }
    };
}

// Wrapper for errors raised while rendering tera pages.
internal_error_wrapper!(TeraError, tera::Error, "Failed to render requested page");

// Wrapper around `RedisError` used for failed redis querys.
internal_error_wrapper!(
    RedisQueryError,
    mobc_redis::redis::RedisError,
    "Database query error"
);
