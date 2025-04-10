use anyhow::{anyhow, Result};
use anyhow::{anyhow, Result};
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use shared_protocol_objects::{error_response, success_response, CallToolParams, CallToolResult, JsonRpcResponse, ToolResponseContent, INTERNAL_ERROR};
use std::fs;
use std::io::{Read}; // Removed unused Write
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct RegexReplaceParams {
    pub file_path: String,
    pub start_pattern: String,
    pub end_pattern: String,
    pub replacement: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub match_occurrence: Option<u32>,
    #[serde(default)]
    pub match_all: bool,
    #[serde(default)]
    pub create_backup: bool,
    #[serde(default)]
    pub context_lines: Option<u32>,
}

/// Attempts to validate if a regex pattern string has common issues
fn validate_regex_pattern(pattern: &str) -> Result<(), String> {
    // Check if pattern is empty
    if pattern.trim().is_empty() {
        return Err("Pattern cannot be empty".to_string());
    }

    // Check for unescaped special characters in string literals
    if pattern.contains("\\(") || pattern.contains("\\)") {
        // These are properly escaped, continue checking other issues
    } else {
        // Look for potentially unescaped parentheses
        if pattern.contains('(') || pattern.contains(')') {
            return Err("Warning: Pattern contains parentheses '(' or ')' that may need to be escaped as '\\(' or '\\)'".to_string());
        }
    }
    
    // Look for potentially unescaped braces
    if pattern.contains('{') || pattern.contains('}') {
        if !pattern.contains("\\{") && !pattern.contains("\\}") {
            return Err("Warning: Pattern contains braces '{' or '}' that may need to be escaped as '\\{' or '\\}'".to_string());
        }
    }
    
    // Check for potentially unescaped square brackets outside of character classes
    let mut in_char_class = false;
    let mut prev_char = None;
    
    for (i, c) in pattern.chars().enumerate() {
        if c == '[' && prev_char != Some('\\') {
            in_char_class = true;
        } else if c == ']' && prev_char != Some('\\') {
            in_char_class = false;
        } else if (c == '[' || c == ']') && prev_char == Some('\\') {
            // This is an escaped bracket, so don't change in_char_class
        }
        
        // When we're not in a character class, detect unescaped square brackets
        if !in_char_class && i > 0 && ((c == '[' && prev_char != Some('\\')) || (c == ']' && prev_char != Some('\\'))) {
            return Err("Warning: Potentially unbalanced square brackets in pattern".to_string());
        }
        
        prev_char = Some(c);
    }
    
    // Check if we ended inside a character class (unbalanced brackets)
    if in_char_class {
        return Err("Warning: Unbalanced square brackets in pattern".to_string());
    }
    
    // Check for potential quantifier issues with quotes
    if let Some(idx) = pattern.find('\'') {
        if idx + 1 < pattern.len() && pattern.chars().nth(idx + 1) == Some('{') {
            return Err("Warning: Single quote followed by '{' may cause issues with quantifiers in regex".to_string());
        }
    }
    
    // Check for unbalanced quotes which might lead to escaping issues
    let single_quotes = pattern.chars().filter(|c| *c == '\'').count();
    let double_quotes = pattern.chars().filter(|c| *c == '"').count();
    
    if single_quotes % 2 != 0 {
        return Err("Warning: Unbalanced single quotes in pattern".to_string());
    }
    
    if double_quotes % 2 != 0 {
        return Err("Warning: Unbalanced double quotes in pattern".to_string());
    }
    
    // Check for common regex syntax errors
    if pattern.contains("**") || pattern.contains("++") || pattern.contains("??") {
        return Err("Warning: Repeated quantifiers (**, ++, ??) are invalid in regex".to_string());
    }
    
    // Check for dollar sign at beginning (rarely intended)
    if pattern.starts_with('$') && pattern.len() > 1 {
        return Err("Warning: Pattern starts with $ which matches end of line, did you mean ^ for start of line?".to_string());
    }
    
    // Try to compile the regex as an additional validation
    if let Err(e) = Regex::new(pattern) {
        return Err(format!("Invalid regex pattern: {}", e));
    }
    
    Ok(())
}

/// Detect and preserve the original line endings in a file
fn detect_line_endings(content: &str) -> &str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Create a backup of the file with a .bak extension
fn create_backup_file(file_path: &str) -> Result<()> {
    let backup_path = format!("{}.bak", file_path);
    
    // Check if backup file already exists and try to delete it
    if Path::new(&backup_path).exists() {
        fs::remove_file(&backup_path)
            .map_err(|e| anyhow!("Failed to remove existing backup file: {}", e))?;
    }
    
    fs::copy(file_path, &backup_path)
        .map_err(|e| anyhow!("Failed to create backup file: {}", e))?;
    
    // Verify backup was created successfully
    if !Path::new(&backup_path).exists() {
        return Err(anyhow!("Backup file creation failed - backup file does not exist after copy"));
    }
    
    Ok(())
}

/// Extract lines of context around a specific line index
fn get_context(lines: &[&str], line_idx: usize, context_lines: u32, total_lines: usize) -> Vec<String> {
    let context = context_lines as usize;
    let start = if line_idx > context { line_idx - context } else { 0 };
    let end = if line_idx + context < total_lines { line_idx + context } else { total_lines - 1 };
    
    let mut result = Vec::new();
    for i in start..=end {
        let prefix = if i == line_idx { ">> " } else { "   " };
        result.push(format!("{}{}: {}", prefix, i + 1, lines[i]));
    }
    
    result
}

pub async fn handle_regex_replace_tool_call(params: CallToolParams, id: Option<Value>) -> Result<JsonRpcResponse> {
    // Ensure id is never null to satisfy Claude Desktop client
    let id = Some(id.unwrap_or(Value::String("regex_replace".into())));
    let args: RegexReplaceParams = serde_json::from_value(params.arguments)
        .map_err(|e| anyhow!("Invalid arguments: {}", e))?;

    // Validate file exists and is readable
    let file_path = Path::new(&args.file_path);
    if !file_path.exists() {
        return Ok(error_response(id, INTERNAL_ERROR, "File not found"));
    }
    
    if !file_path.is_file() {
        return Ok(error_response(id, INTERNAL_ERROR, "Path exists but is not a regular file"));
    }
    
    // Try to check if the file is readable by opening it
    if fs::File::open(&args.file_path).is_err() {
        return Ok(error_response(id, INTERNAL_ERROR, "File exists but could not be opened for reading (check permissions)"));
    }

    // Validate regex patterns are not empty
    if args.start_pattern.trim().is_empty() {
        return Ok(error_response(id, INTERNAL_ERROR, "Start pattern cannot be empty"));
    }
    
    if args.end_pattern.trim().is_empty() {
        return Ok(error_response(id, INTERNAL_ERROR, "End pattern cannot be empty"));
    }

    // Validate regex patterns before proceeding
    if let Err(warning) = validate_regex_pattern(&args.start_pattern) {
        return Ok(error_response(id, INTERNAL_ERROR, &format!("{} in start pattern: '{}'", warning, args.start_pattern)));
    }
    
    if let Err(warning) = validate_regex_pattern(&args.end_pattern) {
        return Ok(error_response(id, INTERNAL_ERROR, &format!("{} in end pattern: '{}'", warning, args.end_pattern)));
    }

    // Read file content
    let mut file = fs::File::open(&args.file_path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| anyhow!("Failed to read file contents: {}", e))?;
    
    // Check if file is empty
    if content.trim().is_empty() {
        return Ok(error_response(id, INTERNAL_ERROR, "File is empty, nothing to replace"));
    }
    
    // Detect and store original line endings
    let line_ending = detect_line_endings(&content);
    
    // Split content into lines to work with line-based matching
    let lines: Vec<&str> = content.split(line_ending).collect();
    
    // Create regex patterns for start and end
    let start_re = match Regex::new(&args.start_pattern) {
        Ok(re) => re,
        Err(e) => return Ok(error_response(id, INTERNAL_ERROR, &format!("Invalid start regex pattern: {}", e))),
    };
    
    let end_re = match Regex::new(&args.end_pattern) {
        Ok(re) => re,
        Err(e) => return Ok(error_response(id, INTERNAL_ERROR, &format!("Invalid end regex pattern: {}", e))),
    };
    let start_matches: Vec<usize> = lines.iter()
        .enumerate()
        .filter(|(_, line)| start_re.is_match(line))
        .map(|(idx, _)| idx)
        .collect();

    if start_matches.is_empty() {
        return Ok(error_response(id, INTERNAL_ERROR, "No matches found for start pattern, no changes made."));
    }

    // Determine which occurrence(s) to replace
    let target_start_indices = if args.match_all {
        start_matches.clone()
    } else if let Some(occurrence) = args.match_occurrence {
        if occurrence == 0 || occurrence as usize > start_matches.len() {
            return Ok(error_response(
                id, 
                INTERNAL_ERROR, 
                &format!("Invalid occurrence {}. Found {} matches for start pattern.", 
                         occurrence, start_matches.len())
            ));
        }
        vec![start_matches[occurrence as usize - 1]]
    } else if start_matches.len() == 1 {
        // Default behavior when only one match is found
        start_matches.clone()
    } else {
        return Ok(error_response(
            id, 
            INTERNAL_ERROR, 
            &format!("Found {} matches for start pattern. Please specify which occurrence to replace using the match_occurrence parameter, or set match_all to true.", 
                     start_matches.len())
        ));
    };

    // Create backup if requested
    if args.create_backup && !args.dry_run {
        create_backup_file(&args.file_path)?;
    }

    let mut new_lines = lines.clone().into_iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let mut replacements_made = Vec::new();
    
    // Process each target start index (in reverse to maintain correct indices)
    for &start_line_idx in target_start_indices.iter().rev() {
        // Find the ending line that matches the end pattern, starting from the start match
        let remaining_lines = &lines[start_line_idx..];
        
        // First check if the start pattern itself matches the end pattern
        // This handles cases where a single line should match both patterns
        let start_line = lines[start_line_idx];
        let start_also_matches_end = end_re.is_match(start_line);
        
        let mut end_matches: Vec<usize> = if start_also_matches_end {
            // If the start line also matches the end pattern, we'll include it as a possible end match
            vec![start_line_idx]
        } else {
            Vec::new()
        };
        
        // Then look for other end matches after the start line
        let additional_end_matches: Vec<usize> = remaining_lines.iter()
            .enumerate()
            .skip(1) // Skip the start line since we've already checked it
            .filter(|(_, line)| end_re.is_match(line))
            .map(|(idx, _)| idx + start_line_idx) // Adjust index to be relative to the original array
            .collect();
        
        // Combine any matches
        end_matches.extend(additional_end_matches);
        
        // Sort matches by line number to ensure we get the closest match
        end_matches.sort();
        
        if end_matches.is_empty() {
            return Ok(error_response(
                id, 
                INTERNAL_ERROR, 
                &format!("No matches found for end pattern '{}' after line {}. Try making your end pattern less restrictive or ensure it exists in the file after the start pattern.", 
                         args.end_pattern, start_line_idx + 1)
            ));
        }
        
        // Get the first end match that comes after the start match
        let end_line_idx = end_matches[0];
        
        // Capture context for the response
        let context_lines = args.context_lines.unwrap_or(2);
        let start_context = get_context(&lines, start_line_idx, context_lines, lines.len());
        let end_context = get_context(&lines, end_line_idx, context_lines, lines.len());
        
        // Record this replacement
        replacements_made.push((
            start_line_idx, 
            end_line_idx,
            start_context,
            end_context
        ));
        
        // Skip actual replacement if this is a dry run
        if !args.dry_run {
            // Remove the lines between start and end (inclusive)
            new_lines.splice(start_line_idx..=end_line_idx, vec![args.replacement.clone()]);
        }
    }
    
    if !args.dry_run {
        // Join the lines back together with original line endings
        let new_content = new_lines.join(line_ending);
        
        // Write the modified content back to the file with error handling
        match fs::write(&args.file_path, &new_content) {
            Ok(_) => {
                // Verify the file was written correctly
                match fs::read_to_string(&args.file_path) {
                    Ok(written_content) if written_content == new_content => {
                        // File was written successfully and content matches
                    },
                    Ok(_) => {
                        return Ok(error_response(
                            id,
                            INTERNAL_ERROR,
                            "File was written but content verification failed - file may be corrupted. Check backup file."
                        ));
                    },
                    Err(e) => {
                        return Ok(error_response(
                            id,
                            INTERNAL_ERROR,
                            &format!("File was written but could not be verified: {}", e)
                        ));
                    }
                }
            },
            Err(e) => {
                return Ok(error_response(
                    id,
                    INTERNAL_ERROR,
                    &format!("Failed to write modified content to file: {}", e)
                ));
            }
        }
    }
    
    // Build detailed response message
    let mut response_text = if args.dry_run {
        "DRY RUN - No changes made to file.\n".to_string()
    } else {
        "".to_string()
    };
    
    for (idx, (start_idx, end_idx, start_context, end_context)) in replacements_made.iter().enumerate() {
        let replaced_lines_count = (end_idx - start_idx) + 1;
        
        response_text.push_str(&format!(
            "{}Replacement #{}: {} lines from line {} to line {}\n",
            if idx > 0 { "\n" } else { "" },
            idx + 1,
            replaced_lines_count, 
            start_idx + 1, 
            end_idx + 1
        ));
        
        // Add context information
        response_text.push_str("Before replacement (start):\n");
        for line in start_context {
            response_text.push_str(&format!("{}\n", line));
        }
        
        response_text.push_str("\nBefore replacement (end):\n");
        for line in end_context {
            response_text.push_str(&format!("{}\n", line));
        }
    }
    
    // Add summary
    let summary = if args.dry_run {
        format!("Dry run completed. Found {} potential replacements.", replacements_made.len())
    } else {
        format!("Successfully made {} replacements.", replacements_made.len())
    };
    
    response_text.push_str(&format!("\n{}", summary));
    
    let tool_res = CallToolResult {
        content: vec![ToolResponseContent {
            type_: "text".into(),
            text: response_text,
            annotations: None,
        }],
        is_error: Some(false),
        _meta: None,
        progress: None,
        total: None,
    };
    
    Ok(success_response(id, serde_json::to_value(tool_res)?))
}

pub fn regex_replace_tool_info() -> shared_protocol_objects::ToolInfo {
    shared_protocol_objects::ToolInfo {
        name: "regex_replace".to_string(),
        description: Some(
            "Precision multi-line text replacement tool using regular expressions to safely modify files. Use this to:
            
            1. Replace sections of code or content between specified regex patterns
            2. Update multi-line blocks in configuration files
            3. Modify sections of text while preserving surrounding content
            4. Make targeted multi-line modifications in various file types
            
            Safety features:
            - Reports the number of matches when more than one is found
            - Preserves original file line endings and encoding
            - Provides dry-run option to preview changes
            - Can create backup files automatically
            - Shows context around replacements
            - Never modifies files unless explicitly instructed
            
            Pattern syntax guide (Rust regex):
            - Character classes: [a-z], [0-9], \\w (word), \\d (digit), \\s (whitespace)
            - Anchors: ^ (start of line), $ (end of line), \\b (word boundary)
            - Quantifiers: * (0+), + (1+), ? (0-1), {n} (exactly n), {n,m} (n to m)
            - Groups: (pattern) creates a capture group, (?:pattern) non-capturing
            
            Example use cases:
            - Replace a function: '^function myFunc\\(\\) {$' and '^}$' with a new implementation
            - Update config blocks: '^config: {$' and '^}$' with new configuration
            - Replace specific sections: '^// START SECTION$' and '^// END SECTION$' with new content".to_string()
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path to the target file."
                },
                "start_pattern": {
                    "type": "string",
                    "description": "The regex pattern to match the start line of the section to replace."
                },
                "end_pattern": {
                    "type": "string",
                    "description": "The regex pattern to match the end line of the section to replace."
                },
                "replacement": {
                    "type": "string",
                    "description": "The text that will replace all lines between the first and last match (inclusive)."
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "If true, shows what changes would be made without actually modifying the file.",
                    "default": false
                },
                "match_occurrence": {
                    "type": "integer",
                    "description": "Which occurrence to replace (1-based indexing). Required if multiple matches are found and match_all is false.",
                    "minimum": 1
                },
                "match_all": {
                    "type": "boolean",
                    "description": "If true, replaces all occurrences of the pattern.",
                    "default": false
                },
                "create_backup": {
                    "type": "boolean",
                    "description": "If true, creates a .bak backup file before making changes.",
                    "default": false
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines to show before and after the replacement.",
                    "default": 2,
                    "minimum": 0,
                    "maximum": 10
                }
            },
            "required": ["file_path", "start_pattern", "end_pattern", "replacement"],
            "additionalProperties": false
        }),
    }
}
