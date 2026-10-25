use yellowstone::GeyserGrpcClient;
pub mod yellowstone;

#[tokio::main]
async fn main() {   
    let client = GeyserGrpcClient::new(HealthClient::new(), GeyserClient::new());
    client.health_check().await;

    


}
