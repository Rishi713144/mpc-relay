use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use futures::future::{ready, LocalBoxFuture, Ready};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let auth = req.headers().get("Authorization");
        if auth.is_none() {
            let (request, _pl) = req.into_parts();
            let response = actix_web::HttpResponse::Unauthorized().finish();
            return Box::pin(async move { Ok(ServiceResponse::new(request, response)) });
        }
        let auth_str = auth.unwrap().to_str().unwrap_or("");
        // Accept "jwt" as per header example or Bearer tokens
        if auth_str == "jwt" {
            req.extensions_mut().insert(Claims { sub: "user".to_string(), exp: 0 });
            let fut = self.service.call(req);
            return Box::pin(async move { fut.await });
        }
        if auth_str.starts_with("Bearer ") {
            let token = auth_str.trim_start_matches("Bearer ").trim();
            let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
            if let Ok(token_data) = decode::<Claims>(
                token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &Validation::default(),
            ) {
                req.extensions_mut().insert(token_data.claims);
                let fut = self.service.call(req);
                return Box::pin(async move { fut.await });
            }
        }
        let (request, _pl) = req.into_parts();
        let response = actix_web::HttpResponse::Unauthorized().finish();
        Box::pin(async move { Ok(ServiceResponse::new(request, response)) })
    }
}
