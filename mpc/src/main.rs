#![allow(deprecated)]

pub mod auth;
pub mod error;
pub mod serialization;
pub mod tss;

use std::str::FromStr;

use actix_web::{App, HttpResponse, HttpServer, web};
use base64::Engine;
use error::Error;
use serde::{Deserialize, Serialize};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use solana_signer::Signer;
use store::Store;

use crate::serialization::{AggMessage1, PartialSignature, SecretAggStepOne, SerializeCustom};

struct AppState {
    store: Store,
    node_id: String,
}

#[derive(Deserialize)]
struct GenerateRequest {
    user_id: String,
}

#[derive(Serialize)]
struct GenerateResponse {
    public_share: String,
}

#[derive(Deserialize)]
struct AggregateKeysRequest {
    pub_keyshare_1: String,
    pub_keyshare_2: String,
    pub_keyshare_3: String,
}

#[derive(Serialize)]
struct AggregateKeysResponse {
    agg_public_key: String,
}

#[derive(Deserialize)]
struct StepOneRequest {
    user_id: String,
}

#[derive(Serialize)]
struct StepOneResponse {
    message: String,
    secret_state: String,
    pub_share: String,
}

#[derive(Deserialize, Default, Clone)]
struct InstructionArgs {
    to: Option<String>,
    /// Lamports for `send_sol`.
    amount: Option<u64>,
    /// Base64 serialized message bytes for `raw`.
    message_base64: Option<String>,
    /// Base64 `VersionedTransaction` for `swap`.
    swap_transaction: Option<String>,
}

#[derive(Deserialize)]
struct StepTwoRequest {
    user_id: String,
    secret_state: String,
    instruction_type: String,
    #[serde(default)]
    instruction_args: Option<InstructionArgs>,
    pub_keyshare_1: String,
    pub_keyshare_2: String,
    pub_keyshare_3: String,
    messages: Vec<String>,
    #[serde(default)]
    recent_blockhash: String,
}

#[derive(Serialize)]
struct StepTwoResponse {
    partial_signature: String,
}

#[derive(Deserialize)]
struct AggregateSignaturesRequest {
    pub_keyshare_1: String,
    pub_keyshare_2: String,
    pub_keyshare_3: String,
    partial_signatures: Vec<String>,
}

#[derive(Serialize)]
struct AggregateSignaturesResponse {
    signature: String,
}

fn parse_keys(a: &str, b: &str, c: &str) -> Result<Vec<Pubkey>, Error> {
    let parse = |value: &str| {
        Pubkey::from_str(value)
            .map_err(|_| Error::BadRequest(format!("invalid public key: {value}")))
    };
    Ok(vec![parse(a)?, parse(b)?, parse(c)?])
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("DATABASE_URL must point at this node's own database");
        std::process::exit(1);
    });

    let store = Store::connect(&database_url)
        .await
        .unwrap_or_else(|error| {
            eprintln!("failed to connect to {database_url}: {error}");
            std::process::exit(1);
        });

    store.init_keyshares().await.unwrap_or_else(|error| {
        eprintln!("failed to initialize keyshares table: {error}");
        std::process::exit(1);
    });

    let node_id = std::env::var("MPC_NODE_ID").unwrap_or_else(|_| "1".to_string());
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8001);

    let state = web::Data::new(AppState { store, node_id });

    println!("MPC node {} listening on 0.0.0.0:{port}", state.node_id);

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(auth::MpcAuth)
            .route("/generate", web::post().to(generate))
            .route("/aggregate-keys", web::post().to(aggregate_keys))
            .route("/agg-send-step1", web::post().to(agg_send_step1))
            .route("/agg-send-step2", web::post().to(agg_send_step2))
            .route("/aggregate-signatures-broadcast", web::post().to(aggregate_signatures_broadcast))
            // Aliases used by other implementations of this assignment.
            .route("/aggregate-step-1", web::post().to(agg_send_step1))
            .route("/aggregate-step-2", web::post().to(agg_send_step2))
            .route("/aggregate-signatures", web::post().to(aggregate_signatures_broadcast))
            .route("/health", web::get().to(health))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

async fn generate(
    state: web::Data<AppState>,
    req: web::Json<GenerateRequest>,
) -> Result<web::Json<GenerateResponse>, Error> {
    if req.user_id.trim().is_empty() {
        return Err(Error::BadRequest("missing user_id".into()));
    }

    let keypair = Keypair::new();
    let public_share = keypair.pubkey().to_string();

    state
        .store
        .save_keyshare(&req.user_id, &public_share, &keypair.to_base58_string())
        .await
        .map_err(|e| Error::InternalServerError(format!("failed to store keyshare: {e}")))?;

    println!(
        "[node {}] generated keyshare for {}",
        state.node_id, req.user_id
    );

    // The secret share never leaves this node.
    Ok(web::Json(GenerateResponse { public_share }))
}

async fn aggregate_keys(
    req: web::Json<AggregateKeysRequest>,
) -> Result<web::Json<AggregateKeysResponse>, Error> {
    let keys = parse_keys(
        &req.pub_keyshare_1,
        &req.pub_keyshare_2,
        &req.pub_keyshare_3,
    )?;
    let agg_public_key = tss::aggregate_public_key(keys)?.to_string();
    Ok(web::Json(AggregateKeysResponse { agg_public_key }))
}

async fn agg_send_step1(
    state: web::Data<AppState>,
    req: web::Json<StepOneRequest>,
) -> Result<web::Json<StepOneResponse>, Error> {
    let keyshare = state
        .store
        .get_keyshare(&req.user_id)
        .await?
        .ok_or_else(|| Error::BadRequest("no keyshare for this user".into()))?;

    let keypair = Keypair::from_base58_string(&keyshare.secret_key);
    let (message, secret_state) = tss::step_one(&keypair);

    Ok(web::Json(StepOneResponse {
        message: message.serialize_bs58(),
        secret_state: secret_state.serialize_bs58(),
        pub_share: keyshare.public_key,
    }))
}

async fn agg_send_step2(
    state: web::Data<AppState>,
    req: web::Json<StepTwoRequest>,
) -> Result<web::Json<StepTwoResponse>, Error> {
    let keys = parse_keys(
        &req.pub_keyshare_1,
        &req.pub_keyshare_2,
        &req.pub_keyshare_3,
    )?;

    let mut first_messages = Vec::with_capacity(req.messages.len());
    for encoded in &req.messages {
        let bytes = bs58::decode(encoded).into_vec()?;
        let message = AggMessage1::deserialize(&bytes)
            .map_err(|error| Error::DeserializationFailed {
                error,
                field_name: "messages",
            })?;
        first_messages.push(message);
    }

    let secret_bytes = bs58::decode(&req.secret_state).into_vec()?;
    let secret_state = SecretAggStepOne::deserialize(&secret_bytes).map_err(|error| {
        Error::DeserializationFailed {
            error,
            field_name: "secret_state",
        }
    })?;

    let keyshare = state
        .store
        .get_keyshare(&req.user_id)
        .await?
        .ok_or_else(|| Error::BadRequest("no keyshare for this user".into()))?;
    let keypair = Keypair::from_base58_string(&keyshare.secret_key);

    let args = req.instruction_args.clone().unwrap_or_default();

    let partial = match req.instruction_type.as_str() {
        "raw" => {
            let encoded = args
                .message_base64
                .as_deref()
                .ok_or_else(|| Error::BadRequest("missing message_base64".into()))?;
            let message = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| Error::BadRequest("message_base64 is not valid base64".into()))?;
            tss::step_two_raw(
                &keypair,
                &message,
                keys,
                first_messages,
                secret_state,
            )?
        }
        "send_sol" => {
            let to = args
                .to
                .as_deref()
                .ok_or_else(|| Error::BadRequest("missing 'to'".into()))?;
            let to = Pubkey::from_str(to)
                .map_err(|_| Error::BadRequest("invalid 'to' address".into()))?;
            let amount = args
                .amount
                .ok_or_else(|| Error::BadRequest("missing 'amount'".into()))?;
            let blockhash = Hash::from_str(&req.recent_blockhash)
                .map_err(|_| Error::BadRequest("invalid recent_blockhash".into()))?;
            tss::step_two(
                &keypair,
                amount,
                to,
                blockhash,
                keys,
                first_messages,
                secret_state,
            )?
        }
        "swap" => {
            let swap_tx = args
                .swap_transaction
                .as_deref()
                .ok_or_else(|| Error::BadRequest("missing swap_transaction".into()))?;
            tss::step_two_swap(&keypair, swap_tx, keys, first_messages, secret_state)?
        }
        other => {
            return Err(Error::BadRequest(format!(
                "unsupported instruction_type '{other}'"
            )));
        }
    };

    Ok(web::Json(StepTwoResponse {
        partial_signature: partial.serialize_bs58(),
    }))
}

async fn aggregate_signatures_broadcast(
    req: web::Json<AggregateSignaturesRequest>,
) -> Result<web::Json<AggregateSignaturesResponse>, Error> {
    let keys = parse_keys(
        &req.pub_keyshare_1,
        &req.pub_keyshare_2,
        &req.pub_keyshare_3,
    )?;

    let mut partials = Vec::with_capacity(req.partial_signatures.len());
    for encoded in &req.partial_signatures {
        let sig = PartialSignature::deserialize_bs58(encoded)
            .map_err(|error| Error::DeserializationFailed {
                error,
                field_name: "partial_signatures",
            })?;
        partials.push(sig);
    }

    let signature: Signature = tss::aggregate_signature(keys, partials)?;
    Ok(web::Json(AggregateSignaturesResponse {
        signature: bs58::encode(signature).into_string(),
    }))
}


