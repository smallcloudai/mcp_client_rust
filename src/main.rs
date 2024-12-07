use anyhow::Result;
use mcp_rust_sdk::{
    client::Client,
    error::Error,
    transport::{Message, Transport},
    types::{ClientCapabilities, Implementation},
};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, stdin, stdout};
use tokio::sync::broadcast;
use futures::Stream;
use std::pin::Pin;
use tokio::time::timeout;
use std::time::Duration;

struct StdioTransport {
    stdin: Arc<tokio::sync::Mutex<tokio::io::Stdout>>,
    tx: broadcast::Sender<Result<Message, Error>>,
}

impl StdioTransport {
    async fn new() -> Result<Self> {
        let (tx, _rx) = broadcast::channel(100);
        let stdin = stdin();
        let stdout = stdout();

        let reader = BufReader::new(stdin);
        let tx_clone = tx.clone();
        
        // Spawn a task to handle stdin
        tokio::spawn(async move {
            let mut lines = reader.lines();
            eprintln!("Starting stdin reader loop");
            while let Ok(line) = lines.next_line().await {
                match line {
                    Some(line) => {
                        eprintln!("Received raw input: {}", line);
                        // Try to parse the line as a Message
                        let msg: Result<Message, Error> = match serde_json::from_str(&line) {
                            Ok(m) => {
                                eprintln!("Successfully parsed message: {:?}", m);
                                Ok(m)
                            }
                            Err(e) => {
                                eprintln!("Failed to parse message: {}", e);
                                Err(Error::Serialization(e.to_string()))
                            }
                        };
                        
                        // Send the parsed message through the channel
                        if let Ok(message) = msg.as_ref() {
                            eprintln!("Sending parsed message through channel: {:?}", message);
                            if let Err(e) = tx_clone.send(msg) {
                                eprintln!("Failed to send message to channel: {}", e);
                                break;
                            }
                        } else {
                            eprintln!("Skipping invalid message");
                        }
                    }
                    None => {
                        eprintln!("Stdin closed, ending reader loop");
                        break;
                    }
                }
            }
            eprintln!("Stdin reader loop ended");
        });

        Ok(Self {
            stdin: Arc::new(tokio::sync::Mutex::new(stdout)),
            tx,
        })
    }
}

#[async_trait::async_trait]
impl Transport for StdioTransport {
    async fn send(&self, message: Message) -> Result<(), Error> {
        let mut stdout = self.stdin.lock().await;
        let json = serde_json::to_string(&message)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        let msg_line = format!("{}\n", json);
        eprintln!("Sending message: {}", msg_line);
        stdout.write_all(msg_line.as_bytes())
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        stdout.flush()
            .await
            .map_err(|e| Error::Io(e.to_string()))?;
        Ok(())
    }

    fn receive(&self) -> Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>> {
        let mut rx = self.tx.subscribe();
        Box::pin(async_stream::stream! {
            eprintln!("Starting receive stream");
            while let Ok(msg) = rx.recv().await {
                eprintln!("Received message in stream: {:?}", msg);
                yield msg;
            }
            eprintln!("Receive stream ended");
        })
    }

    async fn close(&self) -> Result<(), Error> {
        eprintln!("Transport closing");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    eprintln!("Starting MCP client");
    
    let transport = StdioTransport::new().await?;
    let client = Client::new(Arc::new(transport));

    let implementation = Implementation {
        name: "notes-client".to_string(),
        version: "0.1.0".to_string(),
    };
    let capabilities = ClientCapabilities::default();

    eprintln!("Initializing client...");
    match timeout(
        Duration::from_secs(5),
        client.initialize(implementation, capabilities)
    ).await {
        Ok(init_result) => match init_result {
            Ok(response) => eprintln!("Initialization successful: {:?}", response),
            Err(e) => {
                eprintln!("Initialization failed: {:?}", e);
                return Err(e.into());
            }
        },
        Err(_) => {
            eprintln!("Initialization timed out after 5 seconds");
            return Err(anyhow::anyhow!("Initialization timed out"));
        }
    };

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

    eprintln!("Shutting down client...");
    if let Err(e) = client.shutdown().await {
        eprintln!("Error during shutdown: {:?}", e);
    }

    eprintln!("Client terminated");
    Ok(())
}

