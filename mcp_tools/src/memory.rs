use crate::graph_database::{DataNode, GraphManager};
use crate::sequential_thinking::SequentialThinkingTool;
use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use shared_protocol_objects::{
    error_response, success_response,
    CallToolParams, CallToolResult,
    JsonRpcResponse,
    ToolInfo, ToolResponseContent, INVALID_PARAMS,
};
use std::collections::HashMap;

pub struct MemoryTool {
    graph: GraphManager,
    thinking: SequentialThinkingTool,
}

#[derive(Deserialize)]
struct MemorizeThoughtParams {
    thought_number: i32,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ConnectThoughtsParams {
    from_thought: i32,
    to_thought: i32,
    relation: String,
}

#[derive(Deserialize)]
struct SearchMemoryParams {
    query: String,
    include_thoughts: bool,
}

impl MemoryTool {
    pub fn new() -> Self {
        Self {
            graph: GraphManager::new("memories.json".to_string()),
            thinking: SequentialThinkingTool::new(),
        }
    }

    pub async fn memorize_thought(&mut self, params: MemorizeThoughtParams) -> Result<String> {
        let thought = self.thinking.get_thought(params.thought_number)
            .ok_or_else(|| anyhow!("Thought not found"))?;

        // Create a node in the graph from the thought
        let mut node = DataNode::new(
            format!("thought_{}", thought.number),
            thought.content.clone(),
            format!("Thought #{} from sequential thinking", thought.number),
        );

        // Add metadata
        node.metadata.extend(params.metadata);
        node.tags.extend(params.tags);

        // Add thought-specific metadata
        node.metadata.insert("thought_number".to_string(), thought.number.to_string());
        node.metadata.insert("total_thoughts".to_string(), thought.total_thoughts.to_string());
        if thought.is_revision {
            node.metadata.insert("revises_thought".to_string(),
                thought.revises_thought.unwrap_or(0).to_string());
        }
        if let Some(branch_id) = &thought.branch_id {
            node.metadata.insert("branch_id".to_string(), branch_id.clone());
            node.metadata.insert("branch_from_thought".to_string(),
                thought.branch_from_thought.unwrap_or(0).to_string());
        }

        // Create the node in the graph
        if self.graph.root.is_none() {
            self.graph.create_root(node).await?;
        } else {
            let root = self.graph.root.unwrap();
            self.graph.create_connected_node(node, root, "is_thought".to_string()).await?;
        }

        Ok("Thought successfully memorized".to_string())
    }

    pub async fn connect_thoughts(&mut self, params: ConnectThoughtsParams) -> Result<String> {
        let from_node_name = format!("thought_{}", params.from_thought);
        let to_node_name = format!("thought_{}", params.to_thought);

        // Find the nodes
        let from_idx = self.graph.get_node_by_name(&from_node_name)
            .ok_or_else(|| anyhow!("Source thought node not found"))?.0;
        let to_idx = self.graph.get_node_by_name(&to_node_name)
            .ok_or_else(|| anyhow!("Target thought node not found"))?.0;

        // Connect them
        self.graph.connect(from_idx, to_idx, params.relation).await?;

        Ok("Thoughts successfully connected".to_string())
    }

    pub async fn search_memory(&self, params: SearchMemoryParams) -> Result<String> {
        let mut results = Vec::new();

        // Search in graph nodes
        let graph_results = self.graph.search_nodes(&params.query);
        for (idx, node) in graph_results {
            results.push(json!({
                "type": "memory",
                "name": node.name,
                "content": node.content,
                "description": node.description,
                "metadata": node.metadata,
                "tags": node.tags,
            }));
        }

        // Optionally search in thoughts
        if params.include_thoughts {
            // Note: We would need to add a search method to SequentialThinkingTool
            // For now, we'll just note that we need to implement this
            results.push(json!({
                "note": "Searching in thoughts not yet implemented"
            }));
        }

        Ok(serde_json::to_string_pretty(&results)?)
    }
}

pub fn memory_tool_info() -> ToolInfo {
    ToolInfo {
        name: "memory".to_string(),
        description: Some(
            "An advanced knowledge management tool for storing, connecting, and retrieving information long-term.

            **When to Use:**
            - To preserve important insights and discoveries
            - For building a knowledge base of user preferences
            - To maintain context across multiple conversations
            - When tracking the evolution of ideas over time
            - For connecting related pieces of information
            - To build a searchable archive of past interactions
            
            **Key Features:**
            - Permanent storage of thoughts and insights
            - Relationship mapping between pieces of information
            - Tagging system for easy categorization
            - Full-text search capabilities
            - Metadata tracking for context
            
            **Best Practices:**
            1. Always add relevant tags for better retrieval
            2. Include detailed metadata for context
            3. Create meaningful connections between related memories
            4. Use specific, descriptive names for nodes
            5. Regular updates to existing memories as new info emerges
            
            **Integration Points:**
            - Works with sequential_thinking to store thought processes
            - Connects with graph_tool for relationship visualization
            - Supports task_planning for historical context
            
            **Search Strategy:**
            1. Use specific keywords from current context
            2. Consider synonyms and related terms
            3. Leverage tags for categorical searches
            4. Examine connected memories for broader context
            ".to_string()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform",
                    "enum": ["memorize_thought", "connect_thoughts", "search_memory"]
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the action",
                    "oneOf": [
                        {
                            "type": "object",
                            "title": "MemorizeThoughtParams",
                            "properties": {
                                "thought_number": {"type": "integer", "minimum": 1},
                                "tags": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                },
                                "metadata": {
                                    "type": "object",
                                    "additionalProperties": {"type": "string"}
                                }
                            },
                            "required": ["thought_number"]
                        },
                        {
                            "type": "object",
                            "title": "ConnectThoughtsParams",
                            "properties": {
                                "from_thought": {"type": "integer", "minimum": 1},
                                "to_thought": {"type": "integer", "minimum": 1},
                                "relation": {"type": "string"}
                            },
                            "required": ["from_thought", "to_thought", "relation"]
                        },
                        {
                            "type": "object",
                            "title": "SearchMemoryParams",
                            "properties": {
                                "query": {"type": "string"},
                                "include_thoughts": {"type": "boolean", "default": true}
                            },
                            "required": ["query"]
                        }
                    ]
                }
            },
            "required": ["action", "params"]
        }),
    }
}

pub async fn handle_memory_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let mut tool = MemoryTool::new();

    let action = params.arguments.get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Missing or invalid 'action' field"))?;

    let action_params = params.arguments.get("params")
        .ok_or_else(|| anyhow!("Missing 'params' field"))?;

    let result = match action {
        "memorize_thought" => {
            let params: MemorizeThoughtParams = serde_json::from_value(action_params.clone())?;
            tool.memorize_thought(params).await?
        },
        "connect_thoughts" => {
            let params: ConnectThoughtsParams = serde_json::from_value(action_params.clone())?;
            tool.connect_thoughts(params).await?
        },
        "search_memory" => {
            let params: SearchMemoryParams = serde_json::from_value(action_params.clone())?;
            tool.search_memory(params).await?
        },
        _ => return Ok(error_response(id, INVALID_PARAMS, "Invalid action")),
    };

    let tool_res = CallToolResult {
        content: vec![ToolResponseContent {
            type_: "text".into(),
            text: result,
            annotations: None,
        }],
        is_error: None,
        _meta: None,
        progress: None,
        total: None,
    };

    Ok(success_response(id, serde_json::to_value(tool_res)?))
}
