use actix_web::Error;
use std::future::{ready, Ready};

use actix_web::dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform};
use futures_util::future::LocalBoxFuture;

use crate::utils::e400;

// The contents of this file are copied from: https://stackoverflow.com/a/71900552 (modified)

pub struct ContentLengthLimit {
    pub limit: u64, // Number of bytes
}

impl<S, B> Transform<S, ServiceRequest> for ContentLengthLimit
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = ContentLengthLimitMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(ContentLengthLimitMiddleware {
            service,
            limit: self.limit,
        }))
    }
}

impl Default for ContentLengthLimit {
    fn default() -> Self {
        Self {
            limit: 5 << 20, // Default limit at 5MB.
        }
    }
}

pub struct ContentLengthLimitMiddleware<S> {
    service: S,
    limit: u64,
}

impl<S, B> ContentLengthLimitMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    fn is_big(&self, req: &ServiceRequest) -> Result<bool, ()> {
        Ok(req
            .headers()
            .get("content-length")
            .ok_or(())?
            .to_str()
            .map_err(|_| ())?
            .parse::<u64>()
            .map_err(|_| ())?
            > self.limit)
    }
}

impl<S, B> Service<ServiceRequest> for ContentLengthLimitMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if let Ok(r) = self.is_big(&req) {
            if r {
                return Box::pin(async { Err(e400("content limit exceed")) });
            }
        }

        let fut = self.service.call(req);

        Box::pin(async move {
            let res = fut.await?;
            Ok(res)
        })
    }
}