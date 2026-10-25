use actix_web::{web, HttpResponse, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct QuoteRequest {
}

#[derive(Serialize, Deserialize)]
pub struct QuoteResponse {
}


#[derive(Deserialize)]
pub struct SwapRequest {
}

#[derive(Serialize)]
pub struct SwapResponse {
}

#[derive(Serialize)]
pub struct BalanceResponse {
}

#[derive(Serialize)]
pub struct TokenBalanceResponse {
}

#[actix_web::post("/quote")]
pub async fn quote(req: web::Json<QuoteRequest>) -> Result<HttpResponse> {
    let response = QuoteResponse {};
    
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::post("/swap")]
pub async fn swap(req: web::Json<SwapRequest>) -> Result<HttpResponse> {
    
    let response = SwapResponse {};
    
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::get("/sol-balance/{pubkey}")]
pub async fn sol_balance() -> Result<HttpResponse> {
    
    let response = BalanceResponse {
    };
    
    Ok(HttpResponse::Ok().json(response))
}

#[actix_web::get("/token-balance/{pubkey}/{mint}")]
pub async fn token_balance() -> Result<HttpResponse> {    
    
    let response = TokenBalanceResponse {
        
    };
    
    Ok(HttpResponse::Ok().json(response))
}
