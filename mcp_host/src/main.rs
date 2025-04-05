use anyhow::Result;

mod ai_client;
mod anthropic;
mod deepseek;
mod gemini;
mod openai;
mod conversation_service;
mod conversation_state;
mod repl;
mod main_repl;
mod host;

#[tokio::main]
async fn main() -> Result<()> {
    // Simply forward to the REPL implementation
    main_repl::main().await
}
