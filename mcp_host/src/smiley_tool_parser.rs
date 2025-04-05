use anyhow::{Result, anyhow};
use serde_json::Value;
use log;

/// Extracts tool calls from AI responses using smiley delimiter pattern
pub struct SmileyToolParser;

impl SmileyToolParser {
    /// Parse all tool calls from a response using the smiley delimiter pattern
    pub fn parse_tool_calls(response: &str) -> Vec<ToolCall> {
        // Define the exact smiley pattern - must be exactly 14 smileys
        let smiley_pattern = "😊😊😊😊😊😊😊😊😊😊😊😊😊😊";
        let mut tool_calls = Vec::new();
        let mut start_pos = 0;
        
        // Find all instances of smiley-delimited tool calls
        while let Some(start_idx) = response[start_pos..].find(smiley_pattern) {
            let real_start = start_pos + start_idx;
            if let Some(end_idx) = response[real_start + smiley_pattern.len()..].find(smiley_pattern) {
                let real_end = real_start + smiley_pattern.len() + end_idx;
                
                // Extract content between delimiters
                let content_start = real_start + smiley_pattern.len();
                let json_content = response[content_start..real_end].trim();
                
                // Parse JSON
                match serde_json::from_str::<Value>(json_content) {
                    Ok(json) => {
                        match Self::validate_tool_call(&json) {
                            Ok(tool_call) => {
                                log::debug!("Successfully parsed smiley-delimited tool call: {}", tool_call.name);
                                tool_calls.push(tool_call);
                            },
                            Err(e) => {
                                log::debug!("Found JSON but invalid tool call format: {}", e);
                            }
                        }
                    },
                    Err(e) => {
                        log::debug!("Found smiley delimiters but content is not valid JSON: {}", e);
                    }
                }
                
                start_pos = real_end + smiley_pattern.len();
            } else {
                // No closing delimiter found
                break;
            }
        }
        
        tool_calls
    }
    
    /// Validate that a JSON object has the required fields for a tool call
    fn validate_tool_call(json: &Value) -> Result<ToolCall> {
        // Get the name field
        let name = json.get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow!("Tool call missing 'name' field"))?;
            
        // Get the arguments field
        let arguments = json.get("arguments")
            .ok_or_else(|| anyhow!("Tool call missing 'arguments' field"))?;
            
        Ok(ToolCall {
            name: name.to_string(),
            arguments: arguments.clone(),
        })
    }
}

/// Represents a parsed tool call with name and arguments
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_single_tool_call() {
        let response = r#"I'll help you with that.

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "search",
  "arguments": {
    "query": "rust programming"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

Let me know if you need anything else."#;

        let tool_calls = SmileyToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(tool_calls[0].arguments["query"], "rust programming");
    }
    
    #[test]
    fn test_parse_multiple_tool_calls() {
        let response = r#"I'll execute these tools for you.

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "search",
  "arguments": {
    "query": "weather"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊

😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "calculator",
  "arguments": {
    "expression": "5 * 9"
  }
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
"#;

        let tool_calls = SmileyToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(tool_calls[1].name, "calculator");
    }
    
    #[test]
    fn test_no_tool_calls() {
        let response = "I don't have any tool calls to make right now.";
        let tool_calls = SmileyToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
    }
    
    #[test]
    fn test_invalid_json() {
        let response = r#"
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "search",
  "arguments": {
    "query": "weather"
  },
  invalid json here
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
"#;

        let tool_calls = SmileyToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
    }
    
    #[test]
    fn test_missing_fields() {
        let response = r#"
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
{
  "name": "search"
}
😊😊😊😊😊😊😊😊😊😊😊😊😊😊
"#;

        let tool_calls = SmileyToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
    }
}