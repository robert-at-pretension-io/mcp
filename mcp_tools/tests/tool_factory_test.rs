use anyhow::Result;
use std::env;
use tokio::test;

// Import tool implementations
use mcp_tools::tool_impls::create_tools;
use mcp_tools::tool_trait::Tool;

#[test]
async fn test_tool_factory_initialization() -> Result<()> {
    // Test creating tools with default settings
    let tools = create_tools().await?;
    
    // Verify that we got a non-empty list of tools
    assert!(!tools.is_empty(), "Should create at least some default tools");
    
    // Check for some common tools that should always be available
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    
    // Basic tools that should always be available
    assert!(tool_names.contains(&"bash"), "Should have bash tool");
    assert!(tool_names.contains(&"quick_bash"), "Should have quick_bash tool");
    assert!(tool_names.contains(&"regex_replace"), "Should have regex_replace tool");
    
    // Verify tool info
    for tool in &tools {
        let info = tool.info();
        assert_eq!(info.name, tool.name(), "Tool name should match");
        assert!(info.description.is_some(), "Tool should have a description");
        assert!(info.input_schema.is_object(), "Tool schema should be a JSON object");
    }
    
    Ok(())
}

#[test]
async fn test_tool_factory_environment_vars() -> Result<()> {
    // Save the original environment variables
    let original_brave_key = env::var("BRAVE_API_KEY").ok();
    let original_scraping_bee_key = env::var("SCRAPING_BEE_API_KEY").ok();
    let original_google_key = env::var("GOOGLE_API_KEY").ok();
    let original_google_cx = env::var("GOOGLE_SEARCH_CX").ok();
    
    // Set environment variables to test values
    env::set_var("BRAVE_API_KEY", "test_brave_key");
    env::set_var("SCRAPING_BEE_API_KEY", "test_scraping_bee_key");
    env::set_var("GOOGLE_API_KEY", "test_google_key");
    env::set_var("GOOGLE_SEARCH_CX", "test_google_cx");
    
    // Create tools with test environment
    let tools = create_tools().await?;
    
    // Verify that web tools are created when API keys are set
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    
    assert!(tool_names.contains(&"brave_search"), "Should have brave_search tool when API key is set");
    assert!(tool_names.contains(&"scrape_url"), "Should have scrape_url tool when API key is set");
    assert!(tool_names.contains(&"google_search"), "Should have google_search tool when API keys are set");
    
    // Restore original environment
    match original_brave_key {
        Some(key) => env::set_var("BRAVE_API_KEY", key),
        None => env::remove_var("BRAVE_API_KEY"),
    }
    
    match original_scraping_bee_key {
        Some(key) => env::set_var("SCRAPING_BEE_API_KEY", key),
        None => env::remove_var("SCRAPING_BEE_API_KEY"),
    }
    
    match original_google_key {
        Some(key) => env::set_var("GOOGLE_API_KEY", key),
        None => env::remove_var("GOOGLE_API_KEY"),
    }
    
    match original_google_cx {
        Some(cx) => env::set_var("GOOGLE_SEARCH_CX", cx),
        None => env::remove_var("GOOGLE_SEARCH_CX"),
    }
    
    Ok(())
}

#[test]
async fn test_tool_factory_no_environment_vars() -> Result<()> {
    // Save the original environment variables
    let original_brave_key = env::var("BRAVE_API_KEY").ok();
    let original_scraping_bee_key = env::var("SCRAPING_BEE_API_KEY").ok();
    let original_google_key = env::var("GOOGLE_API_KEY").ok();
    let original_google_cx = env::var("GOOGLE_SEARCH_CX").ok();
    
    // Remove all API key environment variables
    env::remove_var("BRAVE_API_KEY");
    env::remove_var("SCRAPING_BEE_API_KEY");
    env::remove_var("GOOGLE_API_KEY");
    env::remove_var("GOOGLE_SEARCH_CX");
    
    // Create tools without environment variables
    let tools = create_tools().await?;
    
    // Get tool names
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    
    // Web tools requiring API keys should not be available
    assert!(!tool_names.contains(&"brave_search"), "Should not have brave_search tool when API key is missing");
    assert!(!tool_names.contains(&"google_search"), "Should not have google_search tool when API keys are missing");
    assert!(!tool_names.contains(&"scrape_url"), "Should not have scrape_url tool when API key is missing");
    
    // But basic tools should still be available
    assert!(tool_names.contains(&"bash"), "Should have bash tool regardless of environment");
    assert!(tool_names.contains(&"quick_bash"), "Should have quick_bash tool regardless of environment");
    assert!(tool_names.contains(&"regex_replace"), "Should have regex_replace tool regardless of environment");
    
    // Restore original environment
    match original_brave_key {
        Some(key) => env::set_var("BRAVE_API_KEY", key),
        None => env::remove_var("BRAVE_API_KEY"),
    }
    
    match original_scraping_bee_key {
        Some(key) => env::set_var("SCRAPING_BEE_API_KEY", key),
        None => env::remove_var("SCRAPING_BEE_API_KEY"),
    }
    
    match original_google_key {
        Some(key) => env::set_var("GOOGLE_API_KEY", key),
        None => env::remove_var("GOOGLE_API_KEY"),
    }
    
    match original_google_cx {
        Some(cx) => env::set_var("GOOGLE_SEARCH_CX", cx),
        None => env::remove_var("GOOGLE_SEARCH_CX"),
    }
    
    Ok(())
}

#[test]
async fn test_tool_info_consistency() -> Result<()> {
    // Create tools
    let tools = create_tools().await?;
    
    // Check that all tools have consistent naming and schema
    for tool in &tools {
        let info = tool.info();
        
        // Verify name consistency
        assert_eq!(tool.name(), info.name, "Tool name should match info.name");
        
        // Verify schema basics
        let schema = &info.input_schema;
        assert!(schema.is_object(), "Schema should be a JSON object");
        
        // Verify that schemas with required fields have those properties defined
        if let Some(required) = schema.get("required") {
            if let Some(required_array) = required.as_array() {
                if let Some(properties) = schema.get("properties") {
                    if let Some(props_obj) = properties.as_object() {
                        for req_field in required_array {
                            if let Some(field_name) = req_field.as_str() {
                                assert!(
                                    props_obj.contains_key(field_name),
                                    "Required field '{}' should be defined in properties",
                                    field_name
                                );
                            }
                        }
                    }
                }
            }
        }
        
        // Make sure description exists and is not empty
        assert!(
            info.description.is_some() && !info.description.as_ref().unwrap().is_empty(),
            "Tool should have a non-empty description"
        );
    }
    
    Ok(())
}