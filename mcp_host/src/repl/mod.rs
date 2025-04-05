// Enhanced MCP Host REPL Implementation 
// Merges REPL simplicity with CLI prompt enhancements
// Keep connections module for tests, will remove later
#[cfg(test)]
mod connections;
mod command;
mod helper;
#[cfg(test)]
mod test_utils;

pub use command::CommandProcessor;
pub use helper::ReplHelper;
// Remove ServerConnections from public API
// pub use connections::ServerConnections;

// Import required types
use crate::host::server_manager::ManagedServer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::process::Command;

use anyhow::{anyhow, Result};
use console::style;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use std::path::PathBuf;
use tokio::time::Duration;
use crate::conversation_service::handle_assistant_response;
use crate::host::MCPHost;
use shared_protocol_objects::Role;

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
    pub fn new(servers: Arc<Mutex<HashMap<String, ManagedServer>>>) -> Result<Self> {
        // Set up config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp");
            
        std::fs::create_dir_all(&config_dir)?;
        let history_path = config_dir.join("history.txt");
        
        // Initialize the editor
        let mut editor = DefaultEditor::new()?;

        // Create helper and command processor with servers map
        let helper = ReplHelper::new();
        let command_processor = CommandProcessor::new(servers);
        
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

        // Load history after printing welcome message but before the loop
        if self.history_path.exists() {
            if let Err(e) = self.editor.load_history(&self.history_path) {
                println!("{}: Failed to load history from {}: {}", style("Warning").yellow(), self.history_path.display(), e);
            }
        }

        loop {
            // Dynamically set the prompt based on the current server
            let prompt = match self.command_processor.current_server_name() {
                Some(server) => format!("{}> ", style(server).green()),
                None => format!("{}> ", style("mcp").dim()), // Dim prompt when no server selected
            };

            // Disabled helper temporarily due to compatibility issues with rustyline
            // let _ = self.editor.set_helper(Some(self.helper.clone()));

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

                    // Update helper state (server names and current tools)
                    if let Some(host) = &self.host {
                        // Update server names for completion
                        let server_names = {
                            let servers_guard = host.servers.lock().await;
                            servers_guard.keys().cloned().collect::<Vec<String>>()
                        };
                        self.helper.update_server_names(server_names);

                        // Update current tools list if a server is selected
                        if let Some(current_server_name) = self.command_processor.current_server_name() {
                            match host.list_server_tools(current_server_name).await {
                                Ok(tools) => self.helper.update_current_tools(tools),
                                Err(e) => {
                                    // Don't print error here, just clear tools if listing fails
                                    println!("{}: Failed to get tools for '{}': {}", style("Warning").yellow(), current_server_name, e);
                                    self.helper.update_current_tools(Vec::new());
                                }
                            }
                        } else {
                            // No server selected, clear the tools list
                            self.helper.update_current_tools(Vec::new());
                        }
                    }
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

        // Save history before exiting
        if let Err(e) = self.editor.save_history(&self.history_path) {
            println!("{}: Failed to save history to {}: {}", style("Error").red(), self.history_path.display(), e);
        }

        // Close the command processor (now a no-op)
        // self.command_processor.close().await?; // Close is likely handled by MCPHost now
        
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
                                
                                // Use the smiley-delimited format system prompt
                                let smiley_prompt = crate::conversation_service::generate_smiley_tool_system_prompt(&state.tools);
                                
                                builder = builder.system(smiley_prompt);
                                
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
        
        // We should always have a host now, so this code path should not be reached
        // For backward compatibility, we'll log a warning
        println!("{}", style("Warning: No host available. REPL-based config loading is deprecated.").yellow());
        
        // Set config path
        self.command_processor.set_config_path(PathBuf::from(config_path));
        
        Ok(())
    }
    
    /// This method is deprecated and will be removed
    #[deprecated(note = "Use MCPHost.start_server instead")]
    pub async fn start_server(&mut self, _name: &str, _command: Command) -> Result<()> {
        println!("{}", style("Warning: Direct server start is deprecated. Use an MCPHost instance instead.").yellow());
        Ok(())
    }
}

/// Truncate a string to a maximum number of lines.
pub fn truncate_lines(text: &str, max_lines: usize) -> String { // Make this function public
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        text.to_string()
    } else {
        let truncated_lines = lines.into_iter().take(max_lines).collect::<Vec<_>>();
        format!("{}\n\n{}", truncated_lines.join("\n"), style("... (output truncated)").dim())
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
