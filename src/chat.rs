use crate::{FunctionCall, MCPClientManager};
use anyhow::Result;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionObject,
    },
    Client as OpenAIClient,
};
use serde_json::from_str;
use std::sync::Arc;

pub struct ChatState {
    pub messages: Vec<(String, String)>, // (role, content)
}

impl ChatState {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.messages
            .push(("system".to_string(), content.to_string()));
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages
            .push(("user".to_string(), content.to_string()));
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages
            .push(("assistant".to_string(), content.to_string()));
    }

    pub fn to_request_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.messages
            .iter()
            .map(|(role, content)| match role.as_str() {
                "system" => ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(content.as_str())
                        .build()
                        .unwrap(),
                ),
                "user" => ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(content.as_str())
                        .build()
                        .unwrap(),
                ),
                "assistant" => ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessageArgs::default()
                        .content(content.as_str())
                        .build()
                        .unwrap(),
                ),
                _ => panic!("Unknown role"),
            })
            .collect()
    }
}

pub async fn handle_user_input(
    openai_client: &OpenAIClient<OpenAIConfig>,
    chat_state: &mut ChatState,
    mcp_manager: &Arc<MCPClientManager>,
    user_input: &str,
    model: &str,
) -> Result<()> {
    chat_state.add_user_message(user_input);

    let messages = chat_state.to_request_messages();

    // Get available tools from MCP server
    let tools: Vec<ChatCompletionTool> = mcp_manager
        .get_available_tools()
        .await?
        .into_iter()
        .map(|tool| ChatCompletionTool {
            r#type: ChatCompletionToolType::Function,
            function: FunctionObject {
                name: tool.name,
                description: Some(tool.description),
                parameters: Some(tool.parameters),
                strict: Some(true),
            },
        })
        .collect();

    let request = CreateChatCompletionRequestArgs::default()
        .model(model)
        .messages(messages)
        .tools(tools)
        .tool_choice("auto")
        .build()?;

    let response = openai_client.chat().create(request).await?;
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No completion choice returned"))?;

    if let Some(tool_calls) = choice.message.tool_calls {
        for tool_call in tool_calls {
            let function_call = FunctionCall {
                name: tool_call.function.name,
                arguments: from_str(&tool_call.function.arguments)?,
            };

            match function_call.execute(mcp_manager).await {
                Ok(result) => chat_state.add_assistant_message(&result),
                Err(e) => chat_state.add_assistant_message(&format!("Error: {}", e)),
            }
        }
    } else if let Some(content) = choice.message.content {
        chat_state.add_assistant_message(&content);
    }

    Ok(())
}
