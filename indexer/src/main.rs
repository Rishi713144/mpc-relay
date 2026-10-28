mod api;
mod indexer;
mod rpc;
mod tracker;

use std::sync::Arc;

use anyhow::Context;
use solana_pubkey::Pubkey;
use store::Store;
use tokio::sync::mpsc;

const DEFAULT_RPC_URL: &str = "https://api.devnet.solana.com";
const DEFAULT_YELLOWSTONE_ENDPOINT: &str = "https://solana-yellowstone-grpc.publicnode.com:443";
const DEFAULT_PORT: u16 = 9000;

fn env_or(name: &str, fallback: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .context("DATABASE_URL must point at the main (backend) database")?;

    let rpc_url = env_or("SOLANA_RPC_URL", DEFAULT_RPC_URL);
    let endpoint = env_or("YELLOWSTONE_ENDPOINT", DEFAULT_YELLOWSTONE_ENDPOINT);
    let port = env_or("INDEXER_PORT", &env_or("PORT", &DEFAULT_PORT.to_string()))
        .parse()
        .unwrap_or(DEFAULT_PORT);

    let store = Arc::new(
        Store::connect(&database_url)
            .await
            .context("failed to connect to Postgres")?,
    );

    let users = store
        .list_users_with_public_key()
        .await
        .context("failed to read existing wallets")?;

    let initial_wallets: Vec<Pubkey> = users
        .iter()
        .filter_map(|user| user.public_key.as_deref())
        .filter_map(|address| address.parse().ok())
        .collect();

    println!(
        "indexer: bootstrapping {} wallet(s) from the database",
        initial_wallets.len()
    );

    let (commands, receiver) = mpsc::channel(128);

    std::thread::spawn(move || {
        let system = actix_web::rt::System::new();
        if let Err(error) = system.block_on(api::run(port, commands)) {
            eprintln!("indexer: control API stopped: {error}");
        }
    });

    indexer::run(
        store,
        rpc::RpcClient::new(rpc_url),
        endpoint,
        initial_wallets,
        receiver,
    )
    .await;

    Ok(())
}
