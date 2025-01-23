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

The servers used with this client should implement the protocol to specification. 

Tool call responses error out if the wrong schema is used or the server returns an error.


### Typed Convenience Methods

The `Client` provides typed methods to interact with the server:

- `list_resources() -> Result<ListResourcesResult, Error>`
- `call_tool(name, arguments) -> Result<CallToolResult, Error>`
- `read_resource(uri) -> Result<ReadResourceResult, Error>`

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

Please @ darinkishore in the PR if you do send one over. 


### Credits
- [MCP Rust SDK](https://github.com/Derek-X-Wang/mcp-rust-sdk)
- [AIchat](https://github.com/sigoden/aichat/tree/main)


### License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
