use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use shared_protocol_objects::{error_response, success_response, CallToolParams, CallToolResult, JsonRpcResponse, ToolResponseContent, INTERNAL_ERROR};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct RegexReplaceParams {
    pub file_path: String,
    pub pattern: String,
    pub replacement: String,
}

pub async fn handle_regex_replace_tool_call(params: CallToolParams, id: Option<Value>) -> Result<JsonRpcResponse> {
    let args: RegexReplaceParams = serde_json::from_value(params.arguments)
        .map_err(|e| anyhow!("Invalid arguments: {}", e))?;

    if !Path::new(&args.file_path).exists() {
        return Ok(error_response(id, INTERNAL_ERROR, "File not found"));
    }

    let content = fs::read_to_string(&args.file_path)?;
    let re = Regex::new(&args.pattern)
        .map_err(|e| anyhow!("Invalid regex pattern: {}", e))?;

    let matches: Vec<_> = re.find_iter(&content).collect();

    if matches.len() == 1 {
        let new_content = re.replace(&content, &args.replacement).to_string();
        fs::write(&args.file_path, new_content)?;
        let tool_res = CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".into(),
                text: "Replacement successful".to_string(),
                annotations: None,
            }],
            is_error: Some(false),
            _meta: None,
            progress: None,
            total: None,
        };
        Ok(success_response(id, serde_json::to_value(tool_res)?))
    } else {
        let msg = if matches.is_empty() {
            "No matches found, no changes made.".to_string()
        } else {
            format!("Found {} matches instead of exactly one, no changes made.", matches.len())
        };
        let tool_res = CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".into(),
                text: msg,
                annotations: None,
            }],
            is_error: Some(true),
            _meta: None,
            progress: None,
            total: None,
        };
        Ok(success_response(id, serde_json::to_value(tool_res)?))
    }
}

pub fn regex_replace_tool_info() -> shared_protocol_objects::ToolInfo {
    shared_protocol_objects::ToolInfo {
        name: "regex_replace".to_string(),
        description: Some(
            "A precise tool for making targeted text replacements in files using regular expressions.

            **When to Use:**
            - When you need to modify specific text patterns in configuration files
            - To update version numbers, dates, or other formatted strings
            - For changing function names, variable names, or code patterns
            - To fix formatting issues or standardize text formats
            - When updating URLs, paths, or other structured text
            
            **Input:**
            - file_path: The exact path to the file that needs modification
            - pattern: A regex pattern that uniquely identifies the text to change
            - replacement: The new text to insert in place of the match
            
            **Output:**
            - Success message if exactly one match was found and replaced
            - Error message if no matches or multiple matches were found
            - Details about what was changed or why the operation failed
            
            **Usage Constraints:**
            - CRITICAL: Tool will only proceed if exactly ONE match is found
            - Pattern must be specific enough to match only the intended text
            - Always verify the pattern will match the intended text only
            - Do not use for bulk replacements or multiple matches
            - Avoid patterns that could match unintended text
            - Test patterns carefully before applying replacements
            
            **Best Practices:**
            1. First use the pattern to verify exactly one match exists
            2. Double-check the replacement text is correctly formatted
            3. Consider the context around the text being replaced
            4. Keep replacements minimal and targeted
            5. Use specific patterns rather than broad ones
            "
            .to_string()
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the target file."
                },
                "pattern": {
                    "type": "string",
                    "description": "The regex pattern to search for."
                },
                "replacement": {
                    "type": "string",
                    "description": "The text to replace the matched pattern with."
                }
            },
            "required": ["file_path", "pattern", "replacement"],
            "additionalProperties": false
        }),
    }
}
