use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::test;
use tokio::time::timeout;

use mcp_tools::tool_trait::{Tool, ExecuteFuture};

// Mock tool that tracks execution order and timing
struct DelayedTool {
    name: String,
    delay_ms: u64,
    execution_order: Arc<Mutex<Vec<String>>>,
}

impl DelayedTool {
    fn new(name: &str, delay_ms: u64, execution_order: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            name: name.to_string(),
            delay_ms,
            execution_order,
        }
    }
}

impl Tool for DelayedTool {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        shared_protocol_objects::ToolInfo {
            name: self.name.clone(),
            description: Some(format!("Mock tool that delays for {} ms", self.delay_ms)),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }
    
    fn execute(&self, params: shared_protocol_objects::CallToolParams, id: Option<serde_json::Value>) -> ExecuteFuture {
        let name = self.name.clone();
        let delay = self.delay_ms;
        let exec_order = Arc::clone(&self.execution_order);
        
        Box::pin(async move {
            // Record start of execution
            {
                let mut order = exec_order.lock().unwrap();
                order.push(format!("start:{}", name));
            }
            
            // Simulate work with delay
            tokio::time::sleep(Duration::from_millis(delay)).await;
            
            // Record end of execution
            {
                let mut order = exec_order.lock().unwrap();
                order.push(format!("end:{}", name));
            }
            
            // Return success response
            let content = shared_protocol_objects::ToolResponseContent {
                type_: "text".to_string(),
                text: format!("Tool {} executed with delay {}", name, delay),
                annotations: None,
            };
            
            let result = shared_protocol_objects::CallToolResult {
                content: vec![content],
                is_error: None,
                _meta: None,
                progress: None,
                total: None,
            };
            
            Ok(shared_protocol_objects::success_response(Some(id.unwrap_or(json!(null))), json!(result)))
        })
    }
}

#[test]
async fn test_concurrent_tool_execution() -> Result<()> {
    // Create shared execution order tracker
    let execution_order = Arc::new(Mutex::new(Vec::new()));
    
    // Create tools with different delays
    let tool1 = DelayedTool::new("tool1", 100, Arc::clone(&execution_order));
    let tool2 = DelayedTool::new("tool2", 50, Arc::clone(&execution_order));
    let tool3 = DelayedTool::new("tool3", 150, Arc::clone(&execution_order));
    
    // Create parameters
    let params1 = shared_protocol_objects::CallToolParams {
        name: "tool1".to_string(),
        arguments: json!({}),
    };
    
    let params2 = shared_protocol_objects::CallToolParams {
        name: "tool2".to_string(),
        arguments: json!({}),
    };
    
    let params3 = shared_protocol_objects::CallToolParams {
        name: "tool3".to_string(),
        arguments: json!({}),
    };
    
    // Execute tools concurrently
    let future1 = tool1.execute(params1, Some(json!(1)));
    let future2 = tool2.execute(params2, Some(json!(2)));
    let future3 = tool3.execute(params3, Some(json!(3)));
    
    // Wait for all futures
    let (result1, result2, result3) = tokio::join!(future1, future2, future3);
    
    // Verify all executions succeeded
    assert!(result1.is_ok(), "Tool 1 should execute successfully");
    assert!(result2.is_ok(), "Tool 2 should execute successfully");
    assert!(result3.is_ok(), "Tool 3 should execute successfully");
    
    // Get execution order
    let order = execution_order.lock().unwrap().clone();
    
    // Verify execution started in the order called
    assert_eq!(order[0], "start:tool1", "Tool 1 should start first");
    assert_eq!(order[1], "start:tool2", "Tool 2 should start second");
    assert_eq!(order[2], "start:tool3", "Tool 3 should start third");
    
    // Verify completion order (based on delay: tool2 < tool1 < tool3)
    assert_eq!(order[3], "end:tool2", "Tool 2 should finish first (shortest delay)");
    assert_eq!(order[4], "end:tool1", "Tool 1 should finish second");
    assert_eq!(order[5], "end:tool3", "Tool 3 should finish last (longest delay)");
    
    Ok(())
}

#[test]
async fn test_tool_timeout_handling() -> Result<()> {
    // Create shared execution order tracker
    let execution_order = Arc::new(Mutex::new(Vec::new()));
    
    // Create a tool with a long delay
    let long_tool = DelayedTool::new("long_tool", 500, Arc::clone(&execution_order));
    
    // Create parameters
    let params = shared_protocol_objects::CallToolParams {
        name: "long_tool".to_string(),
        arguments: json!({}),
    };
    
    // Execute with a short timeout
    let future = long_tool.execute(params, Some(json!(1)));
    let timeout_result = timeout(Duration::from_millis(200), future).await;
    
    // Verify timeout occurred
    assert!(timeout_result.is_err(), "Operation should time out");
    
    // Wait for the tool to actually complete its execution
    tokio::time::sleep(Duration::from_millis(400)).await;
    
    // Verify execution order still shows start and end despite timeout
    let order = execution_order.lock().unwrap().clone();
    assert_eq!(order[0], "start:long_tool", "Tool should have started");
    assert_eq!(order[1], "end:long_tool", "Tool should have completed");
    
    Ok(())
}

#[test]
async fn test_multiple_tool_batch() -> Result<()> {
    // Create shared execution order tracker
    let execution_order = Arc::new(Mutex::new(Vec::new()));
    
    // Create similar tools with different execution times
    let tools = vec![
        DelayedTool::new("batch1", 80, Arc::clone(&execution_order)),
        DelayedTool::new("batch2", 40, Arc::clone(&execution_order)),
        DelayedTool::new("batch3", 60, Arc::clone(&execution_order)),
        DelayedTool::new("batch4", 20, Arc::clone(&execution_order)),
        DelayedTool::new("batch5", 100, Arc::clone(&execution_order)),
    ];
    
    // Create a vector of futures
    let mut futures = Vec::new();
    
    // Launch all tools
    for (i, tool) in tools.iter().enumerate() {
        let params = shared_protocol_objects::CallToolParams {
            name: tool.name().to_string(),
            arguments: json!({}),
        };
        
        futures.push(tool.execute(params, Some(json!(i))));
    }
    
    // Wait for all futures to complete
    let results = futures::future::join_all(futures).await;
    
    // Verify all operations succeeded
    for result in &results {
        assert!(result.is_ok(), "All tool executions should succeed");
    }
    
    // Expected end order based on delays
    let expected_end_order = vec![
        "end:batch4", // 20ms
        "end:batch2", // 40ms
        "end:batch3", // 60ms
        "end:batch1", // 80ms
        "end:batch5", // 100ms
    ];
    
    // Filter only end events
    let order = execution_order.lock().unwrap().clone();
    let end_events: Vec<String> = order.iter()
        .filter(|e| e.starts_with("end:"))
        .cloned()
        .collect();
    
    // Verify end order matches expected
    assert_eq!(end_events, expected_end_order, "Tools should finish in order of delay time");
    
    Ok(())
}

// Test tool with internal concurrency
struct ConcurrentTool {
    name: String,
}

impl ConcurrentTool {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
    
    async fn parallel_work(&self, count: usize) -> Vec<String> {
        let mut futures = Vec::new();
        
        for i in 0..count {
            futures.push(tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                format!("Result from task {}", i)
            }));
        }
        
        let mut results = Vec::new();
        for future in futures {
            if let Ok(result) = future.await {
                results.push(result);
            }
        }
        
        results
    }
}

impl Tool for ConcurrentTool {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn info(&self) -> shared_protocol_objects::ToolInfo {
        shared_protocol_objects::ToolInfo {
            name: self.name.clone(),
            description: Some("Tool that performs concurrent internal operations".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_count": {"type": "number"}
                }
            }),
        }
    }
    
    fn execute(&self, params: shared_protocol_objects::CallToolParams, id: Option<serde_json::Value>) -> ExecuteFuture {
        let name = self.name.clone();
        let this = self.clone();
        
        Box::pin(async move {
            // Get task count from params
            let count = params.arguments.get("task_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(3) as usize;
                
            // Run parallel tasks
            let results = this.parallel_work(count).await;
            
            // Return response with all results
            let content = shared_protocol_objects::ToolResponseContent {
                type_: "text".to_string(),
                text: format!("Tool {} executed {} concurrent tasks:\n{}", 
                    name, count, results.join("\n")),
                annotations: None,
            };
            
            let result = shared_protocol_objects::CallToolResult {
                content: vec![content],
                is_error: None,
                _meta: None,
                progress: None,
                total: None,
            };
            
            Ok(shared_protocol_objects::success_response(Some(id.unwrap_or(json!(null))), json!(result)))
        })
    }
}

impl Clone for ConcurrentTool {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
        }
    }
}

#[test]
async fn test_internal_concurrency() -> Result<()> {
    // Create a tool with internal concurrency
    let tool = ConcurrentTool::new("concurrent_tool");
    
    // Create parameters
    let params = shared_protocol_objects::CallToolParams {
        name: "concurrent_tool".to_string(),
        arguments: json!({"task_count": 5}),
    };
    
    // Measure execution time
    let start = std::time::Instant::now();
    
    // Execute the tool
    let result = tool.execute(params, Some(json!(1))).await?;
    
    // Get the duration
    let duration = start.elapsed();
    
    // The tool runs 5 tasks in parallel, each taking 50ms
    // So it should complete in slightly more than 50ms, not 5 * 50 = 250ms
    assert!(duration.as_millis() < 150, "Concurrent tasks should execute in parallel");
    
    // Verify the result
    let result_value = result.result.unwrap();
    let tool_result: shared_protocol_objects::CallToolResult = serde_json::from_value(result_value)?;
    assert!(tool_result.content[0].text.contains("5 concurrent tasks"), "Result should mention 5 tasks");
    
    Ok(())
}