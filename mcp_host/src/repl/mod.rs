// Enhanced MCP Host REPL Implementation 
// Merges REPL simplicity with CLI prompt enhancements
mod connections;
mod command;
mod helper;
#[cfg(test)]
mod test_utils;

pub use command::CommandProcessor;
pub use helper::ReplHelper;
pub use connections::ServerConnections;

// Import ReplClient and adapter from shared library
pub use shared_protocol_objects::client::{ReplClient, ProcessClientAdapter};

use anyhow::{anyhow, Result};
use console::style;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use serde_json::Value;
use std::path::PathBuf;
use tokio::process::Command;
use tokio::time::Duration;
use crate::conversation_service::handle_assistant_response;
use crate::host::MCPHost;
use shared_protocol_objects::Role;
use shared_protocol_objects::rpc::{McpClientBuilder, ProcessTransport};
use shared_protocol_objects::client::McpClientAdapter;

/// Main REPL implementation with enhanced CLI features
pub struct Repl {
    editor: DefaultEditor,
    command_processor: CommandProcessor,
    helper: ReplHelper,
    history_path: PathBuf,
    host: Option<MCPHost>,
}

impl Repl {
    /// Create a new REPL
    pub fn new() -> Result<Self> {
        // Set up config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp");
            
        std::fs::create_dir_all(&config_dir)?;
        let history_path = config_dir.join("history.txt");
        
        // Initialize the editor
        let mut editor = DefaultEditor::new()?;
        
        // Load history if it exists
        if history_path.exists() {
            let _ = editor.load_history(&history_path);
        }
        
        // Create helper and command processor
        let helper = ReplHelper::new();
        let command_processor = CommandProcessor::new();
        
        Ok(Self {
            editor,
            command_processor,
            helper,
            history_path,
            host: None,
        })
    }
    
    /// Set the MCPHost instance to enable enhanced features
    pub fn with_host(mut self, host: MCPHost) -> Self {
        self.host = Some(host);
        self
    }
    
    /// Run the REPL
    pub async fn run(&mut self) -> Result<()> {
        println!("\n{}", style("MCP Host Interactive Console").cyan().bold());
        println!("Type {} for available commands, or {} to enter AI chat mode", 
            style("help").yellow(), 
            style("chat <server>").green());
        
        loop {
            let prompt = match self.command_processor.current_server_name() {
                Some(server) => format!("{}> ", style(server).green()),
                None => "mcp> ".to_string(),
            };
            
            let readline = self.editor.readline(&prompt);
            
            match readline {
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    
                    self.editor.add_history_entry(line)?;
                    
                    // Check for host commands that require the MCPHost
                    if line.starts_with("chat ") && self.host.is_some() {
                        if let Err(e) = self.handle_chat_command(&line[5..].trim()).await {
                            println!("{}: {}", style("Error").red().bold(), e);
                        }
                    } else {
                        match self.command_processor.process(line).await {
                            Ok(result) => {
                                if result == "exit" {
                                    break;
                                }
                                
                                if !result.is_empty() {
                                    println!("{}", result);
                                }
                            }
                            Err(e) => {
                                println!("{}: {}", style("Error").red().bold(), e);
                            }
                        }
                    }
                    
                    // Update helper with current server names
                    self.helper.update_server_names(self.command_processor.server_names());
                }
                Err(ReadlineError::Interrupted) => {
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    println!("^D");
                    break;
                }
                Err(err) => {
                    println!("Error: {}", err);
                    break;
                }
            }
        }
        
        // Save history
        let _ = self.editor.save_history(&self.history_path);
        
        // Close all connections
        let cmd_processor = std::mem::take(&mut self.command_processor);
        cmd_processor.close().await?;
        
        Ok(())
    }
    
    /// Enhanced chat command that uses the MCPHost's AI capabilities
    async fn handle_chat_command(&self, server_name: &str) -> Result<()> {
        let host = self.host.as_ref().ok_or_else(|| anyhow!("Host not initialized"))?;
        
        match host.enter_chat_mode(server_name).await {
            Ok(mut state) => {
                println!("\n{}", style("Entering chat mode with tools. Type 'exit' or 'quit' to leave.").green());
                
                loop {
                    println!("\n{}", style("User:").blue().bold());
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let user_input = input.trim();
                    
                    if user_input.eq_ignore_ascii_case("exit") || user_input.eq_ignore_ascii_case("quit") {
                        println!("{}", style("Exiting chat mode.").yellow());
                        break;
                    }
                    
                    state.add_user_message(user_input);
                    
                    // Check if we have an AI client
                    if let Some(client) = host.ai_client() {
                        println!("{}", style(format!("Using AI model: {}", client.model_name())).dim());
                        
                        // First ask the LLM to decide whether to use a tool or respond directly
                        println!("{}", style("Analyzing your request...").dim());
                        let decision_request: Result<String, anyhow::Error> = with_progress(
                            "Deciding next action".to_string(), 
                            async {
                                let mut builder = client.raw_builder();
                                
                                // Add all messages to the builder for context
                                for msg in &state.messages {
                                    match msg.role {
                                        Role::System => builder = builder.system(msg.content.clone()),
                                        Role::User => builder = builder.user(msg.content.clone()),
                                        Role::Assistant => builder = builder.assistant(msg.content.clone()),
                                    }
                                }
                                
                                // Add the decision request as a system message
                                let tools_info = state.tools.iter()
                                    .map(|t| format!("- {}: {}", 
                                        t.name, 
                                        t.description.as_ref().unwrap_or(&"No description".to_string())
                                    ))
                                    .collect::<Vec<String>>()
                                    .join("\n");
                                
                                let decision_prompt = format!(
                                    "Given the conversation so far, decide whether to call a tool or provide a direct response to the user. \
                                    You must respond with a JSON object with a 'choice' field set to either 'tool_call' or 'finish_response'.\n\n\
                                    Available tools:\n{}\n\n\
                                    Format: {{\"choice\": \"tool_call\"}} or {{\"choice\": \"finish_response\"}}", 
                                    tools_info
                                );
                                
                                builder = builder.system(decision_prompt);
                                
                                builder.execute().await
                            }
                        ).await;
                        
                        match decision_request {
                            Ok(decision_str) => {
                                // Process the AI's decision
                                if let Err(e) = handle_assistant_response(
                                    host, 
                                    &decision_str, 
                                    server_name, 
                                    &mut state, 
                                    client, 
                                    None
                                ).await {
                                    println!("{}: {}", style("Error").red().bold(), e);
                                }
                            }
                            Err(e) => println!("{}: {}", style("Error").red().bold(), e),
                        }
                    } else {
                        println!("{}", style("Error: No AI client configured. Set ANTHROPIC_API_KEY, OPENAI_API_KEY, or another supported API key environment variable.").red());
                        break;
                    }
                }
                
                Ok(())
            }
            Err(e) => Err(anyhow!("Error entering chat mode: {}", e)),
        }
    }
    
    /// Load a configuration file
    pub async fn load_config(&mut self, config_path: &str) -> Result<()> {
        println!("{}", style(format!("Loading configuration from: {}", config_path)).yellow());
        
        // If we have a host, use that to load the config
        if let Some(host) = &self.host {
            host.load_config(config_path).await?;
            println!("{}", style("Successfully loaded configuration using host").green());
            return Ok(());
        }
        
        // Otherwise, use the REPL's own config loading
        let config_path = PathBuf::from(config_path);
        let config_str = tokio::fs::read_to_string(&config_path).await?;
        let config: Value = serde_json::from_str(&config_str)?;
        
        // Get the mcpServers object
        let servers = config.get("mcpServers")
            .ok_or_else(|| anyhow!("No 'mcpServers' key found in config"))?
            .as_object()
            .ok_or_else(|| anyhow!("'mcpServers' is not an object"))?;
            
        for (name, server_config) in servers {
            let cmd_value = server_config.get("command")
                .ok_or_else(|| anyhow!("Server '{}' is missing 'command' field", name))?;
                
            let command = cmd_value.as_str()
                .ok_or_else(|| anyhow!("Server '{}' command is not a string", name))?;
                
            // Create a Map that lives for the whole scope
            let default_map = serde_json::Map::new();
            let env = server_config.get("env")
                .and_then(|v| v.as_object())
                .unwrap_or(&default_map);
                
            // Build the command
            let mut cmd_parts = command.split_whitespace();
            let cmd_name = cmd_parts.next().ok_or_else(|| anyhow!("Empty command"))?;
            
            let mut cmd = Command::new(cmd_name);
            cmd.args(cmd_parts);
            
            // Add environment variables
            for (key, value) in env {
                if let Some(val_str) = value.as_str() {
                    cmd.env(key, val_str);
                }
            }
            
            // Start the server 
            println!("Starting server '{}'...", style(name).yellow());
            self.start_server(name, cmd).await?;
        }
        
        // Set config path
        self.command_processor.set_config_path(config_path);
        
        Ok(())
    }
    
    /// Start a server with the given command
    pub async fn start_server(&mut self, name: &str, command: Command) -> Result<()> {
        println!("Starting server: {}", style(name).yellow());
        
        // Create the transport
        let transport = ProcessTransport::new(command).await?;
        
        // Create and initialize the client
        let client = McpClientBuilder::new(transport)
            .client_info("mcp-host-repl", "1.0.0")
            .connect().await?;
            
        // Create the adapter
        let adapter = Box::new(ProcessClientAdapter::new(client, name.to_string()));
        
        // Add the server
        self.command_processor.add_server(adapter)?;
        
        // Update helper with current server names
        self.helper.update_server_names(self.command_processor.server_names());
        
        Ok(())
    }
}

/// Helper function for progress spinner
pub async fn with_progress<F, T>(msg: String, future: F) -> T 
where
    F: std::future::Future<Output = T>,
{
    use console::Term;
    
    let term = Term::stderr();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut i = 0;
    
    // Clone the message and term for the spawned task
    let progress_msg = msg.clone();
    let progress_term = term.clone();
    
    let handle = tokio::spawn(async move {
        loop {
            // Write the spinner and message, staying on same line
            progress_term.write_str(&format!("\r{} {}", spinner[i], progress_msg))
                .unwrap_or_default();
            // Ensure the line is flushed
            progress_term.flush().unwrap_or_default();
            
            i = (i + 1) % spinner.len();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let result = future.await;
    handle.abort();
    // Clear the progress line completely
    term.clear_line().unwrap_or_default();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::test_utils::MockTransport;
    
    #[tokio::test]
    async fn test_repl_commands() {
        // Create a command processor
        let mut processor = CommandProcessor::new();
        
        // Create a mock client with Mock transport
        let transport = MockTransport::new();
        let mut client = McpClientBuilder::new(transport)
            .client_info("test", "1.0.0")
            .build();
        
        // Initialize the client first
        let caps = shared_protocol_objects::ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        };
        client.initialize(caps).await.unwrap();
            
        // Add a server using the correct adapter type
        let adapter = Box::new(McpClientAdapter::<MockTransport>::new(client, "test-server".to_string()));
        processor.add_server(adapter).unwrap();
        
        // Test help command
        let result = processor.process("help").await.unwrap();
        assert!(result.contains("Available commands"));
        
        // Test servers command
        let result = processor.process("servers").await.unwrap();
        assert!(result.contains("test-server"));
        
        // Test tools command
        let result = processor.process("tools test-server").await.unwrap();
        assert!(result.contains("test_tool"));
    }
}