# MCP Rust STDIO SDK 

The core SDK is based on https://github.com/Derek-X-Wang/mcp-rust-sdk.

I added some convenience methods and a builder pattern to simplify connecting to MCP servers that follow the MCP specification.

Also did some big debugging on the client side. STDIO had a lot of issues at first, so fixed those.

This is intended to be upstreamed when I have the time. 

The main modifications are:
- Some more types
- Ergonomics
- Fuckton of error handling/debugging
- STDIO focused. Don't handle websockets at all

enjoy!

**Key Features:**
- **Core MCP Protocol Support:** Implements the core MCP protocol, allowing you to initialize, list resources, read resources, and call tools from MCP-compatible servers.
- **Transport Flexibility:** Uses `stdio` transport by default, making it simple to spawn and connect to a subprocess MCP server. You can also implement custom transports for other communication methods.
- **Typed Convenience Methods:** Offers typed return values and helper functions (e.g., `list_resources()`, `call_tool()`) so you don't have to manually serialize and deserialize JSON.
- **Builder Pattern for Initialization:** A `ClientBuilder` streamlines the process of starting a server, setting capabilities, and initializing the connection.
- **Extensible and Composable:** The SDK is designed to be easily integrated with your own Rust code and adapted for custom use cases.

## Table of Contents
- [MCP Rust STDIO SDK](#mcp-rust-stdio-sdk)
  - [Table of Contents](#table-of-contents)
  - [Getting Started](#getting-started)
  - [Usage](#usage)
    - [Spawning the Server](#spawning-the-server)
    - [Initializing the Client](#initializing-the-client)
    - [Typed Convenience Methods](#typed-convenience-methods)
  - [Advanced Topics](#advanced-topics)
  - [Contributing](#contributing)
  - [License](#license)

## Getting Started

1. **Add Dependencies:**  
   In your `Cargo.toml`, add:
   ```toml
   [dependencies]
   mcp-rust-sdk = { git = "https://github.com/yourusername/mcp-rust-sdk.git", branch = "main" }
   anyhow = "1.0"
   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
   serde = { version = "1", features = ["derive"] }
   serde_json = "1"
   ```

2. **Server Requirements:**  
   You'll need an MCP-compliant server. The MCP protocol is language-agnostic. If you don't have your own server, you can use a default MCP server implemented in Python or any other language. Make sure it supports `stdin`/`stdout` communication and MCP initialization.

## Usage

### Spawning the Server

You can either:
- **Manually start the server:** Start it in one terminal and pipe output to/from your client.
- **Use the builder to spawn automatically:** The `ClientBuilder` allows you to spawn a subprocess server easily, attaching to its `stdin` and `stdout`.

Example:
```rust
use mcp_rust_sdk::client::ClientBuilder;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let client = ClientBuilder::new("uv")
        .directory("/path/to/server_directory")
        .arg("run")
        .arg("notes-simple")
        .implementation("my-amazing-client", "1.0.0")
        .spawn_and_initialize().await?;

    // Use the client...
    Ok(())
}
```

### Initializing the Client

The `ClientBuilder` automatically calls `initialize()` for you. If you want more control, you can manually initialize the `Client` by creating a transport and calling `initialize()` yourself:

```rust
use mcp_rust_sdk::{
    client::Client,
    transport::stdio::StdioTransport,
    types::{ClientCapabilities, Implementation},
};
use tokio::process::{Command, Stdio};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Spawn the server process
    let mut child = Command::new("your_server_command")
        .args(&["--option", "value"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    
    let child_stdout = child.stdout.take().unwrap();
    let child_stdin = child.stdin.take().unwrap();

    let transport = StdioTransport::with_streams(child_stdout, child_stdin)?;
    let client = Client::new(transport);

    let implementation = Implementation {
        name: "notes-client".to_string(),
        version: "0.1.0".to_string(),
    };
    let capabilities = ClientCapabilities::default();

    client.initialize(implementation, capabilities).await?;

    // Client is ready to use
    Ok(())
}
```

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

## Advanced Topics

- **Custom Transports:**  
  If you don't want to use `stdio`, implement the `Transport` trait yourself. This lets you connect over TCP, WebSockets, or any other medium.
  
- **Custom Deserialization Logic:**  
  If the server's output doesn't exactly match MCP or you need more control, implement custom `Deserialize` logic for certain responses.

- **Error Handling:**  
  The `Error` type included is flexible. Integrate it with `anyhow` or your own error types for robust error handling strategies.

## Contributing

Contributions are welcome! Please open an issue or submit a PR if you have improvements, bug fixes, or new features to propose.

1. Fork the repo
2. Create a new branch
3. Add your changes and tests
4. Submit a Pull Request

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.
