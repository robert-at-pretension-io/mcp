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
use anyhow::{anyhow, Result};
use console::style;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use std::path::PathBuf;
// Removed unused import: use std::sync::Arc;
use tokio::process::Command as TokioCommand; // Renamed to avoid conflict
// Removed unused import: use tokio::sync::Mutex;
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
    host: MCPHost, // Store host directly, not Option
}

impl Repl {
    /// Create a new REPL, requires an initialized MCPHost
    pub fn new(host: MCPHost) -> Result<Self> {
        // Set up config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp");

        std::fs::create_dir_all(&config_dir)?;
        let history_path = config_dir.join("history.txt");

        // Initialize the editor (remove mut)
        let editor = DefaultEditor::new()?;

        // Create helper and command processor with the host
        let helper = ReplHelper::new();
        let command_processor = CommandProcessor::new(host.clone()); // Pass host clone only

        Ok(Self {
            editor, // Repl owns the editor
            command_processor,
            helper,
            history_path,
            host, // Store the host
        })
    }

    // with_host method removed as host is now required in new()

    /// Run the REPL
    pub async fn run(&mut self) -> Result<()> {
        log::info!("Repl::run started."); // Log at the very beginning of run()

        // Enhanced welcome message
        println!("\n{}", style("MCP Host Interactive Console").cyan().bold());
        println!("Type {} for commands, {} to chat.",
                 style("help").yellow(),
                 style("chat <server>").green());
        println!("{}", style("----------------------------------------").dim());


        // Load history after printing welcome message but before the loop
        if self.history_path.exists() {
            if let Err(e) = self.editor.load_history(&self.history_path) {
                println!("{}: Failed to load history from {}: {}", style("Warning").yellow(), self.history_path.display(), e);
            }
        }

        loop {
            // Dynamically set the prompt based on the current server and AI provider
            let server_part = match self.command_processor.current_server_name() {
                Some(server) => style(server).green().to_string(),
                None => style("mcp").dim().to_string(), // Dim if no server selected
            };
            let provider_part = match self.host.get_active_provider_name().await {
                 Some(provider) => provider,
                 None => "none".to_string(),
            };

            // Get the active model name if a client exists
            let model_part = match self.host.ai_client().await {
                Some(client) => format!(":{}", style(client.model_name()).green()),
                None => "".to_string(),
            };

            // Combine parts for the prompt - make AI info dimmer
            let ai_info_part = if provider_part != "none" {
                 style(format!("({}{})", style(provider_part).cyan(), model_part)).dim().to_string()
            } else {
                 "".to_string() // No provider active, show nothing
            };

            // Use a slightly different prompt character
            let prompt = format!("{} {}❯ ", server_part, ai_info_part);


            // Set the helper for completion and hinting
            // Pass a mutable reference to the helper
            // Note: This might cause lifetime issues if not handled carefully by rustyline,
            // but let's try it as it matches some patterns.
            self.editor.set_helper(Some(&mut self.helper));

            log::debug!("Attempting to read line with prompt: '{}'", prompt); // Add log here
            let readline = self.editor.readline(&prompt);

            match readline {
                // Add logging for each branch
                Ok(line) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    self.editor.add_history_entry(line)?;
                    log::debug!("Read line: '{}'", line);

                    // Check for chat command first
                    if line.starts_with("chat ") {
                        if let Err(e) = self.handle_chat_command(&line[5..].trim()).await {
                            println!("{}: {}", style("Error").red().bold(), e);
                        }
                    } else {
                        // Process other commands via CommandProcessor, passing the editor
                        match self.command_processor.process(line, &mut self.editor).await { // Pass editor here
                            Ok(result) => {
                                log::debug!("Command processed, result: '{}'", result);
                                if result == "exit" {
                                    break;
                                }

                                if !result.is_empty() {
                                    // Print command results normally (styling handled by command)
                                    println!("{}", result);
                                }
                            }
                            Err(e) => {
                                log::error!("Command processing error: {}", e);
                                println!("{}: {}", style("Error").red().bold(), e); // Keep errors red
                            }
                        }
                    }

                    // Update helper state (server names, current tools, available providers)
                    // Update server names for completion
                    let server_names = {
                        let servers_guard = self.host.servers.lock().await;
                        servers_guard.keys().cloned().collect::<Vec<String>>()
                    };
                    self.helper.update_server_names(server_names);

                    // Update current tools list if a server is selected
                    if let Some(current_server_name) = self.command_processor.current_server_name() {
                        match self.host.list_server_tools(current_server_name).await {
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

                    // Update available providers for completion
                    let available_providers = self.host.list_available_providers().await;
                    self.helper.update_available_providers(available_providers);

                    // Update available models for the current provider
                    if let Some(active_provider) = self.host.get_active_provider_name().await {
                        let models = { // Scope lock
                            let models_config_guard = self.host.provider_models.lock().await;
                            models_config_guard.providers
                                .get(&active_provider.to_lowercase())
                                .map(|list| list.models.clone())
                                .unwrap_or_default()
                        };
                        log::debug!("Updating helper with {} suggested models for provider '{}'", models.len(), active_provider);
                        self.helper.update_current_provider_models(models);
                    } else {
                        // No provider active, clear models
                        log::debug!("No active provider, clearing suggested models in helper.");
                        self.helper.update_current_provider_models(Vec::new());
                    }
                    // --- End helper state update ---

                }
                Err(ReadlineError::Interrupted) => {
                    log::debug!("Readline interrupted (Ctrl+C)");
                    println!("{}", style("^C").yellow()); // Style ^C
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    log::debug!("Readline EOF (Ctrl+D)");
                    println!("{}", style("^D").yellow()); // Style ^D
                    break;
                }
                Err(err) => {
                    log::error!("Readline error: {}", err);
                    println!("{}: {}", style("Error").red().bold(), err); // Keep error red
                    break;
                }
            }
        }

        // Save history before exiting
        if let Err(e) = self.editor.save_history(&self.history_path) {
            println!("{}: Failed to save history to {}: {}", style("Error").red().bold(), self.history_path.display(), e); // Keep error red
        }

        // Close the command processor (now a no-op)
        // self.command_processor.close().await?; // Close is likely handled by MCPHost now

        Ok(())
    }
    /// Enhanced chat command that uses the MCPHost's AI capabilities
    async fn handle_chat_command(&self, server_name: &str) -> Result<()> {
        // Use self.host directly
        match self.host.enter_chat_mode(server_name).await {
            Ok(mut state) => {
                let active_provider = self.host.get_active_provider_name().await.unwrap_or("none".to_string());
                let active_model = self.host.ai_client().await.map(|c| c.model_name()).unwrap_or("?".to_string());
                // Use italic and dim for the entry message
                println!(
                    "\n{}",
                    style(format!(
                        "Entering chat mode with server '{}' using provider '{}' (model: {}).",
                        style(server_name).green(),
                        style(&active_provider).cyan(),
                        style(&active_model).green()
                    )).italic()
                );
                println!("{}", style("Type 'exit' or 'quit' to leave.").dim());

                loop {
                    // Use magenta for the user prompt label
                    println!("\n{}", style("User:").magenta().bold());
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    let user_input = input.trim();

                    if user_input.eq_ignore_ascii_case("exit") || user_input.eq_ignore_ascii_case("quit") {
                        println!("{}", style("Exiting chat mode.").yellow());
                        break;
                    }

                    state.add_user_message(user_input);

                    // Get the current AI client from the host
                    if let Some(client) = self.host.ai_client().await {
                        // Dim the model info
                        println!("{}", style(format!("Using AI model: {}", client.model_name())).dim());

                        // First ask the LLM to decide whether to use a tool or respond directly
                        println!("{}", style("Analyzing your request...").dim()); // Dim analysis message
                        let decision_request: Result<String, anyhow::Error> = with_progress(
                            "Deciding next action".to_string(), // Progress message styled in with_progress
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
                            Ok(decision_string) => { // Bind to an owned String
                                // Process the AI's decision
                                if let Err(e) = handle_assistant_response(
                                    &self.host, // Pass reference to host
                                    &decision_string, // Use the owned String
                                    server_name,
                                    &mut state,
                                    client, // Pass the Arc<dyn AIClient>
                                    None
                                ).await {
                                    println!("{}: {}", style("Error").red().bold(), e);
                                }
                            }
                            Err(e) => println!("{}: {}", style("Error").red().bold(), e),
                        }
                    } else {
                        println!("{}", style("Error: No AI client is active. Use 'providers' to see available providers and 'provider <name>' to activate one.").red());
                        break; // Exit chat mode if no client is active
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

        // Use self.host directly to load the config
        self.host.load_config(config_path).await?;
        println!("{}", style("Successfully loaded configuration using host").green());

        // Reload the command processor with the potentially updated host state?
        // Or assume host updates its internal state which command_processor uses.
        // For now, assume host state is updated and command_processor uses the clone.

        Ok(())
    }

    /// This method is deprecated and will be removed
    #[deprecated(note = "Use MCPHost.start_server instead")]
    pub async fn start_server(&mut self, _name: &str, _command: TokioCommand) -> Result<()> {
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
    let progress_msg = style(msg).dim().to_string(); // Style the progress message
    let progress_term = term.clone();

    let handle = tokio::spawn(async move {
        loop {
            // Write the spinner and message, staying on same line
            progress_term.write_str(&format!("\r{} {}", style(spinner[i]).cyan(), progress_msg)) // Style spinner cyan
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
