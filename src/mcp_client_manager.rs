use crate::config::MCPServerConfig;
use anyhow::Result;
use mcp_rust_sdk::client::{Client, ClientBuilder};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

pub struct MCPClientManager {
    clients: HashMap<String, Arc<Client>>,
    tool_mapping: HashMap<String, (String, String)>,
}

impl MCPClientManager {
    pub async fn new(configs: &HashMap<String, MCPServerConfig>) -> Result<Self> {
        let mut clients = HashMap::new();
        let mut tool_mapping = HashMap::new();

        for (name, server_conf) in configs {
            let mut builder = ClientBuilder::new(&server_conf.command);
            for arg in &server_conf.args {
                builder = builder.arg(arg);
            }

            // Add environment variables if specified
            for (key, value) in &server_conf.env {
                builder = builder.env(key, value);
            }

            let client = builder.spawn_and_initialize().await?;
            let client = Arc::new(client);

            // Get tools from this server
            let tools_val = client.request("tools/list", None).await?;
            if let Some(tools_arr) = tools_val.get("tools").and_then(|v| v.as_array()) {
                for t in tools_arr {
                    if let Some(name) = t.get("name").and_then(|x| x.as_str()) {
                        tool_mapping.insert(name.to_string(), (name.to_string(), name.to_string()));
                    }
                }
            }

            clients.insert(name.clone(), client);
        }

        Ok(Self {
            clients,
            tool_mapping,
        })
    }

    pub async fn call_tool(&self, tool_name: &str, arguments: Value) -> Result<Value> {
        let (server_name, tool_id) = self
            .tool_mapping
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("Tool '{}' not found or not registered", tool_name))?;

        let client = self.clients.get(server_name).ok_or_else(|| {
            anyhow::anyhow!("Server '{}' not found for tool {}", server_name, tool_name)
        })?;

        client
            .call_tool(tool_id, arguments)
            .await
            .map_err(|e| anyhow::anyhow!("Tool call failed: {}", e))
    }

    pub async fn get_available_tools(&self) -> Result<Vec<ToolDescription>> {
        // Get tools from the first server for simplicity
        if let Some((_, client)) = self.clients.iter().next() {
            let tools_val = client.request("tools/list", None).await?;

            if let Some(tools_arr) = tools_val.get("tools").and_then(|v| v.as_array()) {
                let mut tools = Vec::new();

                for tool in tools_arr {
                    if let (Some(name), Some(description), Some(parameters)) = (
                        tool.get("name").and_then(|x| x.as_str()),
                        tool.get("description").and_then(|x| x.as_str()),
                        tool.get("parameters"),
                    ) {
                        tools.push(ToolDescription {
                            name: name.to_string(),
                            description: description.to_string(),
                            parameters: parameters.clone(),
                        });
                    }
                }

                Ok(tools)
            } else {
                Err(anyhow::anyhow!("No tools found or invalid tools format"))
            }
        } else {
            Err(anyhow::anyhow!("No MCP servers configured"))
        }
    }
}
