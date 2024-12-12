use crate::function_def::execute_function_call;
use crate::mcp_client_manager::MCPClientManager;
use anyhow::Result;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestFunctionMessageArgs,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionTool, ChatCompletionToolChoiceOption,
        ChatCompletionToolType, CreateChatCompletionRequestArgs, FunctionObject,
    },
    Client as OpenAIClient,
};
use serde_json::Value;
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

    /// Add a function response message:
    /// According to OpenAI spec, after a function call, you add a message:
    /// {"role":"function", "name":"function_name", "content":"result_from_function"}
    pub fn add_function_message(&mut self, function_name: &str, content: &str) {
        self.messages.push((
            "function".to_string(),
            format!("{}|{}", function_name, content),
        ));
    }

    pub fn to_request_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.messages
            .iter()
            .map(|(role, content)| {
                match role.as_str() {
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
                    "function" => {
                        // Content is stored as "function_name|result"
                        let mut split = content.splitn(2, '|');
                        let fname = split.next().unwrap();
                        let fcontent = split.next().unwrap_or("");
                        ChatCompletionRequestMessage::Function(
                            ChatCompletionRequestFunctionMessageArgs::default()
                                .name(fname)
                                .content(fcontent)
                                .build()
                                .unwrap(),
                        )
                    }
                    _ => panic!("Unknown role"),
                }
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

    send_and_handle_function_calls(openai_client, chat_state, mcp_manager, model).await?;
    Ok(())
}

/// This function sends the messages to OpenAI and if a function call is requested,
/// executes it and then re-requests until a final assistant message is obtained.
async fn send_and_handle_function_calls(
    openai_client: &OpenAIClient<OpenAIConfig>,
    chat_state: &mut ChatState,
    mcp_manager: &Arc<MCPClientManager>,
    model: &str,
) -> Result<()> {
    // Prepare request messages
    let messages = chat_state.to_request_messages();

    // Get available tools as functions
    let available_tools = mcp_manager.get_available_tools().await?;
    let functions: Vec<ChatCompletionTool> = available_tools
        .into_iter()
        .map(|tool| ChatCompletionTool {
            function: FunctionObject {
                name: tool.name,
                description: Some(tool.description),
                parameters: Some(tool.parameters),
                strict: Some(false),
            },
            r#type: ChatCompletionToolType::Function,
        })
        .collect();

    // Build the request:
    let request = if functions.is_empty() {
        CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .build()?
    } else {
        CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .tools(functions)
            .tool_choice(ChatCompletionToolChoiceOption::Auto)
            .build()?
    };

    let response = openai_client.chat().create(request).await?;
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No completion choice returned"))?;

    // Check if the assistant decided to call a tool
    if let Some(tool_calls) = choice.message.tool_calls {
        for tool_call in tool_calls {
            if tool_call.r#type == ChatCompletionToolType::Function {
                // The assistant wants to call a function
                let fname = tool_call.function.name.clone();
                let arguments: Value = serde_json::from_str(&tool_call.function.arguments)?;

                // Execute the function via MCP
                match execute_function_call(&fname, &arguments, mcp_manager).await {
                    Ok(result_str) => {
                        // Add a function message with the result
                        chat_state.add_function_message(&fname, &result_str);
                    }
                    Err(e) => {
                        chat_state.add_assistant_message(&format!("Function call failed: {}", e));
                        return Ok(());
                    }
                }
            }
        }
        // Now call the model again to get a final assistant response, but use Box::pin
        Box::pin(send_and_handle_function_calls(
            openai_client,
            chat_state,
            mcp_manager,
            model,
        ))
        .await?;
    } else {
        // No tool calls, just an assistant message
        if let Some(content) = choice.message.content.as_deref() {
            chat_state.add_assistant_message(content);
        }
    }

    Ok(())
}
