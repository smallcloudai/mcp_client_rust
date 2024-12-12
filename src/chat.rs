use crate::mcp_client_manager::MCPClientManager;
use crate::tool_def::execute_function_call;
use anyhow::Result;
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionResponseMessage, ChatCompletionTool,
        ChatCompletionToolChoiceOption, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionObject,
    },
    Client as OpenAIClient,
};
use colored::*;
use serde_json::Value;
use std::sync::Arc;

pub struct ChatState {
    pub messages: Vec<ChatCompletionRequestMessage>,
    pub verbose: bool,
}

impl ChatState {
    pub fn new(verbose: bool) -> Self {
        Self {
            messages: vec![],
            verbose,
        }
    }

    pub fn print_state(&self) {
        if !self.verbose {
            return;
        }

        println!("\n{}", "=".repeat(50).bright_black());
        println!("{}", "Current Chat State:".bright_blue().bold());

        for msg in &self.messages {
            match msg {
                ChatCompletionRequestMessage::System(m) => {
                    println!("{}", "system:".bright_magenta().bold());
                    println!("  {:#?}", m.content);
                }
                ChatCompletionRequestMessage::User(m) => {
                    println!("{}", "user:".bright_green().bold());
                    println!("  {:#?}", m.content);
                }
                ChatCompletionRequestMessage::Assistant(m) => {
                    println!("{}", "assistant:".bright_cyan().bold());
                    println!("  {:#?}", m.content);
                }
                ChatCompletionRequestMessage::Tool(m) => {
                    println!("{}", "tool:".bright_yellow().bold());
                    println!("  {:#?}", m.content);
                }
                ChatCompletionRequestMessage::Function(m) => {
                    // error
                    panic!("Function messages should not be added to the chat state");
                }
            }
        }

        println!("{}\n", "=".repeat(50).bright_black());
    }

    pub fn add_system_message(&mut self, content: &str) {
        let msg = ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessageArgs::default()
                .content(content)
                .build()
                .unwrap(),
        );
        self.messages.push(msg);
        self.print_state();
    }

    pub fn add_user_message(&mut self, content: &str) {
        let msg = ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessageArgs::default()
                .content(content)
                .build()
                .unwrap(),
        );
        self.messages.push(msg);
        self.print_state();
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        let msg = ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(content)
                .build()
                .unwrap(),
        );
        self.messages.push(msg);
        self.print_state();
    }

    /// Add a tool response message that corresponds to a previous assistant `tool_call_id`.
    pub fn add_tool_message(&mut self, content: &str, tool_call_id: &str) {
        let msg = ChatCompletionRequestMessage::Tool(
            ChatCompletionRequestToolMessageArgs::default()
                .content(content)
                .tool_call_id(tool_call_id)
                .build()
                .unwrap(),
        );
        self.messages.push(msg);
        self.print_state();
    }

    pub fn to_request_messages(&self) -> Vec<ChatCompletionRequestMessage> {
        self.messages.clone()
    }

    /// Add the assistant message that indicates tool calls directly from the response.
    /// This uses the `ChatCompletionResponseMessage` returned by the OpenAI API.
    pub fn add_assistant_message_from_response(&mut self, resp: &ChatCompletionResponseMessage) {
        // The assistant message might have content or might be empty.
        // We'll include it even if empty, as required.
        let content = resp.content.as_deref().unwrap_or("");
        let msg = ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessageArgs::default()
                .content(content)
                .tool_calls(
                    resp.tool_calls
                        .as_ref()
                        .map(|tool_calls| tool_calls.to_vec())
                        .unwrap_or_default(),)
                .build()
                .unwrap(),
        );
        self.messages.push(msg);
        self.print_state();
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


// TODO: Add a response type enum to determine if tool call is available # AI! 
enum ResponseType {
    AssistantMessage {
        
    }
    AssistantMessageWithToolCalls
}



/// This function sends the messages to OpenAI and if a tool call is requested,
/// executes it and then repeats until a final assistant message is obtained.
pub async fn send_and_handle_function_calls(
    openai_client: &OpenAIClient<OpenAIConfig>,
    chat_state: &mut ChatState,
    mcp_manager: &Arc<MCPClientManager>,
    model: &str,
) -> Result<()> {
    loop {
        let messages = chat_state.to_request_messages();

        // Get available tools as functions
        let available_tools = mcp_manager.get_available_tools().await?;
        let functions: Vec<ChatCompletionTool> = available_tools
            .iter()
            .map(|tool| ChatCompletionTool {
                function: FunctionObject {
                    name: tool.name.clone(),
                    description: Some(tool.description.clone()),
                    parameters: Some(tool.parameters.clone()),
                    strict: Some(false),
                },
                r#type: ChatCompletionToolType::Function,
            })
            .collect();

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
        
        // determine if tool call available
        
        
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No completion choice returned"))?;

        // Check for tool calls
        if let Some(ref tool_calls) = choice.message.tool_calls {
            // The assistant decided to call a tool.
            // First, add the assistant message from this response (even if empty content).
            chat_state.add_assistant_message_from_response(&choice.message);

            // Execute each tool call and then add the tool message
            for tool_call in tool_calls {
                let fname = tool_call.function.name.clone();
                let arguments = tool_call.function.arguments.clone();
                let tool_call_id = tool_call.id.clone();

                let arguments: Value = serde_json::from_str(&arguments)?;
                let result_value = execute_function_call(&fname, &arguments, mcp_manager).await?;
                let result_str = serde_json::to_string(&result_value)?;

                chat_state.add_tool_message(&result_str, &tool_call_id);
            }
            // After adding tool messages, continue loop to let assistant process them
            continue;
        } else {
            // No tool calls, so this should be a final assistant message
            // Just add the assistant message from the response
            chat_state.add_assistant_message_from_response(&choice.message);
            break;
        }
    }

    Ok(())
}
