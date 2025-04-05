use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;

use shared_protocol_objects::{
    ToolResponseContent, CallToolResult, ResourceContent
};

#[test]
fn test_basic_text_response() {
    // Create a simple text response
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "This is a simple text response.".to_string(),
        annotations: None,
    };
    
    // Create a tool result
    let result = CallToolResult {
        content: vec![content],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };
    
    // Verify the result structure
    assert_eq!(result.content.len(), 1, "Should have one content item");
    assert_eq!(result.content[0].type_, "text", "Content type should be text");
    assert_eq!(result.content[0].text, "This is a simple text response.", "Text content should match");
    assert!(result.is_error.is_none(), "Error flag should be None by default");
}

#[test]
fn test_error_response() {
    // Create an error response
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "An error occurred during execution.".to_string(),
        annotations: None,
    };
    
    // Create a tool result with error flag
    let result = CallToolResult {
        content: vec![content],
        is_error: Some(true),
        _meta: None,
        progress: None,
        total: None,
    };
    
    // Verify the result
    assert_eq!(result.content[0].text, "An error occurred during execution.", "Error text should match");
    assert_eq!(result.is_error, Some(true), "Error flag should be true");
}

#[test]
fn test_multi_part_response() {
    // Create multiple content parts
    let content1 = ToolResponseContent {
        type_: "text".to_string(),
        text: "Part 1 of the response.".to_string(),
        annotations: None,
    };
    
    let content2 = ToolResponseContent {
        type_: "text".to_string(),
        text: "Part 2 of the response.".to_string(),
        annotations: None,
    };
    
    // Create a tool result with multiple parts
    let result = CallToolResult {
        content: vec![content1, content2],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };
    
    // Verify the result
    assert_eq!(result.content.len(), 2, "Should have two content items");
    assert_eq!(result.content[0].text, "Part 1 of the response.", "First part text should match");
    assert_eq!(result.content[1].text, "Part 2 of the response.", "Second part text should match");
}

#[test]
fn test_annotated_response() {
    // Create annotations
    let mut annotations = HashMap::new();
    annotations.insert("highlight".to_string(), json!(true));
    annotations.insert("code_language".to_string(), json!("rust"));
    
    // Create a content with annotations
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "fn main() { println!(\"Hello, world!\"); }".to_string(),
        annotations: Some(annotations),
    };
    
    // Create a tool result
    let result = CallToolResult {
        content: vec![content],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };
    
    // Verify the result
    let content_annotations = result.content[0].annotations.as_ref().unwrap();
    assert!(content_annotations.contains_key("highlight"), "Should have highlight annotation");
    assert!(content_annotations.contains_key("code_language"), "Should have code_language annotation");
    assert_eq!(content_annotations["highlight"], json!(true), "Highlight value should be true");
    assert_eq!(content_annotations["code_language"], json!("rust"), "Code language should be rust");
}

#[test]
fn test_progress_tracking() {
    // Create a content item
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "Processing large dataset...".to_string(),
        annotations: None,
    };
    
    // Create a tool result with progress indicator
    let result = CallToolResult {
        content: vec![content],
        is_error: None,
        _meta: None,
        progress: Some(50),
        total: Some(100),
    };
    
    // Verify the progress indicators
    assert_eq!(result.progress, Some(50), "Progress should be 50");
    assert_eq!(result.total, Some(100), "Total should be 100");
}

#[test]
fn test_metadata_inclusion() {
    // Create a content item
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "Result with metadata".to_string(),
        annotations: None,
    };
    
    // Create a tool result with metadata
    let result = CallToolResult {
        content: vec![content],
        is_error: None,
        _meta: Some(json!({
            "execution_time_ms": 123,
            "cache_hit": false,
            "source": "database"
        })),
        progress: None,
        total: None,
    };
    
    // Verify the metadata
    let meta = result._meta.unwrap();
    assert_eq!(meta.get("execution_time_ms").unwrap(), 123, "Execution time should match");
    assert_eq!(meta.get("cache_hit").unwrap(), false, "Cache hit should be false");
    assert_eq!(meta.get("source").unwrap(), "database", "Source should be database");
}

#[test]
fn test_json_serialization() -> Result<()> {
    // Create a content item
    let content = ToolResponseContent {
        type_: "text".to_string(),
        text: "Serialized content".to_string(),
        annotations: None,
    };
    
    // Create a tool result
    let result = CallToolResult {
        content: vec![content],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };
    
    // Serialize to JSON
    let json_str = serde_json::to_string(&result)?;
    
    // Verify the JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json_str)?;
    assert!(parsed.get("content").is_some(), "JSON should have content field");
    assert_eq!(parsed["content"][0]["type"], "text", "Content type should be text");
    assert_eq!(parsed["content"][0]["text"], "Serialized content", "Text should match");
    
    // Fields with None values should not be present
    assert!(!parsed.get("is_error").is_some(), "is_error should not be included when None");
    assert!(!parsed.get("_meta").is_some(), "_meta should not be included when None");
    assert!(!parsed.get("progress").is_some(), "progress should not be included when None");
    assert!(!parsed.get("total").is_some(), "total should not be included when None");
    
    Ok(())
}