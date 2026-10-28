use std::rc::Rc;

use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{Error, HttpMessage, HttpResponse};
use chrono::{Duration, Utc};
use futures::future::{LocalBoxFuture, Ready, ready};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Create a 24h token for a user id.
pub fn create_token(user_id: &str, secret: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let exp = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user_id.to_string(),
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Accept both `Authorization: Bearer <token>` and the `jwt <token>` form used
/// by the assignment spec.
fn extract_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    let scheme = scheme.trim();
    let token = token.trim();

    if token.is_empty() {
        return None;
    }

    if scheme.eq_ignore_ascii_case("bearer") || scheme.eq_ignore_ascii_case("jwt") {
        Some(token)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_both_schemes() {
        assert_eq!(extract_token("Bearer abc"), Some("abc"));
        assert_eq!(extract_token("jwt abc"), Some("abc"));
        assert_eq!(extract_token("JWT abc"), Some("abc"));
    }

    #[test]
    fn rejects_bare_scheme() {
        assert_eq!(extract_token("jwt"), None);
        assert_eq!(extract_token("Bearer "), None);
        assert_eq!(extract_token("Basic abc"), None);
    }

    #[test]
    fn token_roundtrip() {
        let token = create_token("user-1", "test-secret").unwrap();
        let claims = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"test-secret"),
            &Validation::default(),
        )
        .unwrap()
        .claims;
        assert_eq!(claims.sub, "user-1");
    }
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
        let token = req
            .headers()
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(extract_token)
            .map(str::to_owned);

        if let Some(token) = token {
            let Some(secret) = jwt_secret() else {
                let (request, _payload) = req.into_parts();
                return Box::pin(async move {
                    let response = HttpResponse::InternalServerError()
                        .body("JWT_SECRET is not configured");
                    Ok(ServiceResponse::new(request, response).map_into_right_body())
                });
            };

            if let Ok(token_data) = decode::<Claims>(
                &token,
                &DecodingKey::from_secret(secret.as_bytes()),
                &Validation::default(),
            ) {
                req.extensions_mut().insert(token_data.claims);
                let fut = self.service.call(req);
                return Box::pin(async move { fut.await.map(ServiceResponse::map_into_left_body) });
            }
        }

        let (request, _payload) = req.into_parts();
        Box::pin(async move {
            let response = HttpResponse::Unauthorized().finish();
            Ok(ServiceResponse::new(request, response).map_into_right_body())
        })
    }
}
