use actix_web::{App, HttpResponse, HttpServer, web};
use store::Store;

mod config;
mod middleware;
mod mpc;
mod rpc;
mod routes;
mod solana_utils;
mod state;

use config::Config;
use middleware::AuthMiddleware;
use routes::*;
use state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let config = Config::from_env().unwrap_or_else(|error| {
        eprintln!("configuration error: {error}");
        std::process::exit(1);
    });

    let store = Store::connect(&config.database_url)
        .await
        .unwrap_or_else(|error| {
            eprintln!("failed to connect to the database: {error}");
            std::process::exit(1);
        });

    let port = config.port;
    let state = web::Data::new(AppState::new(store, config));

    println!("backend listening on http://0.0.0.0:{port}/api/v1");

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(
                web::scope("/api/v1")
                    .service(sign_up)
                    .service(sign_in)
                    .route("/health", web::get().to(health)),
            )
            .service(
                web::scope("/api/v1")
                    .wrap(AuthMiddleware)
                    .service(get_user)
                    .service(quote)
                    .service(swap)
                    .service(send)
                    .service(sol_balance)
                    .service(token_balances),
            )
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}
