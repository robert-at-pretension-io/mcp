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
use rmcp::model::Role;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory; // Import History types (Removed unused History trait)
use rustyline::Editor;
use std::path::PathBuf;
// Removed unused import: use std::sync::Arc;
// Removed unused import: use tokio::process::Command as TokioCommand;
// Removed unused import: use tokio::sync::Mutex;
use tokio::time::Duration;

// Removed unused import: use crate::conversation_service::handle_assistant_response;
use crate::host::MCPHost;
// Define Role locally if not directly available from rllm 1.1.7

use crate::conversation_logic::{generate_verification_criteria}; // Removed VerificationOutcome import
use crate::conversation_service::generate_tool_system_prompt; // Import tool prompt generator
use crate::conversation_state::ConversationState; // Import ConversationState

/// Main REPL implementation with enhanced CLI features
pub struct Repl<'a> { // Add lifetime 'a
    editor: Editor<ReplHelper, DefaultHistory>, // Specify History type
    command_processor: CommandProcessor<'a>, // Use lifetime 'a
    // helper field removed, it's now owned by the Editor
    history_path: PathBuf,
    host: MCPHost, // Store host directly, not Option
    chat_state: Option<(String, ConversationState)>, // (server_name, state) - Active chat session
    loaded_conversation: Option<ConversationState>, // Holds state when not actively chatting
    current_conversation_path: Option<PathBuf>, // Path for save/load
    verify_responses: bool, // Added flag for verification
}

// Add lifetime 'a here
impl<'a> Repl<'a> {
    /// Create a new REPL, requires an initialized MCPHost
    pub fn new(host: MCPHost) -> Result<Self> {
        // Set up config directory
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp");

        std::fs::create_dir_all(&config_dir)?;
        let history_path = config_dir.join("history.txt");

        // Initialize the editor with the ReplHelper and DefaultHistory types.
        let mut editor = Editor::<ReplHelper, DefaultHistory>::new()?;

        // Load history early if it exists
        if history_path.exists() {
            if let Err(e) = editor.load_history(&history_path) {
                // Log or print warning, but don't fail creation
                log::warn!("Failed to load history from {}: {}", history_path.display(), e);
                // Optionally: println!("{}: Failed to load history: {}", style("Warning").yellow(), e);
            }
        }

        // Create the Repl instance *before* the CommandProcessor
        // Note: CommandProcessor needs a mutable borrow of Repl, so we create Repl first.
        // Create command processor simply now
        let command_processor = CommandProcessor::new(host.clone());

        let repl_instance = Self {
            editor, // Move editor into the instance
            command_processor, // Assign the created processor
            history_path,
            host: host.clone(), // Clone host for the Repl instance
            chat_state: None,
            loaded_conversation: None,
            current_conversation_path: None,
            verify_responses: false,
        };

        // Remove the problematic assignment and extra creation step that caused borrow errors
        // let command_processor = CommandProcessor::new(host, &mut repl_instance);
        // repl_instance.command_processor = command_processor;

        Ok(repl_instance) // Return the fully constructed instance

    }

    /// Sets the path for the current conversation file.
    pub fn set_current_conversation_path(&mut self, path: Option<PathBuf>) {
        self.current_conversation_path = path;
        if let Some(p) = &self.current_conversation_path {
            log::info!("Current conversation file path set to: {:?}", p);
        } else {
            log::info!("Current conversation file path cleared.");
        }
    }

    /// Gets the directory where conversations should be stored.
    fn get_conversations_dir(&self) -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not determine config directory"))?
            .join("mcp");
        let conversations_dir = config_dir.join("conversations");
        // Ensure the directory exists (create if not)
        std::fs::create_dir_all(&conversations_dir)?;
        Ok(conversations_dir)
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
                // --- Active Chat Mode Prompt ---
                let context_display = if server_context == "*all*" {
                    style("(all servers)").dim().to_string()
                } else {
                    style(server_context).green().to_string()
                };
                log::debug!("Generating active chat prompt for context: {}", server_context);
                format!("{} {}❯ ", style("Chat").magenta(), context_display)
            } else {
                // --- Normal Command Mode Prompt ---
                let conversation_indicator = if self.loaded_conversation.is_some() {
                    // Indicate a conversation is loaded but not active
                    style("[chat loaded]").dim().to_string()
                } else {
                    "".to_string()
                };
                log::debug!("Generating normal command prompt. Current server: {:?}, Provider: {}, Loaded Conv: {}",
                            self.command_processor.current_server_name(), provider_part, self.loaded_conversation.is_some());
                format!("{} {} {}❯ ", server_part, ai_info_part, conversation_indicator).trim_end().to_string() + " " // Ensure space before cursor
            };

            // The helper is now part of the editor, no need to set it here.

            // --- Read Line ---
            log::debug!("Attempting to read line with prompt: '{}'", prompt);
            let readline_result = self.editor.readline(&prompt);

            // --- Handle Readline Result ---
            let line = match readline_result {
                Ok(l) => l, // Successfully read line
                Err(ReadlineError::Interrupted) => { // Ctrl+C
                    // Prefix unused server_context with _
                    if let Some((_server_context, state)) = self.chat_state.take() {
                        log::debug!("Ctrl+C detected in chat mode, moving state to loaded_conversation.");
                        println!("\n{}", style("Exited chat input. Conversation loaded. Type 'chat' to resume or '/<command>'. Use 'new_chat' to clear.").yellow());
                        self.loaded_conversation = Some(state); // Keep the state
                        // Keep self.current_conversation_path as is
                    } else {
                        log::debug!("Ctrl+C detected in normal mode.");
                        println!("\n{}", style("^C").yellow()); // Style ^C, add newline
                    }
                    continue; // Continue to next REPL iteration
                }
                Err(ReadlineError::Eof) => { // Ctrl+D
                    if let Some((_server_context, state)) = self.chat_state.take() {
                        log::debug!("Ctrl+D detected in chat mode, moving state to loaded_conversation.");
                        println!("{}", style("\nExited chat input. Conversation loaded. Type 'chat' to resume or '/<command>'. Use 'new_chat' to clear.").yellow());
                        self.loaded_conversation = Some(state); // Keep the state
                        // Keep self.current_conversation_path as is
                        continue; // Continue REPL loop (now outside chat input)
                    } else {
                        log::debug!("Ctrl+D detected in normal mode, exiting REPL.");
                        println!("{}", style("\n^D").yellow()); // Style ^D, add newline
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
            if let Some((server_context, mut state)) = self.chat_state.take() { // Take ownership to modify state
                // --- In Active Chat Mode ---
                log::debug!("Processing input in active chat mode for context '{}': '{}'", server_context, line);

                // Check for chat-specific commands first
                if line.eq_ignore_ascii_case("exit") || line.eq_ignore_ascii_case("quit") {
                    println!("{}", style("\nExited chat input. Conversation loaded. Type 'chat' to resume or '/<command>'. Use 'new_chat' to clear.").yellow());
                    log::debug!("User requested exit from chat mode, moving state to loaded_conversation.");
                    self.loaded_conversation = Some(state); // Keep the state
                    // Keep self.current_conversation_path as is
                } else if line.starts_with('/') {
                    // --- Process REPL Command While in Chat Mode ---
                    let command_line = line[1..].trim(); // Remove leading '/'
                    log::info!("Processing REPL command from within chat: '{}'", command_line);
                    // Put the state back temporarily so CommandProcessor can potentially access it via the repl argument
                    self.chat_state = Some((server_context.clone(), state));
                    // Process the command, passing self (as &mut Repl)
                    let process_result = self.command_processor.process(
                        self, // Pass mutable reference to self (Repl)
                        command_line,
                        self.verify_responses,
                        &mut self.editor
                    ).await;
                    // Take the state back after processing
                    let (server_context, state) = self.chat_state.take().unwrap(); // Remove mut state here

                    match process_result {
                        Ok((output_string, new_verify_state)) => {
                            if let Some(new_state) = new_verify_state { self.verify_responses = new_state; }
                            if output_string == "exit" { // Handle exit command from within chat
                                log::info!("'exit' command received from within chat, breaking REPL loop.");
                                self.loaded_conversation = Some(state); // Save state before exiting
                                break;
                            }
                            if !output_string.is_empty() { println!("{}", output_string); }
                            // Put state back to continue chat
                            self.chat_state = Some((server_context, state));
                        }
                        Err(e) => {
                            log::error!("Error processing command '{}' from within chat: {}", command_line, e);
                            println!("{}: {}", style("Error").red().bold(), e);
                            // Put state back to continue chat despite command error
                            self.chat_state = Some((server_context, state));
                        }
                    }
                } else if line.eq_ignore_ascii_case("compact") { // Keep compact as a chat-specific keyword for now
                    // --- Handle Compact Command ---
                    log::info!("Compact command received in active chat mode.");
                    println!("{}", style("Compacting conversation history...").dim());
                    match self.execute_compact_conversation(&server_context, &state).await {
                        Ok(new_state) => {
                            println!("{}", style("Conversation compacted successfully.").green());
                            log::info!("Conversation compacted. New state has {} messages.", new_state.messages.len());
                            // Put the new compacted state back
                            self.chat_state = Some((server_context.clone(), new_state));
                        }
                        Err(e) => {
                            log::error!("Error during conversation compaction: {}", e);
                            println!("{}: {}", style("Compaction Error").red().bold(), e);
                            // Put the *original* state back into chat_state if compaction fails
                            self.chat_state = Some((server_context.clone(), state));
                        }
                    }
                } else {
                    // --- Normal Chat Turn ---
                    // Execute chat turn logic using the helper function
                    match self.execute_chat_turn(&server_context, &mut state, line).await {
                        Ok(_) => {
                            log::debug!("Chat turn executed successfully for context '{}'. Putting state back into chat_state.", server_context);
                            // Put the potentially modified state back into chat_state
                            self.chat_state = Some((server_context.clone(), state)); // Clone server_context
                        }
                        Err(e) => {
                            log::error!("Error during chat turn for context '{}': {}", server_context, e);
                            println!("{}: {}", style("Chat Error").red().bold(), e);
                            println!("{}", style("Exiting chat input due to error. Conversation loaded.").yellow());
                            // Move the potentially modified state to loaded_conversation on error
                            self.loaded_conversation = Some(state);
                            // Keep self.current_conversation_path as is
                        }
                    }
                }
            } else {
                // --- Not In Active Chat Mode ---
                log::debug!("Processing input in command mode: '{}'", line);

                // Check if it's a command or potentially a chat message to resume
                if line.starts_with('/') || self.command_processor.is_known_command(line) {
                    // --- Process REPL Command ---
                    let command_line = if line.starts_with('/') {
                        line[1..].trim()
                    } else {
                        line
                    };
                    log::debug!("Processing command: '{}'", command_line);
                    // Pass the current verification state, the mutable editor, and self (as &mut Repl).
                    let process_result = self.command_processor.process(
                        self, // Pass mutable reference to self (Repl)
                        command_line,
                        self.verify_responses, // Pass current state
                        &mut self.editor
                    ).await;

                    // Handle command result
                    match process_result {
                        Ok((output_string, new_verify_state)) => {
                            log::debug!("CommandProcessor result: '{}', New verify state: {:?}", output_string, new_verify_state);

                            // Update verify state if the command changed it
                            if let Some(new_state) = new_verify_state {
                                self.verify_responses = new_state;
                                log::info!("Updated verify_responses to: {}", new_state);
                            }

                            // Handle exit command
                            if output_string == "exit" {
                                log::info!("'exit' command received, breaking REPL loop.");
                                break; // Exit REPL
                            }
                            // Print command output if not empty
                            if !output_string.is_empty() {
                                println!("{}", output_string);
                            }
                        }
                        Err(e) => {
                            // The error is now wrapped in the Result from process
                            log::error!("Command processing error: {}", e);
                            println!("{}: {}", style("Error").red().bold(), e);
                        }
                    }
                } else if line.eq_ignore_ascii_case("chat") {
                    // --- Enter/Resume Chat Mode Command ---
                    log::debug!("'chat' command received.");
                    if let Some(state) = self.loaded_conversation.take() {
                        // --- Resume Loaded Conversation ---
                        log::info!("Resuming loaded conversation.");
                        // Determine server context (might need adjustment if loaded state doesn't store it)
                        // For now, assume multi-server context if loaded without active chat
                        let server_context = "*all*".to_string(); // Or retrieve from state if stored
                        let active_provider = self.host.get_active_provider_name().await.unwrap_or("none".to_string());
                        let active_model = self.host.ai_client().await.map(|c| c.model_name()).unwrap_or("?".to_string());
                        println!(
                            "\n{}",
                            style(format!(
                                "Resuming chat using provider '{}' (model: {}).",
                                style(&active_provider).cyan(),
                                style(&active_model).green()
                            )).italic()
                        );
                        println!("{}", style("Type '/exit' or press Ctrl+C/D to leave chat input.").dim());
                        self.chat_state = Some((server_context, state));
                    } else {
                        // --- Start New Chat (Multi-server default) ---
                        log::info!("Starting new multi-server chat session.");
                        match self.host.enter_multi_server_chat_mode().await {
                            Ok(mut initial_state) => { // Add mut here
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
                                // Generate and add tool instructions as the first user message
                                let tool_instructions = generate_tool_system_prompt(&initial_state.tools);
                                if !initial_state.tools.is_empty() {
                                    let tool_msg = format!("Okay, I have access to the following tools from all servers:\n{}", tool_instructions);
                                    initial_state.add_user_message(&tool_msg);
                                } else {
                                     let no_tool_msg = "No tools found on any active server.".to_string();
                                     initial_state.add_user_message(&no_tool_msg);
                                }
                                println!("{}", style("Type '/exit' or press Ctrl+C/D to leave chat input.").dim());
                                log::info!("Successfully entered multi-server chat mode.");
                                self.chat_state = Some(("*all*".to_string(), initial_state)); // Use special marker
                                self.current_conversation_path = None; // Clear path for new chat
                            }
                            Err(e) => {
                                log::error!("Failed to enter multi-server chat mode: {}", e);
                                println!("{}: Error entering multi-server chat mode: {}", style("Error").red().bold(), e);
                            }
                        }
                    }
                } else if self.loaded_conversation.is_some() {
                     // --- Implicitly Resume Chat ---
                     // Treat non-command input as a chat message if a conversation is loaded
                     log::debug!("Non-command input received while conversation loaded. Resuming chat.");
                     let state = self.loaded_conversation.take().unwrap(); // Take the loaded state
                     // Determine server context (default to *all* for now)
                     let server_context = "*all*".to_string();
                     println!("{}", style("(Resuming chat...)").dim()); // Indicate resumption
                     self.chat_state = Some((server_context.clone(), state)); // Put it into active chat_state
                     // Re-process the line as a chat turn
                     // Need to re-enter the chat processing logic block for this line
                     // This is a bit tricky with the current loop structure.
                     // Let's call execute_chat_turn directly here.
                     let mut current_state = self.chat_state.take().unwrap().1; // Get the state back
                     match self.execute_chat_turn(&server_context.clone(), &mut current_state, line).await {
                         Ok(_) => {
                             // Put the updated state back into active chat
                             self.chat_state = Some((server_context, current_state));
                         }
                         Err(e) => {
                             log::error!("Error during implicit chat resumption for context '{}': {}", server_context, e);
                             println!("{}: {}", style("Chat Error").red().bold(), e);
                             println!("{}", style("Exiting chat input due to error. Conversation loaded.").yellow());
                             // Move state to loaded on error
                             self.loaded_conversation = Some(current_state);
                         }
                     }

                } else {
                    // --- Unknown Command/Input ---
                    println!("{}: Unknown command or input '{}'. Type '/help' or 'chat'.", style("Error").red(), line);
                }


                // --- Old 'chat' command handling removed from here ---
                /*
                // --- Pass self (Repl) to command processor ---
                // We clone the editor temporarily because process needs mutable access
                // This is a bit awkward, maybe refactor CommandProcessor later
                // to not require mutable editor directly for non-interactive commands.
                // Pass the current verification state and the mutable editor.
                */
                let process_result = self.command_processor.process(
                    line,
                    self.verify_responses, // Pass current state
                    &mut self.editor
                ).await;
                // After processing, if the editor state changed (e.g., history),
                // it's already reflected in self.editor.

                if line.starts_with("chat") && process_result.is_err() && process_result.as_ref().err().map_or(false, |e| e.to_string().contains("Unknown command")) {
                     // --- Enter Chat Mode (Only if 'chat' wasn't handled as a command itself) ---
                     // Check the error string directly
                     // This allows potentially overriding 'chat' with a custom command later if needed.
                     log::debug!("Processing 'chat' command to enter chat mode.");
                     let parts: Vec<&str> = line.splitn(2, ' ').collect();
                    let target_server_opt = parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty());

                    if let Some(target_server) = target_server_opt {
                        // --- Specific Server Chat ---
                        log::info!("'chat' command detected for specific server: '{}'", target_server);
                        log::debug!("Attempting to enter single-server chat mode with '{}'", target_server);
                        match self.host.enter_chat_mode(target_server).await {
                            Ok(mut initial_state) => { // Add mut here
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
                                // Generate and add tool instructions as the first user message
                                let tool_instructions = generate_tool_system_prompt(&initial_state.tools);
                                if !initial_state.tools.is_empty() {
                                    let tool_msg = format!("Okay, I have access to the following tools on server '{}':\n{}", target_server, tool_instructions);
                                    // REMOVED: println!("{}", style(&tool_msg).dim()); // Print the tool list dimmed
                                    initial_state.add_user_message(&tool_msg);
                                } else {
                                    let no_tool_msg = format!("No tools found on server '{}'.", target_server);
                                     // REMOVED: println!("{}", style(&no_tool_msg).dim());
                                     initial_state.add_user_message(&no_tool_msg);
                                }
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
                            Ok(mut initial_state) => { // Add mut here
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
                                // Generate and add tool instructions as the first user message
                                let tool_instructions = generate_tool_system_prompt(&initial_state.tools);
                                if !initial_state.tools.is_empty() {
                                    let tool_msg = format!("Okay, I have access to the following tools from all servers:\n{}", tool_instructions);
                                    // REMOVED: println!("{}", style(&tool_msg).dim()); // Print the tool list dimmed
                                    initial_state.add_user_message(&tool_msg);
                                } else {
                                     let no_tool_msg = "No tools found on any active server.".to_string();
                                     // REMOVED: println!("{}", style(&no_tool_msg).dim());
                                     initial_state.add_user_message(&no_tool_msg);
                                }
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
                    // --- Process command result (already processed above) ---
                    match process_result {
                         Ok((output_string, new_verify_state)) => {
                             log::debug!("CommandProcessor result: '{}', New verify state: {:?}", output_string, new_verify_state);

                             // Update verify state if the command changed it
                             if let Some(new_state) = new_verify_state {
                                 self.verify_responses = new_state;
                                 log::info!("Updated verify_responses to: {}", new_state);
                             }

                             // Handle exit command
                             if output_string == "exit" {
                                log::info!("'exit' command received, breaking REPL loop.");
                                break; // Exit REPL
                            }
                            // Print command output if not empty
                            if !output_string.is_empty() {
                                println!("{}", output_string);
                            }
                        }
                        Err(e) => {
                            // The error is now wrapped in the Result from process
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

    /// Compacts the current conversation history using an LLM.
    async fn execute_compact_conversation(
        &self,
        _server_context: &str, // Keep for potential future use, but not needed now
        state: &ConversationState,
    ) -> Result<ConversationState> {
        log::debug!("Executing conversation compaction.");

        // 1. Get AI client
        let client = self.host.ai_client().await
            .ok_or_else(|| anyhow!("No AI client is active for compaction."))?;
        let model_name = client.model_name();
        log::debug!("Using AI client for model: {}", model_name);
        println!("{}", style(format!("Using AI model for compaction: {}", model_name)).dim());

        // 2. Format history for summarization prompt
        let history_string = state.messages.iter()
            .map(|msg| crate::conversation_state::format_chat_message(&msg.role, &msg.content))
            .collect::<Vec<String>>()
            .join("\n\n---\n\n");

        if history_string.is_empty() {
            return Err(anyhow!("Cannot compact an empty conversation history."));
        }

        // 3. Define summarization prompt
        let summarization_prompt = format!(
            "You are an expert conversation summarizer. Analyze the following conversation history and provide a concise summary. Focus on:\n\
            - Key user requests and goals.\n\
            - Important information discovered or generated.\n\
            - Decisions made.\n\
            - Final outcomes or current status.\n\
            - Any critical unresolved questions or next steps mentioned.\n\n\
            Keep the summary factual and brief, retaining essential context for the conversation to continue.\n\n\
            Conversation History:\n\
            ```\n\
            {}\n\
            ```\n\n\
            Concise Summary:",
            history_string
        );

        // 4. Call AI for summarization (use raw_builder, no tools needed)
        // Pass empty system prompt as it's not relevant for summarization itself
        let summary = crate::repl::with_progress(
            "Generating summary".to_string(),
            async {
                client.raw_builder("")
                    .user(summarization_prompt)
                    .execute()
                    .await
                    .map_err(|e| anyhow!("Summarization AI request failed: {}", e))
            }
        ).await?;

        log::debug!("Received summary (length: {})", summary.len());

        // 5. Create new state with original system prompt and tools
        // Use the *original* system prompt and tools from the *input* state
        let mut new_state = ConversationState::new(state.system_prompt.clone(), state.tools.clone());

        // 6. Add summary message to the new state
        let summary_message = format!(
            "Conversation history compacted. Key points from previous discussion:\n\n{}",
            summary.trim()
        );
        // Add as an assistant message to indicate it's a system action summary
        new_state.add_assistant_message(&summary_message);
        log::debug!("Added summary message to new state.");

        Ok(new_state)
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

        let mut final_user_input = user_input.to_string();
        let mut criteria_for_verification = String::new(); // Initialize empty

        // --- Generate Verification Criteria FIRST (only if enabled) ---
        if self.verify_responses {
            println!("{}", style("Generating verification criteria...").dim()); // Inform user
            let criteria_result = generate_verification_criteria(&self.host, user_input).await;

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
                    println!("{}", style("Verification criteria generated.").dim());
                }
                Ok(_) => {
                    // Criteria generation succeeded but was empty
                    log::debug!("Generated criteria were empty.");
                    println!("{}", style("No specific verification criteria generated for this request.").dim());
                    // criteria_for_verification remains empty
                }
                Err(e) => {
                    log::warn!("Failed to generate verification criteria: {}. Proceeding without verification.", e);
                    println!("{}: Failed to generate verification criteria: {}", style("Warning").yellow(), e);
                    // criteria_for_verification remains empty
                }
            }
        } else {
            log::debug!("Response verification is disabled.");
            // criteria_for_verification remains empty
        }
        // --- End Criteria Generation ---


        // 1. Add potentially modified user message to state
        state.add_user_message(&final_user_input); // Use the input (with or without appended criteria)
        log::debug!("Added user message to state. Total messages: {}", state.messages.len());

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
                // Add all messages *up to this point*. System prompt is handled by the builder.
                for msg in state.messages.iter() {
                     match msg.role {
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
