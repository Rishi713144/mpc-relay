use actix_web::{web::{post}, App, Error, HttpResponse, HttpServer};

pub mod error;
pub mod serialization;
pub mod tss;

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    HttpServer::new(|| {
        App::new()
        .route("/generate", post().to(generate))
        .route("/send-single", post().to(send_single))
        .route("/aggregate-keys", post().to(aggregate_keys))
        .route("/agg-send-step1", post().to(agg_send_step1))
        .route("/agg-send-step2", post().to(agg_send_step2))
        .route(
            "/aggregate-signatures-broadcast",
            post().to(aggregate_signatures_broadcast),
        )
    })
    
        .bind("127.0.0.1:8080")?
        .run()
        .await
}

async fn generate() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

async fn send_single() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

async fn aggregate_keys() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

async fn agg_send_step1() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

async fn agg_send_step2() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}

async fn aggregate_signatures_broadcast() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::Ok().body("Hello, world!"))
}