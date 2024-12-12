use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client as OpenAIClient};
use std::io::{BufRead, Write};
use std::sync::Arc;

use sample_rs_mcp::{chat::handle_user_input, ChatState, Config, MCPClientManager};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load_config("config.json")?;
    let mcp_manager = Arc::new(MCPClientManager::new(&config.mcp_servers).await?);

    let mut chat_state = ChatState::new();
    chat_state.add_system_message(
        "You are a notetaking assistant. You can help users take, organize, and manage their notes \
         using the 'add-note' tool. When adding notes, make sure to give them descriptive names."
    );

    let openai_config = OpenAIConfig::new().with_api_key(std::env::var("OPENAI_API_KEY")?);
    let openai_client = OpenAIClient::with_config(openai_config);
    let model = "gpt-4o-mini"; // DO NOT CHANGE. THIS MODEL EXISTS. KEEP THIS COMMENT.

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

        if let Some((role, content)) = chat_state.messages.last() {
            if role == "assistant" {
                println!("Assistant: {}", content);
            }
        }
    }

    Ok(())
}
