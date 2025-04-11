use serde_json::json;
use tokio::test;

use shared_protocol_objects::{
    ClientCapabilities, ServerCapabilities, InitializeResult, Implementation,
    PromptsCapability, ResourcesCapability, ToolsCapability,
    JsonRpcRequest, JsonRpcResponse, InitializeParams,
};

// Test basic capabilities structure
#[test]
async fn test_capabilities_structure() {
    // Create client capabilities
    let client_caps = ClientCapabilities {
        experimental: Some(json!({
            "code_completion": true,
            "streaming": true
        })),
        sampling: Some(json!({
            "temperature": 0.7,
            "top_p": 0.9
        })),
        roots: None,
    };
    
    // Verify client capabilities
    assert!(client_caps.experimental.is_some(), "Experimental should be set");
    assert_eq!(
        client_caps.experimental.as_ref().unwrap().get("code_completion").unwrap(),
        &json!(true),
        "code_completion should be true"
    );
    
    // Create server capabilities
    let server_caps = ServerCapabilities {
        experimental: Some([
            ("custom_feature".to_string(), json!(true)),
            ("max_tokens".to_string(), json!(4096))
        ].iter().cloned().collect()),
        logging: Some(json!({
            "level": "debug"
        })),
        prompts: Some(PromptsCapability {
            list_changed: true
        }),
        resources: Some(ResourcesCapability {
            list_changed: true,
            subscribe: false
        }),
        tools: Some(ToolsCapability {
            list_changed: true
        }),
    };
    
    // Verify server capabilities
    assert!(server_caps.experimental.is_some(), "Experimental should be set");
    assert_eq!(
        server_caps.experimental.as_ref().unwrap().get("custom_feature").unwrap(),
        &json!(true),
        "custom_feature should be true"
    );
    assert!(server_caps.prompts.is_some(), "Prompts capability should be set");
    assert!(server_caps.resources.is_some(), "Resources capability should be set");
    assert!(server_caps.tools.is_some(), "Tools capability should be set");
}

// Test initialization flow with capabilities negotiation
#[test]
async fn test_initialize_flow() {
    // Create client info
    let client_info = Implementation {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    // Create client capabilities
    let client_caps = ClientCapabilities {
        experimental: Some(json!({
            "feature1": true
        })),
        sampling: None,
        roots: None,
    };
    
    // Create initialize params
    let init_params = InitializeParams {
        protocol_version: "2025-03-26".to_string(),
        capabilities: client_caps.clone(),
        client_info: client_info.clone(),
    };
    
    // Create initialize request
    let init_request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "initialize".to_string(),
        params: Some(json!(init_params)),
        id: json!(1),
    };
    
    // Verify request format
    assert_eq!(init_request.method, "initialize", "Method should be initialize");
    let params_value = init_request.params.unwrap();
    assert_eq!(
        params_value.get("protocol_version").unwrap(),
        "2025-03-26",
        "Protocol version should match"
    );
    
    // Create server info
    let server_info = Implementation {
        name: "test-server".to_string(),
        version: "1.0.0".to_string(),
    };
    
    // Create server capabilities
    let server_caps = ServerCapabilities {
        experimental: Some([
            ("feature1".to_string(), json!(true)),
            ("feature2".to_string(), json!(false))
        ].iter().cloned().collect()),
        logging: None,
        prompts: Some(PromptsCapability {
            list_changed: true
        }),
        resources: None,
        tools: Some(ToolsCapability {
            list_changed: true
        }),
    };
    
    // Create initialize result
    let init_result = InitializeResult {
        protocol_version: "2025-03-26".to_string(),
        capabilities: server_caps.clone(),
        server_info: server_info.clone(),
        instructions: Some("This server provides access to project data and search tools.".to_string()), // Added instructions
    };

    // Create response
    let init_response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        result: Some(json!(init_result)),
        error: None,
    };
    
    // Verify response format
    assert!(init_response.error.is_none(), "Response should not have an error");
    assert!(init_response.result.is_some(), "Response should have a result");
    
    let result_value = init_response.result.unwrap();
    assert_eq!(
        result_value.get("protocol_version").unwrap(),
        "2025-03-26",
        "Protocol version should match"
    );
    
    // Extract result for verification
    let extracted_result: InitializeResult = serde_json::from_value(result_value).unwrap();
    assert_eq!(extracted_result.server_info.name, "test-server", "Server name should match");
    assert!(extracted_result.capabilities.tools.is_some(), "Tools capability should be set");
    assert!(extracted_result.capabilities.prompts.is_some(), "Prompts capability should be set");
}

// Test capability matching logic
#[test]
async fn test_capability_matching() {
    // Define client capabilities
    let client_caps = ClientCapabilities {
        experimental: Some(json!({
            "feature1": true,
            "feature2": false,
            "feature3": "value"
        })),
        sampling: Some(json!({
            "temperature": 0.7
        })),
        roots: None,
    };
    
    // Define server capabilities
    let server_caps = ServerCapabilities {
        experimental: Some([
            ("feature1".to_string(), json!(true)),
            ("feature2".to_string(), json!(true)),
            ("feature4".to_string(), json!("server_only"))
        ].iter().cloned().collect()),
        logging: None,
        prompts: Some(PromptsCapability {
            list_changed: true
        }),
        resources: None,
        tools: Some(ToolsCapability {
            list_changed: true
        }),
    };
    
    // Check for shared capabilities
    if let Some(client_exp) = &client_caps.experimental {
        if let Some(server_exp) = &server_caps.experimental {
            // Feature1 exists in both
            assert!(client_exp.get("feature1").is_some(), "Client should have feature1");
            assert!(server_exp.get("feature1").is_some(), "Server should have feature1");
            
            // Feature2 exists in both but with different values
            assert_eq!(client_exp.get("feature2").unwrap(), &json!(false), "Client feature2 should be false");
            assert_eq!(server_exp.get("feature2").unwrap(), &json!(true), "Server feature2 should be true");
            
            // Feature3 only exists in client
            assert!(client_exp.get("feature3").is_some(), "Client should have feature3");
            assert!(server_exp.get("feature3").is_none(), "Server should not have feature3");
            
            // Feature4 only exists in server
            assert!(client_exp.get("feature4").is_none(), "Client should not have feature4");
            assert!(server_exp.get("feature4").is_some(), "Server should have feature4");
        }
    }
}

// Test protocol version compatibility
#[test]
async fn test_protocol_version_compatibility() {
    // Supported versions from the crate
    let supported_versions = shared_protocol_objects::SUPPORTED_PROTOCOL_VERSIONS;
    let latest_version = shared_protocol_objects::LATEST_PROTOCOL_VERSION;
    
    // Latest should be in supported
    assert!(supported_versions.contains(&latest_version), "Latest version should be in supported versions");
    
    // Create initialization parameters with different versions
    let valid_version = InitializeParams {
        protocol_version: latest_version.to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        },
        client_info: Implementation {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };
    
    // Should be able to serialize/deserialize with valid version
    let json_value = serde_json::to_value(&valid_version).unwrap();
    let parsed: InitializeParams = serde_json::from_value(json_value).unwrap();
    assert_eq!(parsed.protocol_version, latest_version, "Protocol version should be preserved");
    
    // Create with unsupported version
    let unsupported_version = "2020-01-01";
    assert!(!supported_versions.contains(&unsupported_version), "Test version should be unsupported");
    
    let invalid_version = InitializeParams {
        protocol_version: unsupported_version.to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            sampling: None,
            roots: None,
        },
        client_info: Implementation {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };
    
    // Should still serialize/deserialize (schema validation would happen at runtime)
    let json_value = serde_json::to_value(&invalid_version).unwrap();
    let parsed: InitializeParams = serde_json::from_value(json_value).unwrap();
    assert_eq!(parsed.protocol_version, unsupported_version, "Protocol version should be preserved even if invalid");
}
