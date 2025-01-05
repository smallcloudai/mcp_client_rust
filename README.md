# Model Context Protocol (MCP) Rust SDK

A Rust implementation of the **Client** of the Model Context Protocol (MCP), designed for seamless communication between AI models and their runtime environments.

## Usage

### Spawning the Server

The `ClientBuilder` allows you to spawn a subprocess server easily, attaching to its `stdin` and `stdout`. 

Minimal Working Example:
```rust
use mcp_rust_sdk::client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new("uvx")
        .arg("notes-simple")
        .spawn_and_initialize().await?;
    Ok(())
}
```

Note this won't work for any remote servers: they're not running locally. 

Remote server support is unplanned. 

### Spec Compliance

There's ambiguity in the spec around the tool call response handling. 

I've chosen to only treat JSON-RPC errors as Error types, so if the LLM or the server's business logic messes up, the LLM will be able to choose how to handle it. 

see: https://spec.modelcontextprotocol.io/specification/server/tools/#error-handling

Resources should be fully implemented. 

### Typed Convenience Methods

The `Client` provides typed methods to interact with the server:

- `list_resources() -> Result<ListResourcesResult, Error>`
- `call_tool(name, arguments) -> Result<CallToolResult, Error>`
- `read_resource(uri) -> Result<ReadResourceResult, Error>`

This spares you the hassle of manually constructing JSON requests and parsing raw JSON responses.

For example:
```rust
let resources = client.list_resources().await?;
println!("Resources: {:?}", resources);

let tool_result = client.call_tool("add-note", serde_json::json!({
    "name": "my_first_note",
    "content": "This is some note content."
})).await?;
println!("Tool result: {:?}", tool_result);

let read_result = client.read_resource("note://internal/my_first_note").await?;
println!("Read resource: {:?}", read_result);
```

## Contributing

Contributions are welcome! Please open an issue or submit a PR if you have improvements, bug fixes, or new features to propose.

1. Fork the repo
2. Create a new branch
3. Add your changes and tests
4. Submit a Pull Request


### Credits
- [MCP Rust SDK](https://github.com/Derek-X-Wang/mcp-rust-sdk)
- [AIchat](https://github.com/sigoden/aichat/tree/main)


### License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
