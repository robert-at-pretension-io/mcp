use anyhow::Result;
use serde_json::json;
use std::env;
use tokio::test;

use mcp_tools::brave_search::BraveSearchClient;
use mcp_tools::scraping_bee::ScrapingBeeClient;
use mcp_tools::google_search::{GoogleSearchClient, GoogleSearchParams};
use mcp_tools::tool_impls::create_tools;

// Test API key handling for the BraveSearch tool
#[test]
async fn test_brave_search_api_key_management() -> Result<()> {
    // Save original API key
    let original_key = env::var("BRAVE_API_KEY").ok();
    
    // Test with valid API key
    env::set_var("BRAVE_API_KEY", "test_api_key");
    
    // Create client
    let client = BraveSearchClient::from_env();
    
    // Check that the API key was loaded
    assert!(client.is_ok(), "Client should initialize with API key");
    let client = client.unwrap();
    
    // Create query parameters
    let search_params = mcp_tools::brave_search::BraveSearchParams {
        query: "test query".to_string(),
        country: Some("US".to_string()),
        search_lang: None,
    };
    
    // We won't actually execute the request since it would call the real API
    // But we can check that the request would include the API key
    let request = client.create_request(&search_params)?;
    
    // Check authorization header
    let auth_header = request.headers().get("X-Subscription-Token")
        .expect("Request should have authorization header");
    assert_eq!(
        auth_header.to_str().unwrap(),
        "test_api_key",
        "Authorization header should contain API key"
    );
    
    // Test with missing API key
    env::remove_var("BRAVE_API_KEY");
    let client_result = BraveSearchClient::from_env();
    assert!(client_result.is_err(), "Client should not initialize without API key");
    
    // Restore original key
    if let Some(key) = original_key {
        env::set_var("BRAVE_API_KEY", key);
    }
    
    Ok(())
}

// Test API key handling for the ScrapingBee tool
#[test]
async fn test_scraping_bee_api_key_management() -> Result<()> {
    // Save original API key
    let original_key = env::var("SCRAPING_BEE_API_KEY").ok();
    
    // Test with valid API key
    env::set_var("SCRAPING_BEE_API_KEY", "test_scraping_bee_key");
    
    // Create client
    let client = ScrapingBeeClient::new();
    
    // Create request for a URL
    let url = "https://example.com";
    let api_key = client.api_key.clone();
    
    // Verify API key was loaded
    assert_eq!(api_key, "test_scraping_bee_key", "API key should be loaded from environment");
    
    // Build URL with parameters
    let request_url = client.build_url(url, true, None, None)?;
    
    // Check that the URL includes the API key
    assert!(request_url.contains("api_key=test_scraping_bee_key"), "URL should include API key");
    
    // Test missing API key
    env::remove_var("SCRAPING_BEE_API_KEY");
    let tools = create_tools().await?;
    
    // Verify the scraping_bee tool is not available when API key is missing
    let scrape_tool = tools.iter().find(|t| t.name() == "scrape_url");
    assert!(scrape_tool.is_none(), "Scrape tool should not be created without API key");
    
    // Restore original key
    if let Some(key) = original_key {
        env::set_var("SCRAPING_BEE_API_KEY", key);
    }
    
    Ok(())
}

// Test Google API key and CX handling
#[test]
async fn test_google_search_auth_management() -> Result<()> {
    // Save original keys
    let original_api_key = env::var("GOOGLE_API_KEY").ok();
    let original_cx = env::var("GOOGLE_SEARCH_CX").ok();
    
    // Test with valid keys
    env::set_var("GOOGLE_API_KEY", "test_google_api_key");
    env::set_var("GOOGLE_SEARCH_CX", "test_google_cx");
    
    // Try to create client
    let client_result = GoogleSearchClient::from_env();
    assert!(client_result.is_ok(), "Client should initialize with API key and CX");
    
    let client = client_result.unwrap();
    
    // Create search parameters
    let params = GoogleSearchParams {
        query: "test query".to_string(),
    };
    
    // Build URL without actually executing the request
    let url = client.build_url(&params)?;
    
    // Verify URL contains authentication parameters
    assert!(url.contains("key=test_google_api_key"), "URL should contain API key");
    assert!(url.contains("cx=test_google_cx"), "URL should contain CX value");
    
    // Test with missing CX
    env::remove_var("GOOGLE_SEARCH_CX");
    let client_result = GoogleSearchClient::from_env();
    assert!(client_result.is_err(), "Client should not initialize without CX");
    
    // Test with missing API key
    env::remove_var("GOOGLE_API_KEY");
    env::set_var("GOOGLE_SEARCH_CX", "test_google_cx");
    let client_result = GoogleSearchClient::from_env();
    assert!(client_result.is_err(), "Client should not initialize without API key");
    
    // Test tools creation without keys
    env::remove_var("GOOGLE_API_KEY");
    env::remove_var("GOOGLE_SEARCH_CX");
    let tools = create_tools().await?;
    
    // Verify Google search tool is not available
    let google_tool = tools.iter().find(|t| t.name() == "google_search");
    assert!(google_tool.is_none(), "Google search tool should not be created without keys");
    
    // Restore original keys
    if let Some(key) = original_api_key {
        env::set_var("GOOGLE_API_KEY", key);
    }
    
    if let Some(cx) = original_cx {
        env::set_var("GOOGLE_SEARCH_CX", cx);
    }
    
    Ok(())
}

// Test API key validation logic
#[test]
async fn test_api_key_validation() -> Result<()> {
    // Check that empty API keys are rejected
    let empty_key = "";
    assert!(!is_valid_api_key(empty_key), "Empty key should be invalid");
    
    // Check that very short keys are rejected
    let short_key = "123";
    assert!(!is_valid_api_key(short_key), "Short key should be invalid");
    
    // Check that keys with disallowed characters are rejected
    let invalid_chars_key = "api_key_with_invalid!chars";
    assert!(!is_valid_api_key(invalid_chars_key), "Key with invalid characters should be invalid");
    
    // Check that valid keys are accepted
    let valid_key = "valid_api_key_123456789abcdefABCDEF";
    assert!(is_valid_api_key(valid_key), "Valid key should be valid");
    
    Ok(())
}

// Mock validation function - real implementations would be more sophisticated
fn is_valid_api_key(key: &str) -> bool {
    // Basic validation rules:
    // 1. Must not be empty
    if key.is_empty() {
        return false;
    }
    
    // 2. Must be at least 8 characters
    if key.len() < 8 {
        return false;
    }
    
    // 3. Must contain only alphanumeric and underscore characters
    if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return false;
    }
    
    true
}

// Test secure handling of API keys
#[test]
fn test_secure_key_handling() {
    // Test that API keys are not accidentally logged or output
    let api_key = "sensitive_api_key_12345";
    
    // Convert key to a format that might be logged
    let safe_key = mask_api_key(api_key);
    
    // Verify the key is masked
    assert!(!safe_key.contains(api_key), "Full API key should not appear in logged string");
    assert!(safe_key.contains("***"), "API key should be masked with asterisks");
    
    // Check first/last few characters are preserved for identification
    assert!(safe_key.starts_with("sens"), "Masked key should preserve start characters");
    assert!(safe_key.ends_with("345"), "Masked key should preserve end characters");
}

// Mock function to securely mask API keys for logging
fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "***".to_string();
    }
    
    let prefix_len = 4.min(key.len() / 3);
    let suffix_len = 3.min(key.len() / 4);
    
    let prefix = &key[0..prefix_len];
    let suffix = &key[key.len() - suffix_len..];
    
    format!("{}***{}", prefix, suffix)
}