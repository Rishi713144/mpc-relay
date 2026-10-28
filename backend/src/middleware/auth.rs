use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage, HttpResponse};
use futures::future::{LocalBoxFuture, Ready, ready};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

/// Returns the configured signing secret, rejecting a missing or blank value.
pub fn jwt_secret() -> Option<String> {
    std::env::var("JWT_SECRET")
        .ok()
        .filter(|secret| !secret.trim().is_empty())
}

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
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
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(
        &self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let auth_str = req
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok());

        if let Some(token) = auth_str.and_then(|value| value.strip_prefix("Bearer ")) {
            let Some(secret) = jwt_secret() else {
                let (request, _pl) = req.into_parts();
                return Box::pin(async move {
                    let response = HttpResponse::InternalServerError().finish();
                    Ok(ServiceResponse::new(request, response).map_into_right_body())
                });
            };

            if let Ok(token_data) = decode::<Claims>(
                token.trim(),
                &DecodingKey::from_secret(secret.as_bytes()),
                &Validation::default(),
            ) {
                req.extensions_mut().insert(token_data.claims);
                let fut = self.service.call(req);
                return Box::pin(async move { fut.await.map(ServiceResponse::map_into_left_body) });
            }
        }

        let (request, _pl) = req.into_parts();
        Box::pin(async move {
            let response = HttpResponse::Unauthorized().finish();
            Ok(ServiceResponse::new(request, response).map_into_right_body())
        })
    }
}
