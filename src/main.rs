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

    eprintln!("Listing resources...");
    match client.request("resources/list", None).await {
        Ok(resources) => eprintln!("Resources: {}", resources),
        Err(e) => eprintln!("Failed to list resources: {:?}", e),
    }

    eprintln!("Adding a new note via tool...");
    match client.request("tools/call", Some(json!({
        "name": "add-note",
        "arguments": {
            "name": "my_first_note",
            "content": "This is the content of my first note."
        }
    }))).await {
        Ok(response) => eprintln!("Tool response: {}", response),
        Err(e) => eprintln!("Failed to add note: {:?}", e),
    }

    match client.request("resources/list", None).await {
        Ok(resources) => eprintln!("Updated resources: {}", resources),
        Err(e) => eprintln!("Failed to list updated resources: {:?}", e),
    }

    eprintln!("Reading the newly added note...");
    match client.request("resources/read", Some(json!({
        "uri": "note://internal/my_first_note"
    }))).await {
        Ok(content) => eprintln!("Note content: {}", content),
        Err(e) => eprintln!("Failed to read note: {:?}", e),
    }

    eprintln!("Listing prompts...");
    match client.request("prompts/list", None).await {
        Ok(prompts) => eprintln!("Available prompts: {}", prompts),
        Err(e) => eprintln!("Failed to list prompts: {:?}", e),
    }

    eprintln!("Getting a prompt (summarize-notes)...");
    match client.request("prompts/get", Some(json!({
        "name": "summarize-notes",
        "arguments": {
            "style": "brief"
        }
    }))).await {
        Ok(detail) => eprintln!("Prompt detail: {}", detail),
        Err(e) => eprintln!("Failed to get prompt: {:?}", e),
    }

    eprintln!("Client terminated");
    Ok(())
}