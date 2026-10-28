use actix_web::{HttpResponse, Result};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use chrono::{Utc, Duration};

#[derive(Deserialize)]
pub struct SignUpRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SignInRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct SignupResponse {
    pub message: String,
}

#[derive(Serialize)]
pub struct SigninResponse {
    pub token: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[actix_web::post("/signup")]
pub async fn sign_up(req: actix_web::web::Json<SignUpRequest>) -> Result<HttpResponse> {
    let _ = req;
    let response = SignupResponse {
        message: "signed up successfully".to_string(),
    };
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::post("/signin")]
pub async fn sign_in(req: actix_web::web::Json<SignInRequest>) -> Result<HttpResponse> {
    let _ = req;
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let expiration = Utc::now()
        .checked_add_signed(Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;
    let claims = Claims { sub: "user".to_string(), exp: expiration };
    let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .unwrap_or_else(|_| "jwt_token".to_string());
    let response = SigninResponse { token };
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::get("/user")]
pub async fn get_user() -> Result<HttpResponse> {
    let user = UserResponse {
        email: "user_email".to_string(),
    };
    Ok(HttpResponse::Ok().json(user))
}
