use anyhow::Result;
use serde_json::json;
use tokio::test;

use mcp_tools::bash::{BashExecutor, BashParams, QuickBashParams, handle_quick_bash};
use mcp_tools::tool_trait::Tool;

// Mock implementation for testing the BashTool
struct MockBashTool {
    executor: BashExecutor,
}

impl MockBashTool {
    fn new() -> Self {
        Self {
            executor: BashExecutor::new(),
        }
    }
}

impl Tool for MockBashTool {
    fn name(&self) -> &str {
        "bash"
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        self.executor.tool_info()
    }
    
    fn execute(&self, params: shared_protocol_objects::CallToolParams, id: Option<serde_json::Value>) -> mcp_tools::tool_trait::ExecuteFuture {
        Box::pin(async move {
            // Parse the parameters
            let bash_params: BashParams = serde_json::from_value(params.arguments)?;
            
            // Execute the command
            let result = self.executor.execute(bash_params).await?;
            
            // Create the response
            let content = shared_protocol_objects::ToolResponseContent {
                type_: "text".to_string(),
                text: format!("stdout: {}\nstderr: {}", result.stdout, result.stderr),
                annotations: None,
            };
            
            let response = shared_protocol_objects::CallToolResult {
                content: vec![content],
                is_error: Some(!result.success),
                _meta: None,
                progress: None,
                total: None,
            };
            
            Ok(shared_protocol_objects::success_response(
                Some(id.unwrap_or(json!(null))),
                json!(response)
            ))
        })
    }
}

#[test]
async fn test_bash_command_execution() -> Result<()> {
    // Create a BashExecutor
    let executor = BashExecutor::new();
    
    // Execute a simple command
    let params = BashParams {
        command: "echo 'Hello, world!'".to_string(),
        cwd: std::env::current_dir()?.to_string_lossy().to_string(),
    };
    
    let result = executor.execute(params).await?;
    
    // Verify the result
    assert!(result.success, "Command should succeed");
    assert_eq!(result.status, 0, "Exit status should be 0");
    assert_eq!(result.stdout.trim(), "Hello, world!", "Stdout should match");
    assert!(result.stderr.is_empty(), "Stderr should be empty");
    
    Ok(())
}

#[test]
async fn test_bash_command_with_error() -> Result<()> {
    // Create a BashExecutor
    let executor = BashExecutor::new();
    
    // Execute a command that will fail
    let params = BashParams {
        command: "ls /nonexistent_directory".to_string(),
        cwd: std::env::current_dir()?.to_string_lossy().to_string(),
    };
    
    let result = executor.execute(params).await?;
    
    // Verify the result
    assert!(!result.success, "Command should fail");
    assert_ne!(result.status, 0, "Exit status should be non-zero");
    assert!(result.stderr.contains("No such file or directory"), "Stderr should contain error message");
    
    Ok(())
}

#[test]
async fn test_bash_with_environment_variables() -> Result<()> {
    // Create a BashExecutor
    let executor = BashExecutor::new();
    
    // Execute a command that uses environment variables
    let params = BashParams {
        command: "echo $TEST_VAR".to_string(),
        cwd: std::env::current_dir()?.to_string_lossy().to_string(),
    };
    
    // Set an environment variable for the test
    std::env::set_var("TEST_VAR", "test_value");
    
    let result = executor.execute(params).await?;
    
    // Verify the result
    assert!(result.success, "Command should succeed");
    assert_eq!(result.stdout.trim(), "test_value", "Should echo the environment variable");
    
    // Remove the environment variable
    std::env::remove_var("TEST_VAR");
    
    Ok(())
}

#[test]
async fn test_bash_with_custom_working_directory() -> Result<()> {
    // Create a BashExecutor
    let executor = BashExecutor::new();
    
    // Create a temporary directory
    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path().to_string_lossy().to_string();
    
    // Execute a command in the temporary directory
    let params = BashParams {
        command: "pwd".to_string(),
        cwd: temp_path.clone(),
    };
    
    let result = executor.execute(params).await?;
    
    // Verify the result
    assert!(result.success, "Command should succeed");
    assert!(result.stdout.trim().contains(&temp_path), "Working directory should be the temporary directory");
    
    Ok(())
}

#[test]
async fn test_quick_bash_handler() -> Result<()> {
    // Test the quick_bash handler
    let params = QuickBashParams {
        cmd: "echo 'Quick bash test'".to_string(),
    };
    
    let result = handle_quick_bash(params).await?;
    
    // Verify the result
    assert!(result.success, "Quick bash command should succeed");
    assert_eq!(result.stdout.trim(), "Quick bash test", "Stdout should match");
    
    Ok(())
}

#[test]
async fn test_bash_command_with_tool_trait() -> Result<()> {
    // Create a mock bash tool
    let tool = MockBashTool::new();
    
    // Create parameters for the tool
    let params = shared_protocol_objects::CallToolParams {
        name: "bash".to_string(),
        arguments: json!({
            "command": "echo 'Testing tool trait'",
            "cwd": std::env::current_dir()?.to_string_lossy().to_string(),
        }),
    };
    
    // Execute the tool
    let response = tool.execute(params, Some(json!(1))).await?;
    
    // Verify the response
    assert!(response.error.is_none(), "Should not have an error");
    assert!(response.result.is_some(), "Should have a result");
    
    // Extract the result content
    let result_value = response.result.unwrap();
    let result: shared_protocol_objects::CallToolResult = serde_json::from_value(result_value)?;
    
    // Verify the content
    assert!(!result.content.is_empty(), "Should have content");
    assert!(result.content[0].text.contains("stdout: Testing tool trait"), "Should contain expected output");
    assert_eq!(result.is_error, Some(false), "Should not be an error result");
    
    Ok(())
}