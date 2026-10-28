use actix_web::{web, App, HttpServer};

mod middleware;
mod routes;
use routes::*;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(
                web::scope("/api/v1")
                    .service(sign_up)
                    .service(sign_in)
                    .service(get_user)
                    .service(quote)
                    .service(swap)
                    .service(send)
                    .service(sol_balance)
                    .service(token_balances)
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
