use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::{
    error::{Error, ErrorCode},
    protocol::{Notification, Request, RequestId},
    transport::{Message, Transport},
    types::{
        CallToolRequest, CallToolResult, ClientCapabilities, CompleteRequest, CompleteResult,
        GetPromptResult, Implementation, InitializeResult, ListPromptsResult, ListResourcesResult,
        ListToolsResult, ServerCapabilities, Tool,
    },
    ReadResourceResult,
};

mod builder;
pub use builder::ClientBuilder;

/// MCP client state
pub struct Client {
    transport: Arc<dyn Transport>,
    server_capabilities: Arc<RwLock<Option<ServerCapabilities>>>,
    request_counter: Arc<RwLock<i64>>,
    response_receiver: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<Message>>>,
    response_sender: tokio::sync::mpsc::UnboundedSender<Message>,
}

impl Client {
    /// Create a new MCP client with the given transport
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let client = Self {
            transport: transport.clone(),
            server_capabilities: Arc::new(RwLock::new(None)),
            request_counter: Arc::new(RwLock::new(0)),
            response_receiver: Arc::new(Mutex::new(rx)),
            response_sender: tx.clone(),
        };

        // Start response handler task
        let transport_clone = transport.clone();
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut stream = transport_clone.receive();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(message) => {
                        if tx_clone.send(message).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        client
    }

    /// Initialize the client according to MCP spec:
    /// Send an "initialize" request with `clientInfo`, `capabilities`, and `protocolVersion`.
    /// Receive `InitializeResult` (including `protocolVersion`, `serverInfo`, `capabilities`).
    /// Then send `notifications/initialized`.
    pub async fn initialize(
        &self,
        implementation: Implementation,
        capabilities: ClientCapabilities,
    ) -> Result<InitializeResult, Error> {
        let params = serde_json::json!({
            "clientInfo": implementation,
            "capabilities": capabilities,
            "protocolVersion": crate::LATEST_PROTOCOL_VERSION,
        });

        let response = self.request("initialize", Some(params)).await?;
        let init_result: InitializeResult = serde_json::from_value(response)?;

        // Store the server capabilities
        *self.server_capabilities.write().await = Some(init_result.capabilities.clone());

        // After initialization completes, send the initialized notification
        // The spec says to send "notifications/initialized"
        self.notify("notifications/initialized", None).await?;

        Ok(init_result)
    }

    /// Send a request to the server and wait for the response.
    ///
    /// This method will block until a response is received from the server.
    /// If the server returns an error, it will be propagated as an `Error`.
    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Error> {
        let mut counter = self.request_counter.write().await;
        *counter += 1;
        let id = RequestId::Number(*counter);

        let request = Request::new(method, params, id.clone());
        self.transport.send(Message::Request(request)).await?;

        // Wait for matching response
        let mut receiver = self.response_receiver.lock().await;
        while let Some(message) = receiver.recv().await {
            if let Message::Response(response) = message {
                if response.id == id {
                    if let Some(error) = response.error {
                        return Err(Error::protocol(
                            match error.code {
                                -32700 => ErrorCode::ParseError,
                                -32600 => ErrorCode::InvalidRequest,
                                -32601 => ErrorCode::MethodNotFound,
                                -32602 => ErrorCode::InvalidParams,
                                -32603 => ErrorCode::InternalError,
                                -32002 => ErrorCode::ServerNotInitialized,
                                -32001 => ErrorCode::UnknownErrorCode,
                                -32000 => ErrorCode::RequestFailed,
                                _ => ErrorCode::UnknownErrorCode,
                            },
                            &error.message,
                        ));
                    }
                    return response.result.ok_or_else(|| {
                        Error::protocol(ErrorCode::InternalError, "Response missing result")
                    });
                }
            }
        }

        Err(Error::protocol(
            ErrorCode::InternalError,
            "Connection closed while waiting for response",
        ))
    }

    /// Send a notification to the server
    pub async fn notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), Error> {
        let notification = Notification::new(method, params);
        self.transport
            .send(Message::Notification(notification))
            .await
    }

    /// Get the server capabilities
    pub async fn capabilities(&self) -> Option<ServerCapabilities> {
        self.server_capabilities.read().await.clone()
    }

    /// Close the client connection
    pub async fn shutdown(&self) -> Result<(), Error> {
        // Close transport
        self.transport.close().await
    }

    /// List available resources
    pub async fn list_resources(&self) -> Result<ListResourcesResult, Error> {
        let response = self.request("resources/list", None).await?;
        serde_json::from_value(response).map_err(Error::from)
    }

    /// List available prompts
    pub async fn list_prompts(&self) -> Result<ListPromptsResult, Error> {
        let response = self.request("prompts/list", None).await?;
        serde_json::from_value(response).map_err(Error::from)
    }

    /// Read a resource by URI
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, Error> {
        let params = serde_json::json!({ "uri": uri });
        let response = self.request("resources/read", Some(params)).await?;
        serde_json::from_value(response).map_err(Error::from)
    }

    // TODO: don't use this for now, shit's buggy

    /// Get a prompt by ID
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<std::collections::HashMap<String, String>>,
    ) -> Result<GetPromptResult, Error> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or_default() // empty dict if None
        });
        let response = self.request("prompts/get", Some(params)).await?;
        let result: GetPromptResult = serde_json::from_value(response)?;
        Ok(result)
    }

    /// Complete a prompt
    pub async fn complete(&self, request: CompleteRequest) -> Result<CompleteResult, Error> {
        let response = self
            .request("prompts/complete", Some(serde_json::to_value(request)?))
            .await?;
        serde_json::from_value(response).map_err(Error::from)
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<ListToolsResult, Error> {
        let response = self.request("tools/list", None).await?;
        serde_json::from_value(response).map_err(Error::from)
    }

    /// Call a tool with the given name and arguments
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, Error> {
        let request = CallToolRequest {
            name: name.to_string(),
            arguments,
        };

        let response = self
            .request("tools/call", Some(serde_json::to_value(request)?))
            .await?;

        serde_json::from_value(response).map_err(Error::from)
    }

    /// Get a specific tool by name from the available tools
    pub async fn get_tool(&self, name: &str) -> Result<Option<Tool>, Error> {
        let tools = self.list_tools().await?;
        Ok(tools.tools.into_iter().find(|t| t.name == name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::Stream;
    use std::{pin::Pin, time::Duration};
    use tokio::sync::broadcast;

    struct MockTransport {
        tx: broadcast::Sender<Result<Message, Error>>,
        send_delay: Duration,
    }

    impl MockTransport {
        fn new(send_delay: Duration) -> (Self, broadcast::Sender<Result<Message, Error>>) {
            let (tx, _) = broadcast::channel(10);
            let tx_clone = tx.clone();
            (Self { tx, send_delay }, tx_clone)
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&self, message: Message) -> Result<(), Error> {
            tokio::time::sleep(self.send_delay).await;
            self.tx.send(Ok(message)).map(|_| ()).map_err(|_| {
                Error::protocol(
                    crate::error::ErrorCode::InternalError,
                    "Failed to send message",
                )
            })
        }

        fn receive(&self) -> Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>> {
            let mut rx = self.tx.subscribe();
            Box::pin(async_stream::stream! {
                while let Ok(msg) = rx.recv().await {
                    yield msg;
                }
            })
        }

        async fn close(&self) -> Result<(), Error> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_client_initialization_timeout() {
        // Create a mock transport with 6 second delay (longer than our timeout)
        let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
        let client = Client::new(Arc::new(transport));

        // Try to initialize with 5 second timeout
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            client.initialize(
                Implementation {
                    name: "test".to_string(),
                    version: "1.0".to_string(),
                },
                ClientCapabilities::default(),
            ),
        )
        .await;

        // Should timeout
        assert!(result.is_err(), "Expected timeout error");
    }

    #[tokio::test]
    async fn test_client_request_timeout() {
        // Create a mock transport with 6 second delay
        let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
        let client = Client::new(Arc::new(transport));

        // Try to send request with 5 second timeout
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            client.request("test", Some(serde_json::json!({"key": "value"}))),
        )
        .await;

        // Should timeout
        assert!(result.is_err(), "Expected timeout error");
    }

    #[tokio::test]
    async fn test_client_notification_timeout() {
        // Create a mock transport with 6 second delay
        let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
        let client = Client::new(Arc::new(transport));

        // Try to send notification with 5 second timeout
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            client.notify("test", Some(serde_json::json!({"key": "value"}))),
        )
        .await;

        // Should timeout
        assert!(result.is_err(), "Expected timeout error");
    }

    #[tokio::test]
    async fn test_client_fast_operation() {
        // Create a mock transport with 1 second delay (shorter than timeout)
        let (transport, _tx) = MockTransport::new(Duration::from_secs(1));
        let client = Client::new(Arc::new(transport));

        // Try to send notification with 5 second timeout
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            client.notify("test", Some(serde_json::json!({"key": "value"}))),
        )
        .await;

        // Should complete before timeout
        assert!(result.is_ok(), "Operation should complete before timeout");
    }
}
