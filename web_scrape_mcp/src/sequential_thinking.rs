use std::fs;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};
use serde_json::{json, Value};
use shared_protocol_objects::{
    JsonRpcResponse, ToolInfo,
    CallToolParams, CallToolResult,
    ToolResponseContent,
    success_response, error_response, INVALID_PARAMS,
};
use chrono;
use tracing::{debug, error, info, warn, instrument};
use crate::graph_database::DEFAULT_GRAPH_DIR;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct Thought {
    pub(crate) content: String,
    pub(crate) number: i32,
    pub(crate) total_thoughts: i32,
    needs_more_thoughts: bool,
    pub(crate) branch_id: Option<String>,
    pub(crate) branch_from_thought: Option<i32>,
    pub(crate) is_revision: bool,
    pub(crate) revises_thought: Option<i32>,
    date_created: chrono::DateTime<chrono::Utc>,
    date_modified: chrono::DateTime<chrono::Utc>,
    metadata: HashMap<String, String>,
}

impl Thought {
    fn new(content: String, number: i32, total_thoughts: i32) -> Self {
        let now = chrono::Utc::now();
        debug!("Creating new thought #{} of {}", number, total_thoughts);
        Self {
            content,
            number,
            total_thoughts,
            needs_more_thoughts: true,
            branch_id: None,
            branch_from_thought: None, 
            is_revision: false,
            revises_thought: None,
            date_created: now,
            date_modified: now,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct SequentialThinkingTool {
    thoughts: Vec<Thought>,
    branches: HashMap<String, Vec<Thought>>,
    path: std::path::PathBuf,
}

impl SequentialThinkingTool {
    pub fn new() -> Self {
        debug!("Creating new SequentialThinkingTool");
        match Self::load() {
            Ok(tool) => {
                debug!("Loaded existing tool with {} thoughts", tool.thoughts.len());
                tool
            },
            Err(e) => {
                warn!("Failed to load existing tool: {}, creating new", e);
                let thoughts_dir = std::env::var("KNOWLEDGE_GRAPH_DIR")
                    .unwrap_or_else(|_| DEFAULT_GRAPH_DIR.to_string());
                let path = std::path::PathBuf::from(&thoughts_dir);

                // Check if directory exists, create if it doesn't
                if !path.exists() {
                    debug!("Creating directory: {:?}", path);
                    std::fs::create_dir_all(&path)
                        .map_err(|e| warn!("Failed to create directory: {}", e))
                        .ok();
                }
                Self {
                    thoughts: Vec::new(),
                    branches: HashMap::new(),
                    path: std::path::PathBuf::from(thoughts_dir),
                }
            }
        }
    }

    #[instrument(skip(self))]
    pub fn add_thought(&mut self, content: String, total_thoughts: i32) -> Result<()> {
        debug!(content_len = content.len(), "Adding new thought");
        let next_number = self.thoughts.len() as i32 + 1;
        let thought = Thought::new(content, next_number, total_thoughts);
        self.thoughts.push(thought);
        debug!("Added thought #{}", next_number);
        self.save()?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn revise_thought(&mut self, content: String, revises_number: i32) -> Result<()> {
        debug!(
            content_len = content.len(),
            revises = revises_number,
            "Revising thought"
        );
        
        if !self.thoughts.iter().any(|t| t.number == revises_number) {
            error!("Cannot revise non-existent thought #{}", revises_number);
            return Err(anyhow!("Thought {} not found", revises_number));
        }

        let next_number = self.thoughts.len() as i32 + 1;
        let mut thought = Thought::new(content, next_number, self.thoughts[0].total_thoughts);
        thought.is_revision = true;
        thought.revises_thought = Some(revises_number);
        
        self.thoughts.push(thought);
        debug!("Added revision #{} for thought #{}", next_number, revises_number);
        self.save()?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn branch_thought(&mut self, content: String, branch_from: i32, branch_id: String) -> Result<()> {
        if branch_from < 1 || branch_from > self.thoughts.len() as i32 {
            return Err(anyhow!("Invalid branch_from thought number"));
        }

        let number = match self.branches.get(&branch_id) {
            Some(branch) => branch.len() as i32 + 1,
            None => 1,
        };

        let mut thought = Thought::new(
            content,
            number,
            self.thoughts[branch_from as usize - 1].total_thoughts,
        );
        thought.branch_id = Some(branch_id.clone());
        thought.branch_from_thought = Some(branch_from);

        self.branches.entry(branch_id)
            .or_insert_with(Vec::new)
            .push(thought);
        
        self.save()?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn save(&self) -> Result<()> {
        let thoughts_file = self.path.join("thoughts.json");
        debug!(path = ?thoughts_file, "Saving thoughts");
        
        let json = json!({
            "thoughts": self.thoughts,
            "branches": self.branches,
        });
        
        fs::write(&thoughts_file, serde_json::to_string_pretty(&json)?)?;
        debug!("Successfully saved {} thoughts", self.thoughts.len());
        Ok(())
    }

    #[instrument]
    pub fn load() -> Result<Self> {
        let thoughts_dir = std::env::var("KNOWLEDGE_GRAPH_DIR")
            .unwrap_or_else(|_| DEFAULT_GRAPH_DIR.to_string());
        let path = std::path::PathBuf::from(&thoughts_dir);
        let thoughts_file = path.join("thoughts.json");
        
        debug!(path = ?thoughts_file, "Loading thoughts");
        
        match fs::read_to_string(&thoughts_file) {
            Ok(contents) => {
                let json: Value = serde_json::from_str(&contents)?;
                let thoughts: Vec<Thought> = serde_json::from_value(json["thoughts"].clone())?;
                let branches: HashMap<String, Vec<Thought>> = serde_json::from_value(json["branches"].clone())?;
                debug!("Successfully loaded {} thoughts", thoughts.len());
                Ok(Self { thoughts, branches, path })
            }
            Err(e) => {
                warn!("Failed to read thoughts file: {}", e);
                Ok(Self {
                    thoughts: Vec::new(),
                    branches: HashMap::new(),
                    path,
                })
            }
        }
    }

    pub fn get_thought(&self, number: i32) -> Option<&Thought> {
        if number < 1 {
            return None;
        }
        self.thoughts.get(number as usize - 1)
    }

    pub fn get_branch(&self, branch_id: &str) -> Option<&Vec<Thought>> {
        self.branches.get(branch_id)
    }
}

#[derive(Deserialize)]
pub struct AddThoughtParams {
    pub content: String,
    pub total_thoughts: i32,
}

#[derive(Deserialize)]
pub struct ReviseThoughtParams {
    pub content: String,
    pub revises_number: i32,
}

#[derive(Deserialize)]
pub struct BranchThoughtParams {
    pub content: String,
    pub branch_from: i32,
    pub branch_id: String,
}

pub fn sequential_thinking_tool_info() -> ToolInfo {
    ToolInfo {
        name: "sequential_thinking".to_string(),
        description: Some("A tool for managing sequential thinking and thought revision process.".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform",
                    "enum": ["add_thought", "revise_thought", "branch_thought", "get_thought", "get_branch", "load"]
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the action",
                    "oneOf": [
                        {
                            "type": "object",
                            "title": "AddThoughtParams",
                    "properties": {
                                "content": {"type": "string"},
                                "total_thoughts": {"type": "integer", "minimum": 1}
                            },
                            "required": ["content", "total_thoughts"]
                        },
                        {
                            "type": "object", 
                            "title": "ReviseThoughtParams",
                            "properties": {
                                "content": {"type": "string"},
                                "revises_number": {"type": "integer", "minimum": 1}
                            },
                            "required": ["content", "revises_number"]
                        },
                        {
                            "type": "object",
                            "title": "BranchThoughtParams", 
                            "properties": {
                                "content": {"type": "string"},
                                "branch_from": {"type": "integer", "minimum": 1},
                                "branch_id": {"type": "string"}
                            },
                            "required": ["content", "branch_from", "branch_id"]
                        },
                        {
                            "type": "object",
                            "title": "GetThoughtParams",
                            "properties": {
                                "number": {"type": "integer", "minimum": 1}
                            },
                            "required": ["number"]
                        },
                        {
                            "type": "object",
                            "title": "GetBranchParams",
                            "properties": {
                                "branch_id": {"type": "string"}
                            },
                            "required": ["branch_id"]
                        }
                    ]
                }
            },
            "required": ["action", "params"]
        }),
    }
}

#[instrument(skip(params))]
pub async fn handle_sequential_thinking_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    debug!(?params, "Handling sequential thinking tool call");
    
    let mut tool = match SequentialThinkingTool::load() {
        Ok(t) => {
            debug!("Successfully loaded existing tool");
            t
        },
        Err(e) => {
            warn!("Failed to load existing tool: {}, creating new", e);
            SequentialThinkingTool::new()
        }
    };

    let action = params.arguments.get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Missing or invalid 'action' field"))?;

    debug!(action = %action, "Processing action");

    let action_params = params.arguments.get("params")
        .ok_or_else(|| anyhow!("Missing 'params' field"))?;

    let result = match action {
        "load" => {
            debug!("Loading sequential thinking tool");
            "Sequential thinking tool loaded successfully".to_string()
        }
        "add_thought" => {
            let params: AddThoughtParams = serde_json::from_value(action_params.clone())?;
            debug!(
                content_len = params.content.len(),
                total_thoughts = params.total_thoughts,
                "Adding thought"
            );
            tool.add_thought(params.content, params.total_thoughts)?;
        
            "Thought added successfully".to_string()
        },
        "revise_thought" => {
            let params: ReviseThoughtParams = serde_json::from_value(action_params.clone())?;
            debug!(
                content_len = params.content.len(),
                revises = params.revises_number,
                "Revising thought"
            );
            tool.revise_thought(params.content, params.revises_number)?;
            "Thought revised successfully".to_string()
        },
        "branch_thought" => {
            let params: BranchThoughtParams = serde_json::from_value(action_params.clone())?;
            debug!(
                content_len = params.content.len(),
                branch_from = params.branch_from,
                branch_id = %params.branch_id,
                "Creating thought branch"
            );
            tool.branch_thought(params.content, params.branch_from, params.branch_id)?;
            "Branch created successfully".to_string()
        },
        "get_thought" => {
            let number = action_params.get("number")
                .and_then(Value::as_i64)
                .ok_or_else(|| anyhow!("Missing or invalid thought number"))? as i32;
            
            debug!(thought_number = number, "Getting thought");
            match tool.get_thought(number) {
                Some(thought) => {
                    debug!("Found thought #{}", number);
                    serde_json::to_string_pretty(thought)?
                },
                None => {
                    warn!("Thought #{} not found", number);
                    "Thought not found".to_string()
                }
            }
        },
        "get_branch" => {
            let branch_id = action_params.get("branch_id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("Missing or invalid branch_id"))?;
            
            debug!(branch_id = %branch_id, "Getting branch");
            match tool.get_branch(branch_id) {
                Some(branch) => {
                    debug!("Found branch {} with {} thoughts", branch_id, branch.len());
                    serde_json::to_string_pretty(branch)?
                },
                None => {
                    warn!("Branch {} not found", branch_id);
                    "Branch not found".to_string()
                }
            }
        },
        _ => {
            error!(action = %action, "Invalid action");
            return Ok(error_response(id, INVALID_PARAMS, "Invalid action"));
        }
    };

    debug!(action = %action, "Successfully handled action");
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

    Ok(success_response(id, json!(tool_res)))
}