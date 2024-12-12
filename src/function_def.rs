use crate::mcp_client_manager::MCPClientManager;
use anyhow::Result;
use serde_json::Value;

// Simple struct to parse function calls from the LLM
pub struct FunctionCall {
    pub name: String,
    pub arguments: Value,
}

impl FunctionCall {
    pub async fn execute(&self, mcp_manager: &MCPClientManager) -> Result<String> {
        // Direct passthrough to MCP server
        let result = mcp_manager
            .call_tool(&self.name, self.arguments.clone())
            .await?;
        Ok(result.to_string())
    }
}
