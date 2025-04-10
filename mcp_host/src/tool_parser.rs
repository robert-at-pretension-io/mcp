use anyhow::{Result, anyhow};
use serde_json::Value;
use log;

/// Extracts tool calls from AI responses using text delimiter pattern
pub struct ToolParser; // Renamed struct

impl ToolParser {
    /// Parse all tool calls from a response using the text delimiter pattern.
    /// Returns a tuple: (Vec<ValidToolCalls>, Option<FirstInvalidAttemptContent>)
    pub fn parse_tool_calls(response: &str) -> (Vec<ToolCall>, Option<String>) {
        // Define the start and end delimiters
        let start_delimiter = "<<<TOOL_CALL>>>";
        let end_delimiter = "<<<END_TOOL_CALL>>>";
        let mut tool_calls = Vec::new();
        let mut first_invalid_content: Option<String> = None; // Track first invalid attempt
        let mut start_pos = 0;

        // Find all instances of delimited tool calls
        while let Some(start_idx) = response[start_pos..].find(start_delimiter) {
            let real_start = start_pos + start_idx;
            if let Some(end_idx) = response[real_start + start_delimiter.len()..].find(end_delimiter) {
                let real_end = real_start + start_delimiter.len() + end_idx;

                // Extract content between delimiters
                let content_start = real_start + start_delimiter.len();
                let json_content = response[content_start..real_end].trim();
                
                // Parse JSON
                match serde_json::from_str::<Value>(json_content) {
                    Ok(json) => {
                        match Self::validate_tool_call(&json) {
                            Ok(tool_call) => {
                                log::debug!("Successfully parsed delimited tool call: {}", tool_call.name); // Updated log
                                tool_calls.push(tool_call);
                            },
                            Err(e) => {
                                log::debug!("Found JSON but invalid tool call format: {}", e);
                            }
                        }
                    },
                    Err(e) => {
                        log::debug!("Found delimiters but content is not valid JSON: {}", e); // Updated log
                    }
                }

                start_pos = real_end + end_delimiter.len(); // Use end delimiter length
            } else {
                // No closing delimiter found
                break;
            }
        }

        (tool_calls, first_invalid_content) // Return tuple
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

<<<TOOL_CALL>>>
{
  "name": "search",
  "arguments": {
    "query": "rust programming"
  }
}
<<<END_TOOL_CALL>>>

Let me know if you need anything else."#;

        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 1);
        assert!(invalid_content.is_none());
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(tool_calls[0].arguments["query"], "rust programming");
    }

    #[test]
    fn test_parse_multiple_tool_calls() {
        let response = r#"I'll execute these tools for you.

<<<TOOL_CALL>>>
{
  "name": "search",
  "arguments": {
    "query": "weather"
  }
}
<<<END_TOOL_CALL>>>

Some text in between.

<<<TOOL_CALL>>>
{
  "name": "calculator",
  "arguments": {
    "expression": "5 * 9"
  }
}
<<<END_TOOL_CALL>>>
"#;

        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 2);
        assert!(invalid_content.is_none());
        assert_eq!(tool_calls[0].name, "search");
        assert_eq!(tool_calls[1].name, "calculator");
    }

    #[test]
    fn test_no_tool_calls() {
        let response = "I don't have any tool calls to make right now.";
        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
        assert!(invalid_content.is_none());
    }

    #[test]
    fn test_invalid_json() {
        let response = r#"
<<<TOOL_CALL>>>
{
  "name": "search",
  "arguments": {
    "query": "weather"
  },
  invalid json here
}
<<<END_TOOL_CALL>>>
"#;

        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
        assert!(invalid_content.is_some());
        assert!(invalid_content.unwrap().contains("invalid json here"));
    }

    #[test]
    fn test_missing_fields() {
        let response = r#"
<<<TOOL_CALL>>>
{
  "name": "search"
}
<<<END_TOOL_CALL>>>
"#;

        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 0);
        assert!(invalid_content.is_some());
        assert!(invalid_content.unwrap().contains("\"name\": \"search\"")); // Contains the partial JSON
    }

    #[test]
    fn test_mixed_valid_invalid() {
        let response = r#"
<<<TOOL_CALL>>>
{ "name": "valid_tool", "arguments": {} }
<<<END_TOOL_CALL>>>
Some text.
<<<TOOL_CALL>>>
{ "invalid": "json" }
<<<END_TOOL_CALL>>>
More text.
<<<TOOL_CALL>>>
{ "name": "another_valid", "arguments": {"key": "value"} }
<<<END_TOOL_CALL>>>
"#;
        let (tool_calls, invalid_content) = ToolParser::parse_tool_calls(response);
        assert_eq!(tool_calls.len(), 2); // Should find both valid calls
        assert_eq!(tool_calls[0].name, "valid_tool");
        assert_eq!(tool_calls[1].name, "another_valid");
        assert!(invalid_content.is_some()); // Should capture the first invalid attempt
        assert!(invalid_content.unwrap().contains("\"invalid\": \"json\""));
    }
}
