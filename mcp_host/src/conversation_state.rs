use shared_protocol_objects::{Role, ToolInfo};
use console::style;
use serde_json;

pub fn format_json_output(json_str: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
        format!("```json\n{}\n```", serde_json::to_string_pretty(&value).unwrap_or(json_str.to_string()))
    } else {
        json_str.to_string()
    }
}

fn format_markdown(text: &str) -> String {
    let parts: Vec<&str> = text.split("```").collect();
    let mut formatted = String::new();
    
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            let lines: Vec<&str> = part.lines().collect();
            for line in lines {
                if line.starts_with("# ") {
                    formatted.push_str(&format!("{}\n", style(line).cyan().bold()));
                } else if line.starts_with("## ") {
                    formatted.push_str(&format!("{}\n", style(line).blue().bold()));
                } else if line.starts_with("> ") {
                    formatted.push_str(&format!("{}\n", style(line).italic()));
                } else if line.starts_with("- ") || line.starts_with("* ") {
                    formatted.push_str(&format!("  {} {}\n", style("•").cyan(), &line[2..]));
                } else {
                    formatted.push_str(&format!("{}\n", line));
                }
            }
        } else {
            if part.trim().starts_with('{') || part.trim().starts_with('[') {
                formatted.push_str(&format_json_output(part));
            } else {
                formatted.push_str(&format!("```{}\n```", part));
            }
        }
    }
    formatted
}

pub fn format_tool_response(tool_name: &str, response: &str) -> String {
    let mut output = String::new();
    output.push_str(&format!("{}\n", style("Tool Response:").green().bold()));
    output.push_str(&format!("└─ {}\n", style(tool_name).yellow()));
    
    if response.trim().starts_with('{') || response.trim().starts_with('[') {
        output.push_str(&format_json_output(response));
    } else {
        output.push_str(&format_markdown(response));
    }
    output
}

pub fn format_chat_message(role: &Role, content: &str) -> String {
    let role_style = match role {
        Role::System => style("System").blue().bold(),
        Role::User => style("User").magenta().bold(),
        Role::Assistant => style("Assistant").cyan().bold(),
    };
    
    format!("{}: {}\n", role_style, format_markdown(content))
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub system_prompt: String,
    pub tools: Vec<ToolInfo>,
}

impl ConversationState {
    pub fn new(system_prompt: String, tools: Vec<ToolInfo>) -> Self {
        let mut state = Self {
            messages: Vec::new(),
            system_prompt: system_prompt.clone(),
            tools: tools.clone(),
        };

        // Generate a combined system prompt including the smiley tool format instructions
        let combined_prompt = format!(
            "{}\n\n{}", 
            system_prompt,
            crate::conversation_service::generate_smiley_tool_system_prompt(&tools)
        );

        // Add the combined system prompt as the first system message
        state.add_system_message(&combined_prompt);
        state
    }

    pub fn add_system_message(&mut self, content: &str) {
        self.messages.push(Message {
            role: Role::System,
            content: content.to_string(),
        });
    }

    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(Message {
            role: Role::User,
            content: content.to_string(),
        });
    }

    pub fn add_assistant_message(&mut self, content: &str) {
        self.messages.push(Message {
            role: Role::Assistant,
            content: content.to_string(),
        });
    }
}
