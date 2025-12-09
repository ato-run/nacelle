use capsuled_engine::proto::onescluster::coordinator::v1::agent_service_client::AgentServiceClient;
use capsuled_engine::proto::onescluster::coordinator::v1::FetchModelRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Wait for engine to start
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let mut client = AgentServiceClient::connect("http://127.0.0.1:50051").await?;

    println!("Sending FetchModel request...");
    let request = tonic::Request::new(FetchModelRequest {
        url: "https://raw.githubusercontent.com/google/go-cmp/master/README.md".to_string(),
        destination: "/tmp/models/llama3/README.md".to_string(),
    });

    let response = client.fetch_model(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
