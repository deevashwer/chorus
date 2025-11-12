use chorus::network::network_service;

#[tokio::main]
async fn main() {
    network_service().await.unwrap();
}