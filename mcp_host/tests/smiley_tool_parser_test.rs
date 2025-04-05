use mcp_host::smiley_tool_parser::SmileyToolParser;

#[test]
fn test_parsing_tool_calls() {
    // Test with single tool call
    let response = r#"
    I'll help you with that information. Let me search for it.

    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "google_search",
      "arguments": {
        "query": "rust programming language"
      }
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊

    I'm searching for information about Rust programming now.
    "#;
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "google_search");
    assert_eq!(tool_calls[0].arguments["query"], "rust programming language");
}

#[test]
fn test_multiple_tool_calls() {
    // Test with multiple tool calls
    let response = r#"
    I'll need to do a few things to answer your question.

    First, let me search for the current weather:
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "weather",
      "arguments": {
        "location": "New York"
      }
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊

    Now, let me check the forecast:
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "forecast",
      "arguments": {
        "location": "New York",
        "days": 5
      }
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    "#;
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].name, "weather");
    assert_eq!(tool_calls[1].name, "forecast");
    assert_eq!(tool_calls[1].arguments["days"], 5);
}

#[test]
fn test_invalid_json() {
    // Test with invalid JSON
    let response = r#"
    I'll help you with that.

    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "search",
      "arguments": {
        "query": "this is invalid JSON
      }
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    "#;
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 0);
}

#[test]
fn test_missing_required_fields() {
    // Test with missing required fields
    let response = r#"
    I'll help you with that.

    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "search"
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊😊
    "#;
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 0);
}

#[test]
fn test_no_tool_calls() {
    // Test with no tool calls
    let response = "I don't need to use any tools for this. The answer is 42.";
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 0);
}

#[test]
fn test_incorrect_smiley_count() {
    // Test with incorrect number of smileys (13 vs 14)
    let response = r#"
    😊😊😊😊😊😊😊😊😊😊😊😊😊
    {
      "name": "search",
      "arguments": {
        "query": "rust"
      }
    }
    😊😊😊😊😊😊😊😊😊😊😊😊😊
    "#;
    
    let tool_calls = SmileyToolParser::parse_tool_calls(response);
    assert_eq!(tool_calls.len(), 0);
}