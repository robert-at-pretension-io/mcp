use rustyline::completion::{Completer, Pair};
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::Context;
use std::borrow::Cow;

// Use rmcp's Tool type directly
use rmcp::model::Tool as ToolInfo;

/// Helper for rustyline with command completion
pub struct ReplHelper {
    pub commands: Vec<String>,
    pub server_names: Vec<String>,
    pub current_tools: Vec<ToolInfo>,
    pub available_providers: Vec<String>,
    pub current_provider_models: Vec<String>, // Added: Suggested models for current provider
    highlighter: MatchingBracketHighlighter,
}

// Implement Default manually since MatchingBracketHighlighter doesn't implement Default
impl Default for ReplHelper {
    fn default() -> Self {
        Self::new() // Use the existing new() method for default values
    }
}

// Manual implementation of Clone since MatchingBracketHighlighter doesn't implement it
impl Clone for ReplHelper {
    fn clone(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            server_names: self.server_names.clone(),
            current_tools: self.current_tools.clone(),
            available_providers: self.available_providers.clone(),
            current_provider_models: self.current_provider_models.clone(), // Clone models
            highlighter: MatchingBracketHighlighter::new(),
        }
    }
}

impl ReplHelper {
    pub fn new() -> Self {
        Self {
            commands: vec![
                "help".to_string(),
                "servers".to_string(),
                "use".to_string(),
                "tools".to_string(),
                "call".to_string(),
                "chat".to_string(),
                "provider".to_string(),
                "providers".to_string(),
                "model".to_string(),
                "add_server".to_string(),
                "edit_server".to_string(),
                "remove_server".to_string(),
                "save_config".to_string(),
                "reload_config".to_string(),
                "show_config".to_string(),
                "exit".to_string(),
                "quit".to_string(),
            ],
            server_names: Vec::new(),
            current_tools: Vec::new(),
            available_providers: Vec::new(),
            current_provider_models: Vec::new(), // Initialize empty models
            highlighter: MatchingBracketHighlighter::new(),
        }
    }

    pub fn update_server_names(&mut self, names: Vec<String>) {
        self.server_names = names;
    }

    // Method to update the list of tools for the current server
    pub fn update_current_tools(&mut self, tools: Vec<ToolInfo>) {
        self.current_tools = tools;
    }

    // Method to update the list of available AI providers
    pub fn update_available_providers(&mut self, providers: Vec<String>) {
        self.available_providers = providers;
    }

    // Method to update the list of suggested models for the current provider
    pub fn update_current_provider_models(&mut self, models: Vec<String>) {
        self.current_provider_models = models;
    }
}

impl Completer for ReplHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let line_parts: Vec<&str> = line[..pos].split_whitespace().collect();

        if line_parts.is_empty() {
            // Return empty list if at beginning of line
            return Ok((0, Vec::new()));
        }

        if line_parts.len() == 1 {
            // Completing the first word (command)
            let word = line_parts[0];
            let start = line.find(word).unwrap_or(0);

            let matches: Vec<Pair> = self.commands.iter()
                .filter(|cmd| cmd.starts_with(word))
                .map(|cmd| Pair { display: cmd.clone(), replacement: cmd.clone() })
                .collect();

            return Ok((start, matches));
        } else if line_parts.len() == 2 {
            let command = line_parts[0];
            let word = line_parts[1];
            let start = line.rfind(word).unwrap_or(pos);

            if command == "use" || command == "tools" || command == "chat" {
                // Complete server names for 'use', 'tools', 'chat'
                let matches: Vec<Pair> = self.server_names.iter()
                    .filter(|name| name.starts_with(word))
                    .map(|name| Pair { display: name.clone(), replacement: name.clone() })
                    .collect();
                return Ok((start, matches));
            } else if command == "call" {
                // Complete tool names for 'call' command using current_tools
                let matches: Vec<Pair> = self.current_tools.iter()
                    .filter(|tool| tool.name.starts_with(word))
                    // Convert Cow to String for Pair
                    .map(|tool| Pair { display: tool.name.to_string(), replacement: tool.name.to_string() })
                    .collect();
                return Ok((start, matches));
            } else if command == "provider" {
                 // Complete provider names for 'provider' command
                 let matches: Vec<Pair> = self.available_providers.iter()
                     .filter(|name| name.starts_with(word))
                     .map(|name| Pair { display: name.clone(), replacement: name.clone() })
                     .collect();
                 return Ok((start, matches));
            } else if command == "model" { // Add completion for model command
                 let matches: Vec<Pair> = self.current_provider_models.iter()
                     .filter(|name| name.starts_with(word))
                     .map(|name| Pair { display: name.clone(), replacement: name.clone() })
                     .collect();
                 return Ok((start, matches));
            }
        } else if line_parts.len() == 3 && line_parts[0] == "call" {
             // Complete server names after the tool name for 'call' command
             let word = line_parts[2];
             let start = line.rfind(word).unwrap_or(pos);

             let matches: Vec<Pair> = self.server_names.iter()
                 .filter(|name| name.starts_with(word))
                 .map(|name| Pair { display: name.clone(), replacement: name.clone() })
                 .collect();
             return Ok((start, matches));
        }

        Ok((pos, Vec::new())) // No completion otherwise
    }
}

impl Hinter for ReplHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        if line.is_empty() || pos < line.len() {
            return None;
        }

        let line_parts: Vec<&str> = line.split_whitespace().collect();

        if line_parts.is_empty() {
            return None;
        }

        // Use more descriptive placeholders
        match line_parts[0] {
            "use" if line_parts.len() == 1 => Some(" [server_name]".to_string()),
            "tools" if line_parts.len() == 1 => Some(" [server_name]".to_string()),
            "call" if line_parts.len() == 1 => Some(" <tool_name> [server_name] [json_args]".to_string()),
            "chat" if line_parts.len() == 1 => Some(" <server_name>".to_string()),
            "provider" if line_parts.len() == 1 => Some(" [provider_name]".to_string()), // Added hint
            "model" if line_parts.len() == 1 => Some(" [model_name]".to_string()), // Added hint
            "edit_server" if line_parts.len() == 1 => Some(" <server_name>".to_string()),
            "remove_server" if line_parts.len() == 1 => Some(" <server_name>".to_string()),
            "show_config" if line_parts.len() == 1 => Some(" [server_name]".to_string()),
            _ => None,
        }
    }
}

impl Highlighter for ReplHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        self.highlighter.highlight(line, pos)
    }

    fn highlight_char(&self, line: &str, pos: usize) -> bool {
        self.highlighter.highlight_char(line, pos)
    }
}

impl Validator for ReplHelper {}

// Simply implement the Helper trait with no associated types
// This is a version-compatible implementation
impl rustyline::Helper for ReplHelper {}

#[cfg(test)]
mod tests {
    use super::*;
    use rustyline::history::DefaultHistory;
    
    #[test]
    fn test_command_completion() {
        let helper = ReplHelper::new();
        let history = DefaultHistory::new();
        
        // Create context with history
        let ctx = Context::new(&history);
        
        // Test command completion
        let (pos, cmds) = helper.complete("h", 1, &ctx).unwrap();
        assert_eq!(pos, 0);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].display, "help");
        
        // Test server completion
        let mut helper = ReplHelper::new();
        helper.update_server_names(vec!["server1".to_string(), "server2".to_string()]);
        
        let (pos, servers) = helper.complete("use s", 5, &ctx).unwrap();
        assert_eq!(pos, 4);
        assert_eq!(servers.len(), 2);
        assert!(servers.iter().any(|p| p.display == "server1"));
        assert!(servers.iter().any(|p| p.display == "server2"));
        
        // Test hint
        let hint = helper.hint("use", 3, &ctx).unwrap();
        assert_eq!(hint, " [server]");
    }
}
