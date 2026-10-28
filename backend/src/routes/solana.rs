#![allow(deprecated)]

use std::str::FromStr;

use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signature::Signature;

use solana_system_interface::instruction as system_instruction;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction::Transaction;
use store::balances::SOL_MINT;
use uuid::Uuid;

use crate::middleware::Claims;
use crate::state::{AppState, StoredQuote};

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

#[derive(Deserialize)]
pub struct SendRequest {
    pub to: String,
    pub amount: u64,
    pub mint: Option<String>,
}

fn error(status: actix_web::http::StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(json!({ "error": message.into() }))
}

fn bad_request(message: impl Into<String>) -> HttpResponse {
    error(actix_web::http::StatusCode::BAD_REQUEST, message)
}

fn unauthorized() -> HttpResponse {
    error(actix_web::http::StatusCode::UNAUTHORIZED, "unauthorized")
}

fn internal(message: impl Into<String>) -> HttpResponse {
    error(
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        message,
    )
}

fn claims(req: &HttpRequest) -> Result<Claims, HttpResponse> {
    req.extensions().get::<Claims>().cloned().ok_or_else(unauthorized)
}

fn is_sol_mint(mint: &str) -> bool {
    mint == SOL_MINT || mint == crate::solana_utils::NATIVE_MINT
}

struct WalletUser {
    id: String,
    wallet: Pubkey,
}

async fn wallet_user(state: &AppState, claims: &Claims) -> Result<WalletUser, HttpResponse> {
    let user = state
        .store
        .get_user_by_id(&claims.sub)
        .await
        .map_err(|error| internal(format!("failed to load user: {error}")))?
        .ok_or_else(unauthorized)?;

    let wallet = user
        .public_key
        .ok_or_else(|| bad_request("user has no wallet address yet"))?;

    let wallet = Pubkey::from_str(&wallet).map_err(|_| internal("stored wallet is invalid"))?;

    Ok(WalletUser {
        id: user.id,
        wallet,
    })
}

#[actix_web::post("/quote")]
pub async fn quote(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<QuoteRequest>,
) -> HttpResponse {
    let claims = match claims(&req) {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    if body.in_amount == 0 {
        return bad_request("inAmount must be greater than zero");
    }
    if body.input_mint == body.output_mint {
        return bad_request("inputMint and outputMint must differ");
    }

    let user = match wallet_user(&state, &claims).await {
        Ok(user) => user,
        Err(response) => return response,
    };

    let balance = if is_sol_mint(&body.input_mint) {
        state.store.get_sol_balance(&user.id).await
    } else {
        state
            .store
            .get_balance_for_mint(&user.id, &body.input_mint)
            .await
    };

    match balance {
        Ok(balance) if (balance as u64) < body.in_amount => {
            return bad_request("insufficient balance");
        }
        Ok(_) => {}
        Err(error) => return internal(format!("failed to read balance: {error}")),
    }

    let url = format!("{}/quote", state.config.jupiter_base_url);
    let response = state
        .http
        .get(&url)
        .query(&[
            ("inputMint", body.input_mint.as_str()),
            ("outputMint", body.output_mint.as_str()),
            ("amount", &body.in_amount.to_string()),
            ("slippageBps", "50"),
        ])
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => return internal(format!("Jupiter quote request failed: {error}")),
    };

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return bad_request(format!("Jupiter rejected the quote ({status}): {text}"));
    }

    let quote: Value = match response.json().await {
        Ok(value) => value,
        Err(error) => return internal(format!("invalid Jupiter quote response: {error}")),
    };

    // Spec: return the worst price the user can receive (slippage maxed out).
    let out_amount = quote["otherAmountThreshold"]
        .as_str()
        .and_then(|value| value.parse::<u64>().ok())
        .or_else(|| {
            quote["outAmount"]
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .map(|value| value.saturating_mul(9_950) / 10_000)
        })
        .unwrap_or(0);

    if out_amount == 0 {
        return bad_request("Jupiter returned no route for this pair");
    }

    let id = Uuid::new_v4();
    state.quotes.insert(
        id,
        StoredQuote {
            user_id: user.id,
            input_mint: body.input_mint.clone(),
            in_amount: body.in_amount,
            quote,
        },
    );

    HttpResponse::Ok().json(QuoteResponse {
        out_amount,
        id: id.to_string(),
    })
}

#[actix_web::post("/swap")]
pub async fn swap(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<SwapRequest>,
) -> HttpResponse {
    let claims = match claims(&req) {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    let id = match Uuid::parse_str(&body.id) {
        Ok(id) => id,
        Err(_) => return bad_request("unknown quote id"),
    };

    let stored = match state.quotes.remove(&id) {
        Some((_, stored)) => stored,
        None => return bad_request("unknown or expired quote id"),
    };

    if stored.user_id != claims.sub {
        return bad_request("quote does not belong to this user");
    }

    let user = match wallet_user(&state, &claims).await {
        Ok(user) => user,
        Err(response) => return response,
    };

    let swap_request = json!({
        "quoteResponse": stored.quote,
        "userPublicKey": user.wallet.to_string(),
        "wrapAndUnwrapSol": true,
        "dynamicComputeUnitLimit": true,
    });

    let response = state
        .http
        .post(format!("{}/swap", state.config.jupiter_base_url))
        .json(&swap_request)
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => return internal(format!("Jupiter swap request failed: {error}")),
    };

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return bad_request(format!("Jupiter rejected the swap ({status}): {text}"));
    }

    let swap: Value = match response.json().await {
        Ok(value) => value,
        Err(error) => return internal(format!("invalid Jupiter swap response: {error}")),
    };

    let encoded = match swap["swapTransaction"].as_str() {
        Some(value) => value,
        None => return internal("Jupiter returned no swap transaction"),
    };

    let bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded) {
        Ok(bytes) => bytes,
        Err(error) => return internal(format!("swap transaction is not base64: {error}")),
    };

    let mut transaction: VersionedTransaction = match bincode::deserialize(&bytes) {
        Ok(transaction) => transaction,
        Err(error) => return internal(format!("swap transaction is malformed: {error}")),
    };

    let blockhash = match crate::rpc::latest_blockhash(&state.http, &state.config.rpc_url).await {
        Ok(blockhash) => blockhash,
        Err(error) => return internal(error),
    };

    match &mut transaction.message {
        VersionedMessage::Legacy(message) => message.recent_blockhash = blockhash,
        VersionedMessage::V0(message) => message.recent_blockhash = blockhash,
    }

    let message = transaction.message.serialize();

    let signature = match state
        .mpc
        .sign_message(&user.id, &message)
        .await
    {
        Ok(signature) => signature,
        Err(error) => return internal(format!("MPC signing failed: {error}")),
    };

    if !verify(&signature, &user.wallet, &message) {
        return internal("MPC returned a signature that does not match the wallet");
    }

    if transaction.signatures.is_empty() {
        transaction.signatures.push(signature);
    } else {
        transaction.signatures[0] = signature;
    }

    match crate::rpc::send_versioned(&state.http, &state.config.rpc_url, &transaction).await {
        Ok(signature) => HttpResponse::Ok().json(json!({
            "status": "ok",
            "signature": signature.to_string(),
            "inputMint": stored.input_mint,
            "inAmount": stored.in_amount,
        })),
        Err(error) => internal(format!("failed to submit swap: {error}")),
    }
}

fn verify(signature: &Signature, wallet: &Pubkey, message: &[u8]) -> bool {
    signature.verify(wallet.as_ref(), message)
}

#[actix_web::post("/send")]
pub async fn send(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<SendRequest>,
) -> HttpResponse {
    let claims = match claims(&req) {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    if body.amount == 0 {
        return bad_request("amount must be greater than zero");
    }

    let user = match wallet_user(&state, &claims).await {
        Ok(user) => user,
        Err(response) => return response,
    };

    let to = match Pubkey::from_str(&body.to) {
        Ok(to) => to,
        Err(_) => return bad_request("invalid 'to' address"),
    };

    let is_sol = body.mint.as_deref().map(is_sol_mint).unwrap_or(true);

    let instructions = if is_sol {
        let balance = match state.store.get_sol_balance(&user.id).await {
            Ok(balance) => balance,
            Err(error) => return internal(format!("failed to read balance: {error}")),
        };

        if (balance as u64) < body.amount {
            return bad_request("insufficient balance");
        }

        vec![system_instruction::transfer(
            &user.wallet,
            &to,
            body.amount,
        )]
    } else {
        let mint_address = body.mint.as_deref().unwrap_or_default();
        let mint = match Pubkey::from_str(mint_address) {
            Ok(mint) => mint,
            Err(_) => return bad_request("invalid mint address"),
        };

        let balance = match state
            .store
            .get_balance_for_mint(&user.id, mint_address)
            .await
        {
            Ok(balance) => balance,
            Err(error) => return internal(format!("failed to read balance: {error}")),
        };

        if (balance as u64) < body.amount {
            return bad_request("insufficient balance");
        }

        let mint_account = match crate::rpc::account(&state.http, &state.config.rpc_url, &mint).await
        {
            Ok(Some(account)) => account,
            Ok(None) => return bad_request("mint account not found"),
            Err(error) => return internal(error),
        };

        let decimals = match crate::solana_utils::parse_mint_decimals(&mint_account.data) {
            Some(decimals) => decimals,
            None => return internal("mint account is malformed"),
        };

        let source = crate::solana_utils::associated_token_address(&user.wallet, &mint);
        let destination = crate::solana_utils::associated_token_address(&to, &mint);

        vec![
            // Make the destination token account exist; no-op when it already does.
            crate::solana_utils::create_ata_idempotent_ix(&user.wallet, &to, &mint),
            crate::solana_utils::transfer_checked_ix(
                &source,
                &mint,
                &destination,
                &user.wallet,
                body.amount,
                decimals,
            ),
        ]
    };

    let blockhash = match crate::rpc::latest_blockhash(&state.http, &state.config.rpc_url).await {
        Ok(blockhash) => blockhash,
        Err(error) => return internal(error),
    };

    let message = Message::new(&instructions, Some(&user.wallet));
    let mut transaction = Transaction::new_unsigned(message);
    transaction.message.recent_blockhash = blockhash;

    let message_bytes = transaction.message.serialize();

    let signature = match state.mpc.sign_message(&user.id, &message_bytes).await {
        Ok(signature) => signature,
        Err(error) => return internal(format!("MPC signing failed: {error}")),
    };

    if !verify(&signature, &user.wallet, &message_bytes) {
        return internal("MPC returned a signature that does not match the wallet");
    }

    transaction.signatures[0] = signature;

    if transaction.verify().is_err() {
        return internal("assembled transaction failed verification");
    }

    match crate::rpc::send_legacy(&state.http, &state.config.rpc_url, &transaction).await {
        Ok(signature) => HttpResponse::Ok().json(json!({
            "status": "ok",
            "signature": signature.to_string(),
            "mint": body.mint.clone().unwrap_or_else(|| SOL_MINT.to_string()),
        })),
        Err(error) => internal(format!("failed to submit transfer: {error}")),
    }
}

#[actix_web::get("/balance/sol")]
pub async fn sol_balance(state: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
    let claims = match claims(&req) {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    match state.store.get_sol_balance(&claims.sub).await {
        Ok(balance) => HttpResponse::Ok().json(json!({ "balance": balance })),
        Err(error) => internal(format!("failed to read balance: {error}")),
    }
}

#[actix_web::get("/balance/tokens")]
pub async fn token_balances(state: web::Data<AppState>, req: HttpRequest) -> HttpResponse {
    let claims = match claims(&req) {
        Ok(claims) => claims,
        Err(response) => return response,
    };

    match state.store.get_token_balances(&claims.sub).await {
        Ok(balances) => HttpResponse::Ok().json(balances),
        Err(error) => internal(format!("failed to read balances: {error}")),
    }
}
