use anyhow::{anyhow, Result};
use console::style;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write; // Import Write trait for building strings
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use log::{error, info, warn}; // Removed unused debug import
// Removed unused io imports

use crate::host::server_manager::ManagedServer;
use crate::host::MCPHost;
use crate::host::config::{ServerConfig};
use rustyline::Editor;
use rustyline::history::DefaultHistory;
use crate::repl::helper::ReplHelper;
use crate::repl::Repl; // Import Repl struct

/// Command processor for the REPL
// Remove lifetime parameter 'a
pub struct CommandProcessor {
    host: MCPHost, // Store the host instance
    servers: Arc<Mutex<HashMap<String, ManagedServer>>>, // Keep servers for direct access if needed
    current_server: Option<String>,
    config_path: Option<PathBuf>,
    // Remove the repl field to break circular reference
    // repl: &'a mut Repl<'a>,
}

// Remove lifetime parameter 'a
impl CommandProcessor {
    // Modify constructor to take only MCPHost
    pub fn new(host: MCPHost) -> Self {
        Self {
            servers: Arc::clone(&host.servers), // Get servers from host
            host,
            current_server: None,
            config_path: None,
            // repl field removed
        }
    }

    /// Check if a string corresponds to a known command.
    pub fn is_known_command(&self, line: &str) -> bool {
        let command = line.split_whitespace().next().unwrap_or("");
        // Add new commands here
        matches!(command,
            "help" | "exit" | "quit" | "servers" | "use" | "tools" | "call" |
            "provider" | "providers" | "model" | "add_server" | "edit_server" |
            "remove_server" | "save_config" | "reload_config" | "show_config" |
            "verify" | "save_chat" | "load_chat" | "new_chat"
            // Note: 'chat' is handled specially in the REPL loop
        )
    }


    /// Process a command string.
    /// Takes the current verification state and returns the output message
    /// and an optional new verification state if it was changed by the command.
    // Remove repl: &mut Repl argument, add mutable state fields needed by commands
    pub async fn process(
        &mut self,
        // Pass mutable references to the parts of Repl state needed by commands
        chat_state: &mut Option<(String, crate::conversation_state::ConversationState)>,
        loaded_conversation: &mut Option<crate::conversation_state::ConversationState>,
        current_conversation_path: &mut Option<PathBuf>,
        command: &str,
        current_verify_state: bool,
        editor: &mut Editor<ReplHelper, DefaultHistory>
    ) -> Result<(String, Option<bool>)> { // Return tuple: (output, Option<new_verify_state>)
        // Split the command into parts, respecting quotes
        let parts = match shellwords::split(command) {
            Ok(parts) => parts,
            Err(_) => return Err(anyhow!("Invalid command syntax (unmatched quotes?)"))
        };

        if parts.is_empty() {
            return Ok(("".to_string(), None)); // Return empty string and no state change
        }

        let cmd = &parts[0];
        let args = &parts[1..];
        // Most commands return Ok(message) which we map to Ok((message, None))
        // The 'verify' command is handled specially.
        let result: Result<(String, Option<bool>)> = match cmd.as_str() {
            "help" => self.cmd_help().map(|s| (s, None)),
            "exit" | "quit" => Ok(("exit".to_string(), None)), // Special string "exit"
            "servers" => self.cmd_servers().await.map(|s| (s, None)),
            "use" => self.cmd_use(args).await.map(|s| (s, None)),
            "tools" => self.cmd_tools(args).await.map(|s| (s, None)),
            "call" => self.cmd_call(args).await.map(|s| (s, None)),
            "provider" => self.cmd_provider(args).await.map(|s| (s, None)),
            "providers" => self.cmd_providers().await.map(|s| (s, None)),
            "model" => self.cmd_model(args).await.map(|s| (s, None)), // Added model command
            // chat command is handled directly in Repl::run
            "add_server" => self.cmd_add_server(editor).await.map(|s| (s, None)), // Pass editor
            "edit_server" => self.cmd_edit_server(args, editor).await.map(|s| (s, None)), // Pass editor
            "remove_server" => self.cmd_remove_server(args).await.map(|s| (s, None)), // New command
            "save_config" => self.cmd_save_config().await.map(|s| (s, None)), // New command
            "reload_config" => self.cmd_reload_config(editor).await.map(|s| (s, None)), // Pass editor
            "show_config" => self.cmd_show_config(args).await.map(|s| (s, None)),
            "verify" => self.cmd_verify(args, current_verify_state).await,
            // Pass mutable state fields to commands that need them
            "save_chat" => self.cmd_save_chat(chat_state, loaded_conversation, current_conversation_path, args).await.map(|s| (s, None)),
            "load_chat" => self.cmd_load_chat(chat_state, loaded_conversation, current_conversation_path, args).await.map(|s| (s, None)),
            "new_chat" => self.cmd_new_chat(chat_state, loaded_conversation, current_conversation_path).await.map(|s| (s, None)),
            _ => {
                 // Check if it looks like a chat command before declaring unknown
                 // 'chat' command is handled in the main REPL loop now
                 // if cmd == "chat" {
                 //     // Let the main REPL loop handle 'chat' if it wasn't explicitly overridden
                 //     // Return a specific error or signal to indicate this.
                 //     // Using the original "Unknown command" error works for now,
                 //     // as the REPL loop checks for this specific error.
                 //     Err(anyhow!("Unknown command: '{}'. Type 'help' for available commands", cmd))
                 // } else {
                     // Let the main REPL loop handle 'chat' if it wasn't explicitly overridden
                     // Return a specific error or signal to indicate this.
                     // Using the original "Unknown command" error works for now,
                     // as the REPL loop checks for this specific error.
                     Err(anyhow!("Unknown command: '{}'. Type 'help' for available commands", cmd))
                 }
                };
        
        result // Return the final result
                
    }

    /// Get available commands
    pub fn cmd_help(&self) -> Result<String> {
        let mut help_text = String::new();
        writeln!(help_text, "{}", style("Available commands:").bold())?;

        // Use more descriptive placeholders: <required>, [optional]
        let commands = [
            ("help", "Show this help message."),
            ("servers", "List configured servers and show the active one."),
            ("use [server_name]", "Set the default server for commands like 'tools' and 'call'. No argument clears selection."),
            ("tools [server_name]", "List tools for the active server (or specified server)."),
            ("call <tool_name> [server_name] [json_args]", "Call a tool. Uses active server and empty args '{}' if omitted."),
            ("chat <server_name>", "Enter interactive chat mode with the specified server, using the active AI provider."),
            ("provider [provider_name]", "Show or set the active AI provider (e.g., openai, anthropic, ollama)."),
            ("providers", "List AI providers with configured API keys."),
            ("model [model_name]", "Show or set the model for the active AI provider. Shows suggestions if no name given."),
            ("add_server", "Interactively add a new server configuration (auto-saved)."),
            ("edit_server <server_name>", "Interactively edit an existing server configuration (auto-saved)."),
            ("remove_server <server_name>", "Remove a server configuration (use 'save_config' to persist)."),
            ("show_config [server_name]", "Display the current configuration (all or a specific server)."),
            ("save_config", "Save server configuration changes to the file."),
            ("reload_config", "Reload server and provider model configs from files (discards unsaved changes)."),
            ("verify [on|off]", "Enable or disable AI response verification during chat (default: off)."),
            ("save_chat [filename]", "Save the current conversation to a JSON file (default: conversations/chat_<timestamp>.json)."),
            ("load_chat <filename>", "Load a conversation from a JSON file."),
            ("new_chat", "Clear the current loaded conversation."),
            ("exit, quit", "Exit the REPL."),
        ];

        for (cmd, desc) in commands {
            // Style command name yellow, description normal
            writeln!(help_text, "  {} - {}", style(cmd).yellow(), desc)?;
        }

        Ok(help_text)
    }

    // --- Interactive Add Server ---
    async fn cmd_add_server(&mut self, editor: &mut Editor<ReplHelper, DefaultHistory>) -> Result<String> { // Added History type
        println!("--- {} ---", style("Add New Server Configuration").cyan());

        let name = self.prompt_for_input("Enter unique server name:", editor)?; // Pass editor
        if name.is_empty() { return Ok(style("Cancelled.").yellow().to_string()); }
        // Check uniqueness
        {
            let config_guard = self.host.config.lock().await;
            if config_guard.servers.contains_key(&name) {
                return Err(anyhow!("Server name '{}' already exists.", name));
            }
        }


        let command = self.prompt_for_input("Enter command to run server:", editor)?; // Pass editor
        if command.is_empty() { return Ok(style("Cancelled.").yellow().to_string()); }

        let mut args = Vec::new();
        println!("{}", style("Enter command arguments (one per line, press Enter on empty line to finish):").dim());
        loop {
            let arg = self.prompt_for_input(&format!("Argument {}:", args.len() + 1), editor)?; // Pass editor
            if arg.is_empty() { break; }
            args.push(arg);
        }

        let mut env = HashMap::new();
        println!("{}", style("Enter environment variables (KEY=VALUE format, press Enter on empty line to finish):").dim());
        loop {
            let env_line = self.prompt_for_input("Env Var (e.g., KEY=value):", editor)?; // Pass editor
            if env_line.is_empty() { break; }
            if let Some((key, value)) = env_line.split_once('=') {
                env.insert(key.trim().to_string(), value.trim().to_string());
            } else {
                println!("{}", style("Invalid format. Use KEY=VALUE.").yellow());
            }
        }

        let server_config = ServerConfig {
            command,
            env,
            args: if args.is_empty() { None } else { Some(args) }, // Store args
        };

        // Add to in-memory config
        {
            let mut config_guard = self.host.config.lock().await;
            config_guard.servers.insert(name.clone(), server_config);
        }

        // Automatically save the configuration
        match self.host.save_host_config().await {
            Ok(_) => Ok(format!("Server '{}' added and configuration saved successfully.", style(&name).green())),
            Err(e) => {
                // Log the error, but still report success for adding to memory
                log::error!("Failed to automatically save config after adding server '{}': {}", name, e);
                Ok(format!("Server '{}' added to configuration, but {} to save automatically: {}. Run '{}' manually.",
                    style(&name).green(),
                    style("failed").red(),
                    e,
                    style("save_config").yellow()
                ))
            }
        }
    }


    // --- Verify Command ---
    /// Handles the 'verify' command.
    /// Takes the current verification state.
    /// Returns a tuple: (output_message, Option<new_state>)
    async fn cmd_verify(&self, args: &[String], current_state: bool) -> Result<(String, Option<bool>)> {
        if args.is_empty() {
            // Show current status - no state change
            let status = if current_state { style("on").green() } else { style("off").yellow() };
            Ok((format!("Response verification is currently {}.", status), None))
        } else {
            match args[0].to_lowercase().as_str() {
                "on" | "true" | "yes" | "enable" => {
                    // Request to enable - state changes to true
                    Ok((style("Response verification enabled.").green().to_string(), Some(true)))
                }
                "off" | "false" | "no" | "disable" => {
                    // Request to disable - state changes to false
                    Ok((style("Response verification disabled.").yellow().to_string(), Some(false)))
                }
                _ => Err(anyhow!("Invalid argument '{}'. Use 'on' or 'off'.", args[0])),
            }
        }
    }


    // --- Edit Server ---
    async fn cmd_edit_server(&mut self, args: &[String], editor: &mut Editor<ReplHelper, DefaultHistory>) -> Result<String> { // Added History type
        const DELETE_KEYWORD: &str = "DELETE";
        if args.is_empty() {
            return Err(anyhow!("Usage: edit_server <server_name>"));
        }
        let name = &args[0];
        println!("--- {} '{}' ---", style("Edit Server Configuration for").cyan(), style(name).green());
        println!("{}", style(format!("(Press Enter to keep current value, type '{}' to delete)", style(DELETE_KEYWORD).red())).dim());

        // --- Clone data and release lock BEFORE prompting ---
        let (mut edited_command, mut edited_args, mut edited_env) = {
            let config_guard = self.host.config.lock().await;
            let server_config = match config_guard.servers.get(name) { // Use immutable get
                Some(cfg) => cfg,
                None => return Err(anyhow!("Server '{}' not found in configuration.", name)),
            };
            // Clone the data needed for editing
            (
                server_config.command.clone(),
                server_config.args.clone().unwrap_or_default(),
                server_config.env.clone()
            )
        }; // config_guard is dropped here

        // --- Perform interactive editing using cloned data ---

        // Edit Command
        let command_prompt = format!("Command: ");
        let input_command = self.prompt_with_initial(&command_prompt, &edited_command, editor)?; // Pass editor
        if !input_command.is_empty() {
            edited_command = input_command;
        } else {
            println!("{}", style(format!("Keeping current command: {}", edited_command)).dim());
        }

        // Edit Arguments
        println!("\n--- {} ---", style("Editing Arguments").cyan());
        let mut final_args = Vec::new();
        for (i, arg) in edited_args.iter().enumerate() {
            let prompt = format!("Arg {}: ", i);
            let new_arg = self.prompt_with_initial(&prompt, arg, editor)?; // Pass editor
            if new_arg.eq_ignore_ascii_case(DELETE_KEYWORD) {
                println!("{}", style(format!("Deleting argument: {}", arg)).yellow());
            } else if new_arg.is_empty() {
                println!("{}", style(format!("Keeping argument: {}", arg)).dim());
                final_args.push(arg.clone());
            } else {
                final_args.push(new_arg);
            }
        }
        // Add new arguments
        println!("{}", style("--- Add New Arguments (Press Enter on empty line to finish) ---").dim());
        loop {
            let prompt = format!("New Arg {}: ", final_args.len());
            let new_arg = self.prompt_for_input(&prompt, editor)?; // Pass editor
            if new_arg.is_empty() { break; }
            final_args.push(new_arg);
        }
        edited_args = final_args; // Update the edited_args vec

        // Edit Environment Variables
        println!("\n--- {} ---", style("Editing Environment Variables").cyan());
        let mut final_env = HashMap::new();
        let mut sorted_keys: Vec<String> = edited_env.keys().cloned().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            let value = edited_env.get(&key).unwrap();
            let prompt = format!("Env '{}': ", key);
            let new_value = self.prompt_with_initial(&prompt, value, editor)?; // Pass editor
            if new_value.eq_ignore_ascii_case(DELETE_KEYWORD) {
                println!("{}", style(format!("Deleting env var: {}", key)).yellow());
            } else if new_value.is_empty() {
                println!("{}", style(format!("Keeping env var: {}={}", key, value)).dim());
                final_env.insert(key.clone(), value.clone());
            } else {
                final_env.insert(key.clone(), new_value);
            }
        }
        // Add new environment variables
        println!("{}", style("--- Add New Environment Variables (KEY=VALUE format, press Enter to finish) ---").dim());
        loop {
            let env_line = self.prompt_for_input("New Env Var (e.g., KEY=value):", editor)?; // Pass editor
            if env_line.is_empty() { break; }
            if let Some((key, value)) = env_line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                if !key.is_empty() {
                    final_env.insert(key.to_string(), value.to_string());
                } else {
                    println!("{}", style("Invalid format: Key cannot be empty.").yellow());
                }
            } else {
                println!("{}", style("Invalid format. Use KEY=VALUE.").yellow());
            }
        }
        edited_env = final_env; // Update the edited_env map

        // --- Re-acquire lock and update the actual config ---
        {
            let mut config_guard = self.host.config.lock().await;
            if let Some(server_config) = config_guard.servers.get_mut(name) {
                server_config.command = edited_command;
                server_config.args = if edited_args.is_empty() { None } else { Some(edited_args) };
                server_config.env = edited_env;
            } else {
                // Should not happen if we found it initially, but handle defensively
                return Err(anyhow!("Server '{}' disappeared during editing.", name));
            }
        } // Lock released here

        // --- Automatically save the configuration ---
        match self.host.save_host_config().await {
            Ok(_) => Ok(format!("Server '{}' configuration updated and saved successfully.", style(name).green())),
            Err(e) => {
                log::error!("Failed to automatically save config after editing server '{}': {}", name, e);
                 Ok(format!("Server '{}' configuration updated in memory, but {} to save automatically: {}. Run '{}' manually.",
                    style(name).green(),
                    style("failed").red(),
                    e,
                    style("save_config").yellow()
                ))
            }
        }
    }

    // --- Save Chat ---
    // Remove repl: &mut Repl, add state fields
    async fn cmd_save_chat(
        &mut self,
        chat_state: &mut Option<(String, crate::conversation_state::ConversationState)>,
        loaded_conversation: &mut Option<crate::conversation_state::ConversationState>,
        current_conversation_path: &mut Option<PathBuf>,
        args: &[String]
    ) -> Result<String> {
        // Access state fields directly via arguments
        let state_to_save = chat_state.as_ref().map(|(_, s)| s.clone())
            .or_else(|| loaded_conversation.clone());

        let state = match state_to_save {
            Some(s) => s,
            None => return Ok(style("No active or loaded conversation to save.").yellow().to_string()),
        };

        let filename = if args.is_empty() {
            // Generate default filename with timestamp
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            format!("chat_{}.json", timestamp)
        } else {
            let name = args[0].clone();
            if name.ends_with(".json") { name } else { format!("{}.json", name) }
        };

        // Calculate conversations dir (needs helper or config access)
        // Let's assume a helper function `get_conversations_dir_path()` exists or is added.
        // For now, hardcoding the logic here. Ideally, move this to a shared place.
        let conversations_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not determine config directory"))?
            .join("mcp/conversations");
        std::fs::create_dir_all(&conversations_dir)?; // Ensure it exists

        let path = conversations_dir.join(&filename);

        match state.save_to_json(&path).await {
            Ok(_) => {
                // Update the current conversation path via the argument
                *current_conversation_path = Some(path.clone());
                Ok(format!("Conversation saved to: {}", style(path.display()).green()))
            }
            Err(e) => Err(anyhow!("Failed to save conversation: {}", e)),
        }
    }

    // --- Load Chat ---
    // Remove repl: &mut Repl, add state fields
    async fn cmd_load_chat(
        &mut self,
        chat_state: &mut Option<(String, crate::conversation_state::ConversationState)>,
        loaded_conversation: &mut Option<crate::conversation_state::ConversationState>,
        current_conversation_path: &mut Option<PathBuf>,
        args: &[String]
    ) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: load_chat <filename>"));
        }
        let filename = args[0].clone();
        let filename_with_ext = if filename.ends_with(".json") { filename } else { format!("{}.json", filename) };

        // Calculate conversations dir
        let conversations_dir = dirs::config_dir()
            .ok_or_else(|| anyhow!("Could not determine config directory"))?
            .join("mcp/conversations");
        // No need to create dir on load

        let path = conversations_dir.join(&filename_with_ext);

        if !path.exists() {
            return Err(anyhow!("Conversation file not found: {}", path.display()));
        }

        match crate::conversation_state::ConversationState::load_from_json(&path).await {
            Ok(loaded_state) => {
                // Use arguments to update state
                *chat_state = None; // Clear active chat
                *loaded_conversation = Some(loaded_state); // Set loaded state
                *current_conversation_path = Some(path.clone()); // Update path
                Ok(format!("Conversation loaded from: {}", style(path.display()).green()))
            }
            Err(e) => Err(anyhow!("Failed to load conversation: {}", e)),
        }
    }

    // --- New Chat ---
    // Remove repl: &mut Repl, add state fields
    async fn cmd_new_chat(
        &mut self,
        chat_state: &mut Option<(String, crate::conversation_state::ConversationState)>,
        loaded_conversation: &mut Option<crate::conversation_state::ConversationState>,
        current_conversation_path: &mut Option<PathBuf>
    ) -> Result<String> {
        // Use arguments to update state
        *chat_state = None;
        *loaded_conversation = None;
        *current_conversation_path = None;
        Ok(style("Cleared current conversation.").yellow().to_string())
    }


    // --- Remove Server ---
    async fn cmd_remove_server(&mut self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: remove_server <server_name>"));
        }
        let name = &args[0];

        let removed = {
            let mut config_guard = self.host.config.lock().await;
            config_guard.servers.remove(name).is_some()
        };

        if removed {
             // Make save_config suggestion more prominent
             Ok(format!("Server '{}' removed from configuration. Run {} to make it persistent.",
                 style(name).yellow(),
                 style("save_config").yellow().bold() // Added bold
             ))
        } else {
            Err(anyhow!("Server '{}' not found in configuration.", name))
        }
    }

    // --- Save Config ---
    async fn cmd_save_config(&self) -> Result<String> {
        match self.host.save_host_config().await {
            Ok(_) => Ok(style("Configuration saved successfully.").green().to_string()),
            Err(e) => Err(anyhow!("Failed to save configuration: {}", e)),
        }
        // Potentially trigger reconfiguration here if needed immediately after save
        // self.host.reload_host_config().await?; // Or a more direct reconfigure
    }

    // --- Reload Config ---
    async fn cmd_reload_config(&mut self, editor: &mut Editor<ReplHelper, DefaultHistory>) -> Result<String> { // Added History type
         println!("{}", style("Warning: This will discard any unsaved configuration changes.").yellow());
         let confirm = self.prompt_for_input("Proceed? (yes/no):", editor)?; // Pass editor
         if confirm.trim().to_lowercase() != "yes" {
             return Ok(style("Reload cancelled.").yellow().to_string());
         }
        log::info!("Proceeding with configuration reload.");

        // Reload main config first
        log::debug!("Calling reload_host_config...");
        if let Err(e) = self.host.reload_host_config().await {
            error!("Failed to reload main configuration: {}", e);
            return Err(anyhow!("Failed to reload main configuration: {}", e));
        }
        info!("Main configuration reloaded successfully.");

        // Reload provider models config
        log::debug!("Calling reload_provider_models...");
        if let Err(e) = self.host.reload_provider_models().await {
             // Log warning but don't fail the whole reload if models file fails
             warn!("Failed to reload provider models configuration: {}", e);
             Ok(format!("{} Main configuration reloaded, but {} to reload provider models config.",
                style("Warning:").yellow(),
                style("failed").red()
             ))
        } else {
             info!("Provider models configuration reloaded successfully.");
             Ok(style("Configuration files reloaded successfully.").green().to_string())
        }
    }

     // --- Show Config ---
     async fn cmd_show_config(&self, args: &[String]) -> Result<String> {
        let config_guard = self.host.config.lock().await;

        if args.is_empty() {
            // Show all config
            let config_str = serde_json::to_string_pretty(&*config_guard)
                .map_err(|e| anyhow!("Failed to serialize config: {}", e))?;
            // Style the output slightly
            Ok(format!("{}\n{}", style("Current Configuration:").bold(), style(config_str).dim()))
        } else {
            // Show specific server config
            let server_name = &args[0];
            if let Some(server_config) = config_guard.servers.get(server_name) {
                let server_config_str = serde_json::to_string_pretty(server_config)
                    .map_err(|e| anyhow!("Failed to serialize server config: {}", e))?;
                Ok(format!("Configuration for server '{}':\n{}", style(server_name).green(), style(server_config_str).dim()))
            } else {
                Err(anyhow!("Server '{}' not found in configuration.", server_name))
            }
        }
    }


    // Helper for interactive input - takes editor as argument
    fn prompt_for_input(&self, prompt: &str, editor: &mut Editor<ReplHelper, DefaultHistory>) -> Result<String> { // Added History type
        // Use cyan for prompts
        let readline = editor.readline(&style(prompt).cyan().to_string());
        match readline {
            Ok(line) => Ok(line.trim().to_string()),
            Err(e) => Err(anyhow!("Failed to read input: {}", e)),
        }
    }

    // Helper for interactive input with initial text for editing - takes editor as argument
    fn prompt_with_initial(&self, prompt: &str, initial: &str, editor: &mut Editor<ReplHelper, DefaultHistory>) -> Result<String> { // Added History type
        // Use cyan for prompts
        let readline = editor.readline_with_initial(
            &style(prompt).cyan().to_string(),
            (initial, ""), // Provide initial text and empty cursor position hint
        );
        match readline {
            Ok(line) => Ok(line.trim().to_string()),
            Err(e) => Err(anyhow!("Failed to read input: {}", e)),
        }
    }


    /// List available servers
    pub async fn cmd_servers(&self) -> Result<String> {
        let servers_map = self.servers.lock().await;
        let servers: Vec<String> = servers_map.keys().cloned().collect();
        if servers.is_empty() {
            return Ok("No servers available".to_string());
        }

        let current = self.current_server.as_deref();
        let server_list = servers.iter()
            .map(|name| {
                if Some(name.as_str()) == current {
                    format!("{} {}", style("✔").green(), style(name).bold()) // Highlight current
                } else {
                    format!("  {}", name) // Indent non-current
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("Available servers:\n{}", server_list))
    }

    /// Set the current server
    pub async fn cmd_use(&mut self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            self.current_server = None;
            return Ok("Cleared current server selection".to_string());
        }

        let server_name = &args[0];
        // Check if server exists
        let servers_map = self.servers.lock().await;
        if servers_map.contains_key(server_name) {
            self.current_server = Some(server_name.clone());
            Ok(format!("Now using server '{}'", style(server_name).green())) // Style confirmation
        } else {
            Err(anyhow!("Server '{}' not found", server_name))
        }
    }

    /// List tools for a server
    pub async fn cmd_tools(&self, args: &[String]) -> Result<String> {
        let server_name = self.get_target_server_name(args)?;

        let servers_map = self.servers.lock().await;
        let server = servers_map.get(&server_name)
            .ok_or_else(|| anyhow!("Internal error: Server '{}' vanished", server_name))?;

        // Call list_tools directly on the Peer stored in ManagedServer
        let list_tools_result = server.client.list_tools(None).await?; // Pass None for default params
        let tools = list_tools_result.tools; // Extract Vec<Tool>

        if tools.is_empty() {
            return Ok(format!("No tools available on {}", style(&server_name).green()));
        }

        let tool_list = tools.iter()
            .map(|tool| {
                // Use .as_ref() on Cow to get &str
                let desc = tool.description.as_ref();
                // Style tool name yellow, description dimmed
                format!("  {} - {}", style(tool.name.as_ref()).yellow(), style(desc).dim())
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!("Tools on {}:\n{}", style(&server_name).green(), tool_list))
    }

    /// Call a tool
    pub async fn cmd_call(&self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            return Err(anyhow!("Usage: call <tool> [server] [json]"));
        }

        let tool_name = &args[0];

        // Determine server name and JSON args
        let (server_name, json_arg_opt) = self.parse_call_args(args)?;
        let args_value: Value = match json_arg_opt {
            Some(json_str) => serde_json::from_str(&json_str)
                           .map_err(|e| anyhow!("Invalid JSON: {}", e))?,
            None => serde_json::json!({}), // Default to empty object
        };

        // Lock map, get server, call tool
        // Lock map, get server
        let servers_map = self.servers.lock().await;
        let server = servers_map.get(&server_name)
            .ok_or_else(|| anyhow!("Internal error: Server '{}' vanished", server_name))?;

        // Call tool with progress indicator
        let progress_msg = format!("Calling tool '{}' on server '{}'...", style(tool_name).yellow(), style(&server_name).green());

        // Prepare parameters for the Peer's call_tool method
        let arguments_map = match args_value {
            Value::Object(map) => Some(map),
            Value::Null => None,
            _ => return Err(anyhow!("Tool arguments must be a JSON object or null")),
        };
        let params = rmcp::model::CallToolRequestParam { // Use rmcp type directly
            name: tool_name.to_string().into(),
            arguments: arguments_map,
        };

        // Call call_tool directly on the Peer stored in ManagedServer
        let result = crate::repl::with_progress(
            progress_msg,
            server.client.call_tool(params) // Pass the prepared params
        ).await?;

        // Format result
        let mut raw_output = if result.is_error.unwrap_or(false) {
            format!("{} Tool '{}' on server '{}' returned an error:\n", style("Error:").red(), style(tool_name).yellow(), style(&server_name).green())
        } else {
            format!("{} Result from tool '{}' on server '{}':\n", style("Success:").green(), style(tool_name).yellow(), style(&server_name).green())
        };

        // Use the shared formatter which handles different Content types
        // Ensure format_tool_result is public or move it
        raw_output.push_str(&crate::host::server_manager::format_tool_result(&result));

        // Truncate the output before returning
        Ok(crate::repl::truncate_lines(&raw_output, 150))
    }

    /// Show or set the active AI provider
    async fn cmd_provider(&self, args: &[String]) -> Result<String> {
        if args.is_empty() {
            // Show current provider
            match self.host.get_active_provider_name().await {
                Some(name) => Ok(format!("Current AI provider: {}", style(name).cyan())),
                None => Ok("No AI provider is currently active.".to_string()),
            }
        } else {
            // Set provider
            let provider_name = &args[0];
            match self.host.set_active_provider(provider_name).await {
                Ok(_) => Ok(format!("AI provider set to: {}", style(provider_name).cyan())),
                Err(e) => Err(anyhow!("Failed to set provider: {}", e)),
            }
        }
    }

    /// List available AI providers
    async fn cmd_providers(&self) -> Result<String> {
        let providers = self.host.list_available_providers().await;
        if providers.is_empty() {
            Ok("No AI providers available (check API key environment variables)".to_string())
        } else {
            let current_provider = self.host.get_active_provider_name().await;
            let provider_list = providers.iter()
                .map(|name| {
                    if Some(name) == current_provider.as_ref() {
                        format!("{} {}", style("✔").green(), style(name).cyan().bold()) // Highlight current
                    } else {
                        format!("  {}", style(name).cyan()) // Indent non-current
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(format!("Available AI providers:\n{}", provider_list))
        }
    }

    /// Show or set the active AI model for the current provider
    async fn cmd_model(&self, args: &[String]) -> Result<String> {
        let active_provider_opt = self.host.get_active_provider_name().await;

        if args.is_empty() {
            // --- Show current model and suggestions ---
            let active_provider = match active_provider_opt {
                Some(name) => name,
                 // Suggest command if no provider is active
                 None => return Ok(format!("No AI provider is currently active. Use {} first.", style("provider <name>").yellow())),
            };
            log::debug!("Showing model info for active provider: {}", active_provider);

            let current_model_opt = match self.host.ai_client().await {
                Some(client) => Some(client.model_name()),
                None => None,
            };
            log::debug!("Current model: {:?}", current_model_opt);

            let suggestions = { // Scope lock
                log::debug!("Acquiring provider_models lock to get suggestions...");
                let models_config_guard = self.host.provider_models.lock().await; // Lock the models config
                log::debug!("Provider_models lock acquired.");
                let provider_key = active_provider.to_lowercase(); // Use lowercase key
                 // --- Add detailed logging ---
                 let available_keys: Vec<_> = models_config_guard.providers.keys().cloned().collect();
                 log::debug!(
                     "cmd_model Display: Looking for key '{}'. Available keys: {:?}",
                     provider_key,
                     available_keys
                 );
                 // --- End detailed logging ---
                let models = models_config_guard.providers // Access inner HashMap
                    .get(&provider_key) // Use lowercase key
                    .map(|list| list.models.clone()) // Clone Vec<String>
                    .unwrap_or_default(); // Return empty Vec if not found
                log::debug!("Found {} suggestions for provider key '{}'", models.len(), provider_key); // Log count
                models
            }; // Lock released here
            log::debug!("Provider_models lock released.");


            let mut output = format!(
                "Active provider: {}\n",
                style(&active_provider).cyan()
            );

            match current_model_opt {
                Some(ref current_model) => { // Use ref here
                    output.push_str(&format!(
                        "Current model:   {}\n",
                        style(current_model).green()
                    ));

                    if !suggestions.is_empty() {
                        output.push_str(&format!("\n{}", style("Suggested models (from config):").dim()));
                        for suggestion in suggestions {
                            if &suggestion == current_model { // Compare suggestion with current_model ref
                                // Highlight current model if it's in suggestions
                                output.push_str(&format!("\n  {} {}", style("✔").green(), style(suggestion).green()));
                            } else {
                                output.push_str(&format!("\n  - {}", suggestion));
                            }
                        }
                        output.push('\n'); // Add newline after list
                    } else {
                        output.push_str(&format!("\n{}", style(format!("No suggested models found in config for '{}'.", active_provider)).dim()));
                    }
                    output.push_str(&format!("\nUse '{}' to change model.", style(format!("model <name>")).yellow()));

                }
                None => {
                    // Should ideally not happen if provider is active, but handle defensively
                    output.push_str("No model currently active for this provider.\n");
                     if !suggestions.is_empty() {
                        output.push_str(&format!("\n{}", style("Suggested models (from config):").dim()));
                        for suggestion in suggestions {
                             output.push_str(&format!("\n  - {}", suggestion));
                        }
                        output.push('\n'); // Add newline after list
                     }
                     output.push_str(&format!("\nUse '{}' to set a model.", style(format!("model <name>")).yellow()));
                }
            }
            Ok(output)

        } else {
            // --- Set model (existing logic) ---
            let provider_name = match active_provider_opt {
                 Some(name) => name,
                 // Suggest command if no provider is active
                 None => return Err(anyhow!("No active AI provider. Use {} first.", style("provider <name>").yellow())),
            };
            let model_name = &args[0];
            log::info!("Attempting to set model to '{}' for provider '{}'", model_name, provider_name);
            match self.host.set_active_model(&provider_name, model_name).await {
                Ok(_) => Ok(format!(
                    "Model for provider '{}' set to: {}",
                    style(&provider_name).cyan(), // Use provider_name here
                    style(model_name).green()
                )),
                Err(e) => Err(anyhow!("Failed to set model: {}", e)),
            }
        }
    }

    // Helper methods

    /// Helper to get the server name to target
    fn get_target_server_name(&self, args: &[String]) -> Result<String> {
        if !args.is_empty() {
            // Explicit server name provided
            Ok(args[0].clone())
        } else {
            // Use current server if set
            self.current_server.clone()
                .ok_or_else(|| anyhow!("No server specified and no current server selected. Use {} first.", style("use <server_name>").yellow()))
        }
    }

    /// Helper to parse arguments for the 'call' command
    fn parse_call_args(&self, args: &[String]) -> Result<(String, Option<String>)> {
        // args[0] is the tool name

        // Determine server name (arg[1] unless it's JSON) or use current
        let (server_name, json_arg_index) = if args.len() > 1 && !args[1].starts_with('{') {
            (args[1].clone(), 2) // Server specified in arg[1], JSON might be in arg[2]
        } else {
            // Server not specified or arg[1] is JSON, use current
            let current = self.current_server.clone().ok_or_else(|| {
                 anyhow!("No server specified for tool call and no current server selected. Use {} first.", style("use <server_name>").yellow())
            })?;
            (current, 1) // JSON might be in arg[1]
        };

        // Extract JSON argument if present
        let json_arg = if args.len() > json_arg_index {
            Some(args[json_arg_index].clone())
        } else {
            None
        };

        Ok((server_name, json_arg))
    }
    
    // Public methods for server management
    
    pub fn current_server_name(&self) -> Option<&str> {
        self.current_server.as_deref()
    }
    
    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }
    
    pub async fn close(&self) -> Result<()> {
        // Nothing to close now, since we don't own the servers
        Ok(())
    }
}
