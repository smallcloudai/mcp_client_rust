use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{timeout, Duration};

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
            tracing::debug!("Starting response handler task");
            let mut stream = transport_clone.receive();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(message) => {
                        tracing::trace!(?message, "Received message from transport");
                        if tx_clone.send(message).is_err() {
                            tracing::error!("Failed to forward message - channel closed");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!(?e, "Error receiving message from transport");
                        break;
                    }
                }
            }
            tracing::debug!("Response handler task terminated");
        });

        tracing::debug!("Created new MCP client");
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
        tracing::info!(?implementation, "Initializing MCP client");

        let params = serde_json::json!({
            "clientInfo": implementation,
            "capabilities": capabilities,
            "protocolVersion": crate::LATEST_PROTOCOL_VERSION,
        });

        let response = self.request("initialize", Some(params)).await?;
        let init_result: InitializeResult = serde_json::from_value(response)?;

        tracing::debug!(?init_result, "Received initialization response");

        // Store the server capabilities
        *self.server_capabilities.write().await = Some(init_result.capabilities.clone());

        // After initialization completes, send the initialized notification
        tracing::debug!("Sending initialized notification");
        self.notify("notifications/initialized", None).await?;

        tracing::info!("MCP client initialization complete");
        Ok(init_result)
    }

    /// Send a request to the server and wait for the response.
    ///
    /// This method will block until a response is received from the server or timeout occurs after 30 seconds.
    /// If the server returns an error, it will be propagated as an `Error`.
    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Error> {
        // increment request ID
        let mut counter = self.request_counter.write().await;
        *counter += 1;
        let id = RequestId::Number(*counter);

        let request = Request::new(method, params, id.clone());
        tracing::debug!(?request, "Sending MCP request");

        // Send request
        self.transport.send(Message::Request(request)).await?;

        // Wait for matching response with timeout
        let mut rx = self.response_receiver.lock().await;

        match tokio::time::timeout(std::time::Duration::from_secs(30), async {
            while let Some(message) = rx.recv().await {
                match message {
                    Message::Response(response) if response.id == id => {
                        tracing::debug!(?response, "Received matching MCP response");
                        if let Some(error) = response.error {
                            tracing::error!(?error, "Server returned error");
                            return Err(Error::Protocol {
                                code: error.code.into(),
                                message: error.message,
                                data: error.data,
                            });
                        }
                        return response.result.ok_or_else(|| {
                            Error::protocol(ErrorCode::InternalError, "Response missing result")
                        });
                    }
                    Message::Response(response) => {
                        tracing::debug!(
                            ?response,
                            "Received non-matching response, continuing to wait"
                        );
                    }
                    Message::Notification(notif) => {
                        tracing::debug!(?notif, "Received notification while waiting for response");
                    }
                    Message::Request(req) => {
                        tracing::debug!(?req, "Received request while waiting for response");
                    }
                }
            }

            // Channel closed
            Err(Error::protocol(
                ErrorCode::InternalError,
                "Connection closed while waiting for response",
            ))
        })
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::error!("Request to '{}' timed out after 30 seconds", method);
                Err(Error::Other(format!(
                    "Request to '{method}' timed out after 30 seconds"
                )))
            }
        }
    }

    /// Send a notification to the server
    pub async fn notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), Error> {
        let notification = Notification::new(method, params.clone());
        tracing::debug!(?method, ?params, "Sending MCP notification");
        self.transport
            .send(Message::Notification(notification))
            .await
    }

    /// Get the server capabilities
    pub async fn capabilities(&self) -> Option<ServerCapabilities> {
        let caps = self.server_capabilities.read().await.clone();
        tracing::trace!(?caps, "Retrieved server capabilities");
        caps
    }

    /// Close the client connection
    pub async fn shutdown(&self) -> Result<(), Error> {
        tracing::info!("Shutting down MCP client");
        // Close transport
        self.transport.close().await
    }

    /// List available resources
    pub async fn list_resources(&self) -> Result<ListResourcesResult, Error> {
        tracing::debug!("Listing available resources");
        let response = self.request("resources/list", None).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received resources list");
        result
    }

    /// List available prompts
    pub async fn list_prompts(&self) -> Result<ListPromptsResult, Error> {
        tracing::debug!("Listing available prompts");
        let response = self.request("prompts/list", None).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received prompts list");
        result
    }

    /// Read a resource by URI
    pub async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, Error> {
        tracing::debug!(%uri, "Reading resource");
        let params = serde_json::json!({ "uri": uri });
        let response = self.request("resources/read", Some(params)).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received resource content");
        result
    }

    /// Get a prompt by ID
    pub async fn get_prompt(
        &self,
        name: &str,
        arguments: Option<std::collections::HashMap<String, String>>,
    ) -> Result<GetPromptResult, Error> {
        tracing::debug!(%name, ?arguments, "Getting prompt");
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments.unwrap_or_default()
        });
        let response = self.request("prompts/get", Some(params)).await?;
        let result = serde_json::from_value(response)?;
        tracing::debug!(?result, "Received prompt");
        Ok(result)
    }

    /// Complete a prompt
    pub async fn complete(&self, request: CompleteRequest) -> Result<CompleteResult, Error> {
        tracing::debug!(?request, "Completing prompt");
        let response = self
            .request("prompts/complete", Some(serde_json::to_value(request)?))
            .await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received completion");
        result
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<ListToolsResult, Error> {
        tracing::debug!("Listing available tools");
        let response = self.request("tools/list", None).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received tools list");
        result
    }

    /// Call a tool with the given name and arguments
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, Error> {
        tracing::debug!(%name, ?arguments, "Calling tool");
        let request = CallToolRequest {
            name: name.to_string(),
            arguments,
        };

        let response = self
            .request("tools/call", Some(serde_json::to_value(request)?))
            .await?;

        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received tool result");
        result
    }

    /// Get a specific tool by name from the available tools
    pub async fn get_tool(&self, name: &str) -> Result<Option<Tool>, Error> {
        tracing::debug!(%name, "Getting specific tool");
        let tools = self.list_tools().await?;
        let tool = tools.tools.into_iter().find(|t| t.name == name);
        tracing::debug!(?tool, "Found tool");
        Ok(tool)
    }
}
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use async_trait::async_trait;
//     use futures::Stream;
//     use std::{pin::Pin, time::Duration};
//     use tokio::sync::broadcast;
//
//     struct MockTransport {
//         tx: broadcast::Sender<Result<Message, Error>>,
//         send_delay: Duration,
//     }
//
//     impl MockTransport {
//         fn new(send_delay: Duration) -> (Self, broadcast::Sender<Result<Message, Error>>) {
//             let (tx, _) = broadcast::channel(10);
//             let tx_clone = tx.clone();
//             (Self { tx, send_delay }, tx_clone)
//         }
//     }
//
//     #[async_trait]
//     impl Transport for MockTransport {
//         async fn send(&self, message: Message) -> Result<(), Error> {
//             tokio::time::sleep(self.send_delay).await;
//             self.tx.send(Ok(message)).map(|_| ()).map_err(|_| {
//                 Error::protocol(
//                     crate::error::ErrorCode::InternalError,
//                     "Failed to send message",
//                 )
//             })
//         }
//
//         fn receive(&self) -> Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>> {
//             let mut rx = self.tx.subscribe();
//             Box::pin(async_stream::stream! {
//                 while let Ok(msg) = rx.recv().await {
//                     yield msg;
//                 }
//             })
//         }
//
//         async fn close(&self) -> Result<(), Error> {
//             Ok(())
//         }
//     }
//
//     #[tokio::test]
//     async fn test_client_initialization_timeout() {
//         // Create a mock transport with 6 second delay (longer than our timeout)
//         let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
//         let client = Client::new(Arc::new(transport));
//
//         // Try to initialize with 5 second timeout
//         let result = tokio::time::timeout(
//             Duration::from_secs(5),
//             client.initialize(
//                 Implementation {
//                     name: "test".to_string(),
//                     version: "1.0".to_string(),
//                 },
//                 ClientCapabilities::default(),
//             ),
//         )
//         .await;
//
//         // Should timeout
//         assert!(result.is_err(), "Expected timeout error");
//     }
//
//     #[tokio::test]
//     async fn test_client_request_timeout() {
//         // Create a mock transport with 6 second delay
//         let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
//         let client = Client::new(Arc::new(transport));
//
//         // Try to send request with 5 second timeout
//         let result = tokio::time::timeout(
//             Duration::from_secs(5),
//             client.request("test", Some(serde_json::json!({"key": "value"}))),
//         )
//         .await;
//
//         // Should timeout
//         assert!(result.is_err(), "Expected timeout error");
//     }
//
//     #[tokio::test]
//     async fn test_client_notification_timeout() {
//         // Create a mock transport with 6 second delay
//         let (transport, _tx) = MockTransport::new(Duration::from_secs(6));
//         let client = Client::new(Arc::new(transport));
//
//         // Try to send notification with 5 second timeout
//         let result = tokio::time::timeout(
//             Duration::from_secs(5),
//             client.notify("test", Some(serde_json::json!({"key": "value"}))),
//         )
//         .await;
//
//         // Should timeout
//         assert!(result.is_err(), "Expected timeout error");
//     }
//
//     #[tokio::test]
//     async fn test_client_fast_operation() {
//         // Create a mock transport with 1 second delay (shorter than timeout)
//         let (transport, _tx) = MockTransport::new(Duration::from_secs(1));
//         let client = Client::new(Arc::new(transport));
//
//         // Try to send notification with 5 second timeout
//         let result = tokio::time::timeout(
//             Duration::from_secs(5),
//             client.notify("test", Some(serde_json::json!({"key": "value"}))),
//         )
//         .await;
//
//         // Should complete before timeout
//         assert!(result.is_ok(), "Operation should complete before timeout");
//     }
// }
