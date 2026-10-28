use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct QuoteRequest {
    #[serde(rename = "inputMint")]
    pub input_mint: String,
    #[serde(rename = "outputMint")]
    pub output_mint: String,
    #[serde(rename = "inAmount")]
    pub in_amount: u64,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    #[serde(rename = "outAmount")]
    pub out_amount: u64,
    pub id: String,
}

#[derive(Deserialize)]
pub struct SwapRequest {
    pub id: String,
}

#[derive(Serialize)]
pub struct SwapResponse {}

#[derive(Serialize)]
pub struct SolBalanceResponse {
    pub balance: u64,
}

#[derive(Serialize)]
pub struct TokenBalance {
    pub balance: u64,
    #[serde(rename = "tokenMint")]
    pub token_mint: String,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Serialize)]
pub struct TokenBalancesResponse {
    pub balances: Vec<TokenBalance>,
}

#[derive(Deserialize)]
pub struct SendRequest {
    pub to: String,
    pub amount: u64,
    pub mint: Option<String>,
}

#[derive(Serialize)]
pub struct SendResponse {}

#[actix_web::post("/quote")]
pub async fn quote(_req: web::Json<QuoteRequest>) -> Result<HttpResponse> {
    let response = QuoteResponse {
        out_amount: 100000030,
        id: Uuid::new_v4().to_string(),
    };
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::post("/swap")]
pub async fn swap(_req: web::Json<SwapRequest>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(SwapResponse {}))
}

#[actix_web::post("/send")]
pub async fn send(_req: web::Json<SendRequest>) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(SendResponse {}))
}

#[actix_web::get("/balance/sol")]
pub async fn sol_balance() -> Result<HttpResponse> {
    let response = SolBalanceResponse { balance: 10000000 };
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::get("/balance/tokens")]
pub async fn token_balances() -> Result<HttpResponse> {
    let response = TokenBalancesResponse {
        balances: vec![TokenBalance {
            balance: 10000000,
            token_mint: "".to_string(),
            symbol: "USDC".to_string(),
            decimals: 6,
        }],
    };
    Ok(HttpResponse::Ok().json(response))
}
