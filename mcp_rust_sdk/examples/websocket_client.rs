use std::sync::Arc;
use mcp_rust_sdk::{
    client::Client,
    transport::websocket::WebSocketTransport,
    types::{ClientCapabilities, Implementation},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create WebSocket transport
    let transport = WebSocketTransport::new("ws://127.0.0.1:8780").await?;
    
    // Create MCP client
    let client = Client::new(Arc::new(transport));

    // Initialize client
    let implementation = Implementation {
        name: "example-client".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let capabilities = ClientCapabilities::default();

    println!("Initializing MCP client...");
    let server_capabilities = client.initialize(implementation, capabilities).await?;
    println!("Server capabilities: {:?}", server_capabilities);

    // Send a request
    println!("Sending test request...");
    let response = client
        .request(
            "test/method",
            Some(serde_json::json!({
                "message": "Hello from Rust MCP client!"
            })),
        )
        .await?;
    println!("Received response: {:?}", response);

    // Shutdown client
    println!("Shutting down...");
    client.shutdown().await?;

    Ok(())
}
