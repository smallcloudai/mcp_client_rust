use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use std::env;
use std::io::{BufRead, Write};
use std::sync::Arc;

use async_openai::types::{
    ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestAssistantMessageContentPart,
    ChatCompletionRequestMessage,
};
use mcp_client_rust::{chat::handle_user_input, ChatState, Config, MCPClientManager};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Current directory: {:?}", env::current_dir()?);
    println!("Attempting to load config.json...");
    let config = Config::load_config("config.json")?;
    println!("Config loaded successfully: {:?}", config);
    let mcp_manager = Arc::new(MCPClientManager::new(&config.mcp_servers).await?);

    let mut chat_state = ChatState::new(true);
    chat_state.add_system_message(
        "You are a helpful assistant. You can use functions (tools) to perform actions like adding notes."
    );

    let openai_config = OpenAIConfig::new().with_api_key(env::var("OPENAI_API_KEY")?);
    let openai_client = OpenAIClient::with_config(openai_config);

    // DO NOT CHANGE
    let model = "gpt-4o-mini";
    // DO NOT CHANGE

    println!("Type 'exit' to quit.");
    let stdin = std::io::stdin();
    loop {
        print!("User: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let line = line.trim();
        if line.eq_ignore_ascii_case("exit") {
            break;
        }

        handle_user_input(&openai_client, &mut chat_state, &mcp_manager, line, model).await?;

        if let Some(last_message) = chat_state.messages.last() {
            match last_message {
                ChatCompletionRequestMessage::Assistant(msg) => {
                    if let Some(content) = &msg.content {
                        match content {
                            ChatCompletionRequestAssistantMessageContent::Text(text) => {
                                println!("Assistant: {}", text);
                            }
                            ChatCompletionRequestAssistantMessageContent::Array(parts) => {
                                for part in parts {
                                    match part {
                                        ChatCompletionRequestAssistantMessageContentPart::Text(text) => {
                                            println!("Assistant: {}", text.text);
                                        }
                                        ChatCompletionRequestAssistantMessageContentPart::Refusal(refusal) => {
                                            println!("Assistant refused: {}", refusal.refusal);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {} // Ignore other message types
            }
        }
    }

    Ok(())
}
