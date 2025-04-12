// Enhanced MCP Host REPL Implementation
// Merges REPL simplicity with CLI prompt enhancements
// connections module removed as MCPHost handles server management
mod command;
mod helper;


pub use command::CommandProcessor;
pub use helper::ReplHelper;
// Remove ServerConnections from public API
// pub use connections::ServerConnections;

// Import required types
use anyhow::{anyhow, Result};
use console::style;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory; // Import History types (Removed unused History trait)
use rustyline::Editor;
use std::path::PathBuf;
// Removed unused import: use std::sync::Arc;
use tokio::process::Command as TokioCommand; // Renamed to avoid conflict
// Removed unused import: use tokio::sync::Mutex;
use tokio::time::Duration;

// Removed unused import: use crate::conversation_service::handle_assistant_response;
use crate::host::MCPHost;
// Define Role locally if not directly available from rllm 1.1.7
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Role { System, User, Assistant } // Local definition matching usage
use crate::conversation_logic::generate_verification_criteria; // Import the function

/// Main REPL implementation with enhanced CLI features
pub struct Repl {
    editor: Editor<ReplHelper, DefaultHistory>, // Specify History type
    command_processor: CommandProcessor,
    // helper field removed, it's now owned by the Editor
    history_path: PathBuf,
    host: MCPHost, // Store host directly, not Option
    chat_state: Option<(String, crate::conversation_state::ConversationState)>, // (server_name, state)
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

        // Initialize the editor with the ReplHelper and DefaultHistory types.
        // ReplHelper::default() will be called internally.
        let editor = Editor::<ReplHelper, DefaultHistory>::new()?;

        // Create command processor with the host
        let command_processor = CommandProcessor::new(host.clone()); // Pass host clone only

        Ok(Self {
            editor, // Repl owns the editor with its helper
            command_processor,
            // helper field removed
            history_path,
            host, // Store the host
            chat_state: None, // Initialize chat state
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
                 // Clone provider_part here to avoid moving it before the debug log below
                 style(format!("({}{})", style(provider_part.clone()).cyan(), model_part)).dim().to_string()
            } else {
                 "".to_string() // No provider active, show nothing
            };

            // --- Prompt Generation ---
            let prompt = if let Some((server_context, _)) = &self.chat_state {
                // Chat mode prompt
                let context_display = if server_context == "*all*" {
                    style("(all servers)").dim().to_string()
                } else {
                    style(server_context).green().to_string()
                };
                log::debug!("Generating chat prompt for context: {}", server_context);
                format!("{} {}❯ ", style("Chat").magenta(), context_display)
            } else {
                // Normal command mode prompt
                log::debug!("Generating normal command prompt. Current server: {:?}, Provider: {}",
                            self.command_processor.current_server_name(), provider_part);
                format!("{} {}❯ ", server_part, ai_info_part)
            };

            // The helper is now part of the editor, no need to set it here.

            // --- Read Line ---
            log::debug!("Attempting to read line with prompt: '{}'", prompt);
            let readline_result = self.editor.readline(&prompt);

            // --- Handle Readline Result ---
            let line = match readline_result {
                Ok(l) => l, // Successfully read line
                Err(ReadlineError::Interrupted) => {
                    if self.chat_state.is_some() {
                        log::debug!("Ctrl+C detected in chat mode, exiting chat.");
                        println!("{}", style("Exiting chat mode (Ctrl+C).").yellow());
                        self.chat_state = None; // Exit chat mode
                    } else {
                        log::debug!("Ctrl+C detected in normal mode.");
                        println!("{}", style("^C").yellow()); // Style ^C
                    }
                    continue; // Continue to next REPL iteration
                }
                Err(ReadlineError::Eof) => {
                    if self.chat_state.is_some() {
                        log::debug!("Ctrl+D detected in chat mode, exiting chat.");
                        println!("{}", style("Exiting chat mode (Ctrl+D).").yellow());
                        self.chat_state = None; // Exit chat mode
                        continue; // Continue to next REPL iteration (now outside chat)
                    } else {
                        log::debug!("Ctrl+D detected in normal mode, exiting REPL.");
                        println!("{}", style("^D").yellow()); // Style ^D
                        break; // Exit REPL
                    }
                }
                Err(err) => {
                    log::error!("Readline error: {}", err);
                    println!("{}: {}", style("Error").red().bold(), err); // Keep error red
                    break; // Exit REPL on other errors
                }
            };

            // --- Process Input ---
            let line = line.trim();
            if line.is_empty() {
                log::debug!("Empty line entered, continuing.");
                continue; // Skip empty lines
            }

            // Add non-empty line to history (both commands and chat messages)
            log::debug!("Adding line to history: '{}'", line);
            if let Err(e) = self.editor.add_history_entry(line) {
                 log::warn!("Failed to add line to history: {}", e);
                 // Optional: Notify user?
                 // println!("{}: Failed to add to history: {}", style("Warning").yellow(), e);
            }

            // --- Process based on State (Chat or Command) ---
            if let Some((ref server_context, mut state)) = self.chat_state.take() { // Take ownership to modify state
                // --- In Chat Mode ---
                log::debug!("Processing input in chat mode for context '{}': '{}'", server_context, line);
                if line.eq_ignore_ascii_case("exit") || line.eq_ignore_ascii_case("quit") {
                    println!("{}", style("Exiting chat mode.").yellow());
                    log::debug!("User requested exit from chat mode.");
                    // self.chat_state remains None because we took it
                } else {
                    // Execute chat turn logic using the new helper function
                    // Pass the server_context (which might be "*all*")
                    match self.execute_chat_turn(server_context, &mut state, line).await {
                        Ok(_) => {
                            log::debug!("Chat turn executed successfully for context '{}'. Putting state back.", server_context);
                            // Put the potentially modified state back
                            self.chat_state = Some((server_context.clone(), state)); // Clone server_context
                        }
                        Err(e) => {
                            log::error!("Error during chat turn for context '{}': {}", server_context, e);
                            println!("{}: {}", style("Chat Error").red().bold(), e);
                            println!("{}", style("Exiting chat mode due to error.").yellow());
                            // self.chat_state remains None, exiting chat mode
                        }
                    }
                }
            } else {
                // --- Not In Chat Mode (Normal Command Processing) ---
                log::debug!("Processing input in command mode: '{}'", line);
                if line.starts_with("chat") {
                    // --- Enter Chat Mode ---
                    let parts: Vec<&str> = line.splitn(2, ' ').collect();
                    let target_server_opt = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty());

                    if let Some(target_server) = target_server_opt {
                        // --- Specific Server Chat ---
                        log::info!("'chat' command detected for specific server: '{}'", target_server);
                        log::debug!("Attempting to enter single-server chat mode with '{}'", target_server);
                        match self.host.enter_chat_mode(target_server).await {
                            Ok(initial_state) => {
                                let active_provider = self.host.get_active_provider_name().await.unwrap_or("none".to_string());
                                let active_model = self.host.ai_client().await.map(|c| c.model_name()).unwrap_or("?".to_string());
                                println!(
                                    "\n{}",
                                    style(format!(
                                        "Entering chat mode with server '{}' using provider '{}' (model: {}).",
                                        style(target_server).green(),
                                        style(&active_provider).cyan(),
                                        style(&active_model).green()
                                    )).italic()
                                );
                                println!("{}", style("Type 'exit' or 'quit' to leave.").dim());
                                log::info!("Successfully entered single-server chat mode with '{}'", target_server);
                                self.chat_state = Some((target_server.to_string(), initial_state));
                            }
                            Err(e) => {
                                log::error!("Failed to enter single-server chat mode for '{}': {}", target_server, e);
                                println!("{}: Error entering chat mode for '{}': {}", style("Error").red().bold(), target_server, e);
                            }
                        }
                    } else {
                        // --- Multi-Server Chat ---
                        log::info!("'chat' command detected with no server specified. Entering multi-server mode.");
                        match self.host.enter_multi_server_chat_mode().await {
                            Ok(initial_state) => {
                                let active_provider = self.host.get_active_provider_name().await.unwrap_or("none".to_string());
                                let active_model = self.host.ai_client().await.map(|c| c.model_name()).unwrap_or("?".to_string());
                                println!(
                                    "\n{}",
                                    style(format!(
                                        "Entering multi-server chat mode using provider '{}' (model: {}). Tools from all servers available.",
                                        style(&active_provider).cyan(),
                                        style(&active_model).green()
                                    )).italic()
                                );
                                println!("{}", style("Type 'exit' or 'quit' to leave.").dim());
                                log::info!("Successfully entered multi-server chat mode.");
                                self.chat_state = Some(("*all*".to_string(), initial_state)); // Use special marker
                            }
                            Err(e) => {
                                log::error!("Failed to enter multi-server chat mode: {}", e);
                                println!("{}: Error entering multi-server chat mode: {}", style("Error").red().bold(), e);
                            }
                        }
                    }
                } else {
                    // --- Process other commands ---
                    log::debug!("Passing non-chat command to CommandProcessor: '{}'", line);
                    match self.command_processor.process(line, &mut self.editor).await {
                        Ok(result) => {
                            log::debug!("CommandProcessor result: '{}'", result);
                            if result == "exit" {
                                log::info!("'exit' command received, breaking REPL loop.");
                                break; // Exit REPL
                            }
                            if !result.is_empty() {
                                println!("{}", result); // Print command output
                            }
                        }
                        Err(e) => {
                            log::error!("Command processing error: {}", e);
                            println!("{}: {}", style("Error").red().bold(), e);
                        }
                    }
                }
            }

            // --- Update Helper State (Runs regardless of mode) ---
            log::debug!("Updating REPL helper state.");
            // Update server names for completion
            let server_names = {
                        let servers_guard = self.host.servers.lock().await;
                        servers_guard.keys().cloned().collect::<Vec<String>>()
                    };
                    // Access helper via editor
                    if let Some(h) = self.editor.helper_mut() { h.update_server_names(server_names); }


                    // Update current tools list if a server is selected
                    if let Some(current_server_name) = self.command_processor.current_server_name() {
                        match self.host.list_server_tools(current_server_name).await {
                            Ok(tools) => {
                                if let Some(h) = self.editor.helper_mut() { h.update_current_tools(tools); }
                            },
                            Err(e) => {
                                // Don't print error here, just clear tools if listing fails
                                println!("{}: Failed to get tools for '{}': {}", style("Warning").yellow(), current_server_name, e);
                                if let Some(h) = self.editor.helper_mut() { h.update_current_tools(Vec::new()); }
                            }
                        }
                    } else {
                        // No server selected, clear the tools list
                        if let Some(h) = self.editor.helper_mut() { h.update_current_tools(Vec::new()); }
                    }

                    // Update available providers for completion
                    let available_providers = self.host.list_available_providers().await;
                    if let Some(h) = self.editor.helper_mut() { h.update_available_providers(available_providers); }


                    // Update available models for the current provider
                    if let Some(active_provider) = self.host.get_active_provider_name().await {
                        let models = { // Scope lock
                            let models_config_guard = self.host.provider_models.lock().await; // Lock the models config
                            let provider_key = active_provider.to_lowercase();
                            // --- Add detailed logging ---
                            let available_keys: Vec<_> = models_config_guard.providers.keys().cloned().collect();
                            log::debug!(
                                "Helper Update: Looking for key '{}'. Available keys: {:?}",
                                provider_key,
                                available_keys
                            );
                            // --- End detailed logging ---
                            models_config_guard.providers // Access the inner HashMap
                                .get(&provider_key) // Use lowercase key
                                .map(|list| list.models.clone()) // Clone the Vec<String> if found
                                .unwrap_or_default() // Return empty Vec if not found
                        };
                        log::debug!("Updating helper with {} suggested models for provider '{}'", models.len(), active_provider); // Log the count
                        if let Some(h) = self.editor.helper_mut() { h.update_current_provider_models(models); } // Update the helper
                    } else {
                        // No provider active, clear models
                        log::debug!("No active provider, clearing suggested models in helper.");
                        if let Some(h) = self.editor.helper_mut() { h.update_current_provider_models(Vec::new()); }
                    }
                    // --- End helper state update ---

            } // End of main loop processing block


        // Save history before exiting
        if let Err(e) = self.editor.save_history(&self.history_path) {
            println!("{}: Failed to save history to {}: {}", style("Error").red().bold(), self.history_path.display(), e); // Keep error red
        }

        // Close the command processor (now a no-op)
        // self.command_processor.close().await?; // Close is likely handled by MCPHost now

        Ok(())
    }

    /// Executes one turn of the chat interaction.
    /// Takes user input, calls the AI, and handles the response (including tool calls).
    async fn execute_chat_turn(
        &mut self, // Changed to &mut self in case helper needs updates later
        server_name: &str,
        state: &mut crate::conversation_state::ConversationState,
        user_input: &str,
    ) -> Result<()> {
        log::debug!("Executing chat turn for server '{}'. Original user input: '{}'", server_name, user_input);

        // --- Generate Verification Criteria FIRST ---
        let criteria_result = generate_verification_criteria(&self.host, user_input).await;
        let mut final_user_input = user_input.to_string();
        let criteria_for_verification: String; // Store clean criteria for verification step

        match criteria_result {
            Ok(c) if !c.is_empty() => {
                log::debug!("Generated criteria:\n{}", c);
                criteria_for_verification = c.clone(); // Store for later verification
                // Append criteria to the user input that the LLM will see
                final_user_input.push_str(&format!(
                    "\n\n---\n**Note:** Your response will be evaluated against the following criteria:\n{}\n---",
                    c
                ));
                log::debug!("Appended criteria to user input for LLM.");
            }
            Ok(_) => {
                // Criteria generation succeeded but was empty
                log::debug!("Generated criteria were empty.");
                criteria_for_verification = String::new();
            }
            Err(e) => {
                log::warn!("Failed to generate verification criteria: {}. Proceeding without verification.", e);
                criteria_for_verification = String::new(); // Use empty criteria if generation fails
            }
        }
        // --- End Criteria Generation ---

        // 1. Add potentially modified user message to state
        state.add_user_message(&final_user_input); // Use the input with appended criteria
        log::debug!("Added user message (potentially with criteria) to state. Total messages: {}", state.messages.len());

        // 2. Get AI client
        let client = self.host.ai_client().await
            .ok_or_else(|| {
                log::error!("No AI client active during chat turn.");
                anyhow!("No AI client is active. Use 'providers' and 'provider <name>'.")
            })?;
        let model_name = client.model_name(); // Get model name for logging
        log::debug!("Using AI client for model: {}", model_name);

        // 3. Print model info (optional, kept for consistency)
        println!("{}", style(format!("Using AI model: {}", model_name)).dim());

        // 4. Build *initial* request and call AI (using with_progress for the first call)
        println!("{}", style("Analyzing your request...").dim());
        let initial_response_result: Result<String> = crate::repl::with_progress( // Use with_progress for the *first* call
            "Getting initial response".to_string(),
            async {
                // Get system prompt from state helper method
                let system_prompt = state.get_system_prompt().unwrap_or(""); // Use empty if not found
                let mut builder = client.raw_builder(system_prompt);
                log::trace!("Building raw AI request for initial chat turn.");
                // Add messages *up to this point* (excluding potential future tool results)
                for msg in state.messages.iter() {
                     match msg.role {
                         Role::System => builder = builder.system(msg.content.clone()),
                         Role::User => builder = builder.user(msg.content.clone()),
                         Role::Assistant => builder = builder.assistant(msg.content.clone()),
                         // Removed unreachable pattern
                     }
                }
                // Tool prompt is already included in state via ConversationState::new

                log::debug!("Executing initial AI request...");
                builder.execute().await.map_err(|e| {
                    log::error!("Initial AI execution failed: {}", e);
                    anyhow!("Initial AI request failed: {}", e)
                })
            }
        ).await;

        // 5. Process initial AI response using the new shared logic
        match initial_response_result {
            Ok(initial_response) => {
                log::debug!("Received initial AI response (length: {})", initial_response.len());

                // Configuration for the shared logic (interactive)
                // Use default config which now has max_tool_iterations = 3
                let config = crate::conversation_logic::ConversationConfig {
                    interactive_output: true,
                    ..Default::default() // Use default for max_tool_iterations
                };

                // Call the shared logic function, passing the criteria
                // It will handle printing, tool calls, verification, and return the outcome
                match crate::conversation_logic::resolve_assistant_response(
                    &self.host,
                    server_name,
                    state, // Pass mutable state
                    &initial_response, // Pass the first response
                    client, // Pass the client Arc
                    &config,
                    &criteria_for_verification, // Pass the clean criteria string
                )
                .await
                {
                    Ok(outcome) => {
                        // The final response was already printed by resolve_assistant_response
                        // The state has been mutated in place.
                        log::debug!(
                            "Chat turn resolved successfully. Verification passed: {:?}. Final state has {} messages.",
                            outcome.verification_passed, state.messages.len()
                        );
                        // Put the updated state back into the REPL's chat_state
                        self.chat_state = Some((server_name.to_string(), state.clone()));
                    }
                    Err(e) => {
                        // This error is from resolve_assistant_response itself (e.g., non-recoverable tool error)
                        log::error!("Error resolving assistant response: {}", e);
                        println!("{}: {}", style("Chat Error").red().bold(), e);
                        println!("{}", style("Exiting chat mode due to error.").yellow());
                        // Don't put state back, effectively exiting chat mode
                        // self.chat_state remains None as it was taken at the start of the outer block
                    }
                }
            }
            Err(e) => {
                log::error!("Initial AI decision request failed: {}", e);
                println!("{}: {}", style("Chat Error").red().bold(), e);
                println!("{}", style("Exiting chat mode due to initial AI error.").yellow());
                // Don't put state back
                // self.chat_state remains None
            }
        }
        Ok(())
    }

    // Removed load_config method. Config is loaded/reloaded via MCPHost.

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
