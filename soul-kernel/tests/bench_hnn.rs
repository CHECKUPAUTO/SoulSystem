use soul_kernel::hnn_bridge::HnnState;
use std::time::Instant;

#[tokio::test]
async fn test_fetch_performance() {
    let client = reqwest::Client::new();
    let start = Instant::now();
    let _state = HnnState::fetch(&client).await;
    let duration = start.elapsed();
    println!("HnnState::fetch took: {:?}", duration);
}
