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

#[cfg(test)]
mod test;

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
    /// Receive `InitializeResult`, then send `notifications/initialized`.
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

    /// Send a request to the server and wait for the response (30s timeout).
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

        // Wait for matching response
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

    /// List available tools
    pub async fn list_tools(&self) -> Result<ListToolsResult, Error> {
        tracing::debug!("Listing available tools");
        let response = self.request("tools/list", None).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received tools list");
        result
    }

    /// Call a tool with the given name and arguments. If `isError` is `true`, treat it as an error.
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

        let tool_result: CallToolResult = serde_json::from_value(response)?;
        if tool_result.is_error {
            // We treat tool-level errors (isError=true) as a Rust error
            let maybe_msg = tool_result
                .content
                .iter()
                .filter_map(|msg| {
                    if let crate::types::MessageContent::Text { text } = msg {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            return Err(Error::Other(format!(
                "Tool '{}' execution failed: {}",
                name, maybe_msg
            )));
        }
        tracing::debug!(?tool_result, "Tool call succeeded");
        Ok(tool_result)
    }

    /// Get a specific tool by name from the available tools
    pub async fn get_tool(&self, name: &str) -> Result<Option<Tool>, Error> {
        tracing::debug!(%name, "Getting specific tool");
        let tools = self.list_tools().await?;
        let tool = tools.tools.into_iter().find(|t| t.name == name);
        tracing::debug!(?tool, "Found tool");
        Ok(tool)
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

    /// List available resources
    pub async fn list_resources(&self) -> Result<ListResourcesResult, Error> {
        tracing::debug!("Listing available resources");
        let response = self.request("resources/list", None).await?;
        let result = serde_json::from_value(response).map_err(Error::from);
        tracing::debug!(?result, "Received resources list");
        result
    }
}
