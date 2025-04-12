use rmcp::model::{PromptMessageRole as Role, Tool as ToolInfo};
use console::style;
use serde_json;

/// Formats JSON nicely within a code block.
pub fn format_json_output(json_str: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json_str) {
        // Dim the JSON content slightly
        format!("```json\n{}\n```", style(serde_json::to_string_pretty(&value).unwrap_or_else(|_| json_str.to_string())).dim())
    } else {
        // If not valid JSON, just return dimmed
        style(json_str).dim().to_string()
    }
}

/// Applies basic markdown styling with subtle colors.
fn format_markdown(text: &str) -> String {
    let parts: Vec<&str> = text.split("```").collect();
    let mut formatted = String::new();

    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 0 {
            // Process normal text lines
            let lines: Vec<&str> = part.lines().collect();
            for line in lines {
                if line.starts_with("# ") {
                    formatted.push_str(&format!("{}\n", style(line).cyan().bold()));
                } else if line.starts_with("## ") {
                    formatted.push_str(&format!("{}\n", style(line).blue().bold()));
                } else if line.starts_with("> ") {
                    formatted.push_str(&format!("{}\n", style(line).italic().dim())); // Dim quotes
                } else if line.starts_with("- ") || line.starts_with("* ") {
                    formatted.push_str(&format!("  {} {}\n", style("•").cyan(), style(&line[2..]).dim())); // Dim list items
                } else {
                    // Dim regular text lines
                    formatted.push_str(&format!("{}\n", style(line).dim()));
                }
            }
        } else {
            // Process code blocks
            if part.trim().starts_with('{') || part.trim().starts_with('[') {
                // Format JSON within the code block
                formatted.push_str(&format_json_output(part));
            } else {
                // Format other code blocks, dimmed
                formatted.push_str(&format!("```{}\n```", style(part).dim()));
            }
        }
    }
    // Trim trailing newline if added unnecessarily
    formatted.trim_end().to_string()
}

/// Formats the output of a tool call.
pub fn format_tool_response(tool_name: &str, response: &str) -> String {
    let mut output = String::new();
    // Style the label blue and bold, tool name yellow
    output.push_str(&format!("{}\n", style("Tool Response:").blue().bold()));
    output.push_str(&format!("└─ {}\n", style(tool_name).yellow()));

    // Format the response content (JSON or markdown), which will be dimmed
    if response.trim().starts_with('{') || response.trim().starts_with('[') {
        output.push_str(&format_json_output(response));
    } else {
        output.push_str(&format_markdown(response));
    }
    output
}

/// Formats a chat message with role styling and dimmed content.
pub fn format_chat_message(role: &Role, content: &str) -> String {
    let role_style = match role {
        Role::System => style("System").blue().bold(),
        Role::User => style("User").magenta().bold(),
        Role::Assistant => style("Assistant").cyan().bold(),
        _ => style("Unknown").red().bold(), // Handle any new role types
    };

    // Apply markdown formatting (which includes dimming) to the content
    format!("{}: {}", role_style, format_markdown(content))
}

/// Formats the raw assistant response, highlighting tool call sections.
pub fn format_assistant_response_with_tool_calls(raw_response: &str) -> String {
    let mut formatted_output = String::new();
    let mut current_pos = 0;
    let start_delimiter = "<<<TOOL_CALL>>>";
    let end_delimiter = "<<<END_TOOL_CALL>>>";

    while let Some(start_index) = raw_response[current_pos..].find(start_delimiter) {
        let absolute_start_index = current_pos + start_index;

        // Append the text before the tool call (dimmed)
        formatted_output.push_str(&format_markdown(&raw_response[current_pos..absolute_start_index]));

        // Find the end delimiter after the start delimiter
        if let Some(end_index) = raw_response[absolute_start_index..].find(end_delimiter) {
            let absolute_end_index = absolute_start_index + end_index + end_delimiter.len();

            // Append the highlighted tool call section
            let tool_call_part = &raw_response[absolute_start_index..absolute_end_index];
            // Style the tool call block - yellow and italic
            formatted_output.push_str(&style(tool_call_part).yellow().italic().to_string());

            // Update current position
            current_pos = absolute_end_index;
        } else {
            // If no end delimiter found, append the rest of the string normally (shouldn't happen with valid LLM output)
            formatted_output.push_str(&format_markdown(&raw_response[absolute_start_index..]));
            current_pos = raw_response.len();
            break;
        }
    }

    // Append any remaining text after the last tool call (dimmed)
    if current_pos < raw_response.len() {
        formatted_output.push_str(&format_markdown(&raw_response[current_pos..]));
    }

    // Add the Assistant role prefix
    format!("{}: {}", style("Assistant").cyan().bold(), formatted_output)
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

        // Generate a combined system prompt including the tool format instructions
        let combined_prompt = format!(
            "{}\n\n{}",
            system_prompt,
            crate::conversation_service::generate_tool_system_prompt(&tools) // Use new function name
        );

        // Add the combined system prompt as the first system message
        state.add_system_message(&combined_prompt);
        // REMOVED: state.add_user_message(&combined_prompt); // Do not add duplicate user message
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
