use mcp_rust_sdk::client::ClientBuilder;
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = ClientBuilder::new("uv")
        .directory("/Users/darin/shit/notes_simple")
        .arg("run")
        .arg("notes-simple")
        .implementation("my-amazing-client", "1.0.0")
        .spawn_and_initialize()
        .await?;

    let resources = client.list_resources().await?;
    println!("Resources: {:?}", resources);

    let tool_result = client.call_tool("add-note", json!({
        "name": "my_first_note",
        "content": "This is some note content."
    })).await?;

    println!("Tool result: {:?}", tool_result);

    let read_result = client.read_resource("note://internal/my_first_note").await?;
    println!("Read resource: {:?}", read_result);
    
    // This just closes the transport.
    client.shutdown().await?;
    Ok(())
}
