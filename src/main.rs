use anyhow::Result;
use mcp_rust_sdk::{
    client::Client,
    transport::stdio::StdioTransport,
    types::{ClientCapabilities, Implementation},
};
use serde_json::json;
use std::sync::Arc;
use tokio::process::Command;

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("Starting MCP client");

    let mut child = Command::new("uv")
        .arg("--directory")
        .arg("/Users/darin/shit/notes_simple")
        .arg("run")
        .arg("notes-simple")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let child_stdout = child.stdout.take().expect("Failed to take child stdout");
    let child_stdin = child.stdin.take().expect("Failed to take child stdin");

    let transport = StdioTransport::with_streams(child_stdout, child_stdin)?;
    let client = Client::new(Arc::new(transport));

    let implementation = Implementation {
        name: "notes-client".to_string(),
        version: "0.1.0".to_string(),
    };
    let capabilities = ClientCapabilities::default();

    eprintln!(
        "Initializing client with implementation: {:?}, capabilities: {:?}",
        implementation, capabilities
    );

    match client.initialize(implementation, capabilities).await {
        Ok(response) => eprintln!("Initialization successful: {:?}", response),
        Err(e) => {
            eprintln!("Initialization failed: {:?}", e);
            return Err(e.into());
        }
    }

    // Your requests follow as before...

    eprintln!("Shutting down client...");
    if let Err(e) = client.shutdown().await {
        eprintln!("Error during shutdown: {:?}", e);
    }

    eprintln!("Waiting for server to exit...");
    if let Ok(status) = child.wait().await {
        eprintln!("Server exited with status: {:?}", status);
    }

    eprintln!("Client terminated");
    Ok(())
}