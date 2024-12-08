# Model Context Protocol (MCP) Rust SDK

> ⚠️ **Warning**: This SDK is currently a work in progress and is not ready for production use.

A Rust implementation of the Model Context Protocol (MCP), designed for seamless communication between AI models and their runtime environments.

THIS IS BASED ON https://github.com/Derek-X-Wang/mcp-rust-sdk.


[![Rust CI/CD](https://github.com/Derek-X-Wang/mcp-rust-sdk/actions/workflows/rust.yml/badge.svg)](https://github.com/Derek-X-Wang/mcp-rust-sdk/actions/workflows/rust.yml)


## Features

- Full implementation of MCP protocol specification
- Multiple transport layers (WebSocket, stdio)
- Async/await support using Tokio
- Type-safe message handling
- Comprehensive error handling
- Zero-copy serialization/deserialization

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
mcp_rust_sdk = "0.1.0"
```

## Quick Start

### Using the Client Builder

```rust
use mcp_rust_sdk::client::ClientBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create and initialize a client
    let client = ClientBuilder::new("path/to/server")
        .arg("--some-flag")
        .directory("working/dir")
        .implementation("my-client", "1.0.0")
        .spawn_and_initialize()
        .await?;
    
    // Use the client
    let resources = client.list_resources().await?;
    let prompts = client.list_prompts().await?;
    
    Ok(())
}
```

### Manual Client Setup

```rust
use std::sync::Arc;
use mcp_rust_sdk::{
    client::Client,
    transport::stdio::StdioTransport,
    types::{Implementation, ClientCapabilities},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up transport
    let transport = StdioTransport::with_streams(stdin, stdout)?;
    
    // Create client
    let client = Client::new(Arc::new(transport));
    
    // Initialize
    let implementation = Implementation {
        name: "my-client".to_string(),
        version: "1.0.0".to_string(),
    };
    client.initialize(implementation, ClientCapabilities::default()).await?;
    
    Ok(())
}
```

## Transport Layers

The SDK supports multiple transport mechanisms:

### stdio Transport
- Designed for local process communication
- Uses standard input/output streams
- Ideal for command-line tools and local development

### WebSocket Transport (Coming Soon)
- Network-based communication
- Support for secure (WSS) and standard (WS) connections
- Built-in reconnection handling

## Error Handling

The SDK provides comprehensive error handling through the `Error` type:

```rust
use mcp_rust_sdk::Error;

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err(Error::Protocol { code, message, .. }) => {
        println!("Protocol error {}: {}", code, message)
    },
    Err(Error::Transport(e)) => println!("Transport error: {}", e),
    Err(e) => println!("Other error: {}", e),
}
```

## Available Operations

The client supports the following operations:

- `initialize()` - Initialize the client with the server
- `list_resources()` - Get available resources
- `list_prompts()` - Get available prompts
- `read_resource()` - Read a specific resource
- `complete()` - Complete a prompt
- `call_tool()` - Call a server-side tool

Of these, I don't think list_resources and list_prompts really work.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 
