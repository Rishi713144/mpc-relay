use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use serde_json::json;
use store::user::{CreateUserRequest, UserError};

use crate::middleware::{Claims, create_token};
use crate::state::AppState;

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

fn http_error(status: actix_web::http::StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message.into() }))
}

fn unauthorized(message: &str) -> HttpResponse {
    http_error(actix_web::http::StatusCode::UNAUTHORIZED, message)
}

fn internal(message: impl Into<String>) -> HttpResponse {
    http_error(
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        message,
    )
}

fn bad_request(message: impl Into<String>) -> HttpResponse {
    http_error(actix_web::http::StatusCode::BAD_REQUEST, message)
}

#[actix_web::post("/signup")]
pub async fn sign_up(
    state: web::Data<AppState>,
    req: web::Json<SignUpRequest>,
) -> HttpResponse {
    let user = match state
        .store
        .create_user(CreateUserRequest {
            email: req.username.clone(),
            password: req.password.clone(),
        })
        .await
    {
        Ok(user) => user,
        Err(UserError::UserExists) => return unauthorized("user already exists"),
        Err(UserError::InvalidInput(message)) => return bad_request(message),
        Err(error) => return internal(format!("failed to create user: {error}")),
    };

    // Create one key share per MPC node, then derive the user's address.
    let shares = match state.mpc.generate(&user.id).await {
        Ok(shares) => shares,
        Err(error) => {
            return internal(format!("MPC key generation failed: {error}"));
        }
    };

    let address = match state.mpc.aggregate_keys(&shares).await {
        Ok(address) => address,
        Err(error) => return internal(format!("MPC key aggregation failed: {error}")),
    };

    if let Err(error) = state.store.set_public_key(&user.id, &address).await {
        return internal(format!("failed to store wallet address: {error}"));
    }

    subscribe_indexer(&state, &address).await;

    HttpResponse::Ok().json(SignupResponse {
        message: "signed up successfully".to_string(),
    })
}

#[actix_web::post("/signin")]
pub async fn sign_in(
    state: web::Data<AppState>,
    req: web::Json<SignInRequest>,
) -> HttpResponse {
    let user = match state
        .store
        .authenticate(&req.username, &req.password)
        .await
    {
        Ok(user) => user,
        Err(UserError::InvalidCredentials) => return unauthorized("invalid credentials"),
        Err(error) => return internal(format!("failed to sign in: {error}")),
    };

    match create_token(&user.id, &state.config.jwt_secret) {
        Ok(token) => HttpResponse::Ok().json(SigninResponse { token }),
        Err(error) => internal(format!("failed to issue token: {error}")),
    }
}

#[actix_web::get("/user")]
pub async fn get_user(state: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
    let Some(claims) = req.extensions().get::<Claims>().cloned() else {
        return unauthorized("unauthorized");
    };

    match state.store.get_user_by_id(&claims.sub).await {
        Ok(Some(user)) => HttpResponse::Ok().json(UserResponse { email: user.email }),
        Ok(None) => unauthorized("unauthorized"),
        Err(error) => internal(format!("failed to load user: {error}")),
    }
}

/// Tell the indexer to start watching this wallet. Failure is non-fatal: the
/// indexer also bootstraps from the database on startup.
async fn subscribe_indexer(state: &AppState, address: &str) {
    let Some(url) = state.config.indexer_url.as_deref() else {
        return;
    };

    let result = state
        .http
        .post(format!("{url}/subscribe"))
        .json(&json!({ "pubkeys": [address] }))
        .send()
        .await;

    if let Err(error) = result {
        eprintln!("warning: could not notify indexer: {error}");
    }
}
