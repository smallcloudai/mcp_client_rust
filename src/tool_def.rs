use crate::mcp_client_manager::MCPClientManager;
use anyhow::Result;
use serde_json::Value;

pub async fn execute_function_call(
    function_name: &str,
    arguments: &Value,
    mcp_manager: &MCPClientManager,
) -> Result<String> {
    let result = mcp_manager
        .call_tool(function_name, arguments.clone())
        .await?;
    Ok(serde_json::to_string(&result)?)
}
