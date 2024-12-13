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
use crate::graph_database::{GraphManager, DataNode};

#[derive(Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

#[derive(Serialize, Deserialize, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub estimated_hours: Option<f32>,
    pub actual_hours: Option<f32>,
    pub dependencies: Vec<String>,  // Task IDs
    pub blockers: Vec<String>,      // Description of blockers
    pub tags: Vec<String>,
    pub project: String,
    pub metadata: HashMap<String, String>,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: "".to_string(),
            title: "".to_string(),
            description: "".to_string(),
            status: TaskStatus::NotStarted,
            priority: TaskPriority::Low,
            estimated_hours: None,
            actual_hours: None,
            dependencies: vec![],
            blockers: vec![],
            tags: vec![],
            project: "".to_string(),
            metadata: Default::default(),
        }
    }
}

impl Task {
    fn to_node(&self) -> DataNode {
        let mut node = DataNode::new(
            self.id.clone(),
            self.description.clone(),
            self.title.clone(),
        );
        
        node.metadata.insert("status".to_string(), format!("{:?}", self.status));
        node.metadata.insert("priority".to_string(), format!("{:?}", self.priority));
        if let Some(est) = self.estimated_hours {
            node.metadata.insert("estimated_hours".to_string(), est.to_string());
        }
        if let Some(actual) = self.actual_hours {
            node.metadata.insert("actual_hours".to_string(), actual.to_string());
        }
        node.metadata.insert("project".to_string(), self.project.clone());
        node.metadata.extend(self.metadata.clone());
        node.tags = self.tags.clone();
        
        node
    }

    fn from_node(node: &DataNode) -> Result<Self> {
        Ok(Task {
            id: node.name.clone(),
            title: node.description.clone(),
            description: node.content.clone(),
            status: node.metadata.get("status")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(TaskStatus::NotStarted),
            priority: node.metadata.get("priority")
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(TaskPriority::Medium),
            estimated_hours: node.metadata.get("estimated_hours")
                .and_then(|s| s.parse().ok()),
            actual_hours: node.metadata.get("actual_hours")
                .and_then(|s| s.parse().ok()),
            dependencies: Vec::new(), // Will be populated from graph edges
            blockers: node.metadata.get("blockers")
                .map(|s| s.split(',').map(String::from).collect())
                .unwrap_or_default(),
            tags: node.tags.clone(),
            project: node.metadata.get("project")
                .cloned()
                .unwrap_or_default(),
            metadata: node.metadata.clone(),
        })
    }
}

pub struct TaskPlanningTool {
    graph: GraphManager,
}

#[derive(Deserialize)]
pub(crate) struct CreateTaskParams {
    title: String,
    description: String,
    priority: TaskPriority,
    estimated_hours: Option<f32>,
    dependencies: Vec<String>,
    project: String,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}

#[derive(Deserialize)]
pub(crate) struct UpdateTaskParams {
    task_id: String,
    status: Option<TaskStatus>,
    priority: Option<TaskPriority>,
    actual_hours: Option<f32>,
    blockers: Option<Vec<String>>,
    metadata: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
pub(crate) struct AddDependencyParams {
    from_task: String,
    to_task: String,
    dependency_type: String,
}

#[derive(Deserialize)]
pub(crate) struct GetProjectTasksParams {
    project: String,
    status: Option<TaskStatus>,
    priority: Option<TaskPriority>,
}

#[derive(Deserialize)]
pub(crate) struct SearchTasksParams {
    query: String,
    project: Option<String>,
    tags: Option<Vec<String>>,
}

impl TaskPlanningTool {
    pub fn new() -> Self {
        Self {
            graph: GraphManager::new("task_planning.json".to_string()),
        }
    }

    pub async fn create_task(&mut self, params: CreateTaskParams) -> Result<String> {
        let task_id = format!("task_{}", uuid::Uuid::new_v4());
        let task = Task {
            id: task_id.clone(),
            title: params.title,
            description: params.description,
            status: TaskStatus::NotStarted,
            priority: params.priority,
            estimated_hours: params.estimated_hours,
            actual_hours: None,
            dependencies: params.dependencies,
            blockers: Vec::new(),
            tags: params.tags,
            project: params.project,
            metadata: params.metadata,
        };

        let node = task.to_node();
        
        // Create the task node
        if self.graph.root.is_none() {
            self.graph.create_root(node).await?;
        } else {
            let root = self.graph.root.unwrap();
            self.graph.create_connected_node(node, root, "is_task".to_string()).await?;
        }

        // Add dependencies
        for dep_id in task.dependencies {
            if let Some((dep_idx, _)) = self.graph.get_node_by_name(&dep_id) {
                let task_idx = self.graph.get_node_by_name(&task_id)
                    .ok_or_else(|| anyhow!("Failed to get created task node"))?
                    .0;
                self.graph.connect(dep_idx, task_idx, "blocks".to_string()).await?;
            }
        }

        Ok(format!("Task created with ID: {}", task_id))
    }

    pub async fn update_task(&mut self, params: UpdateTaskParams) -> Result<String> {
        let (node_idx, node) = self.graph.get_node_by_name(&params.task_id)
            .ok_or_else(|| anyhow!("Task not found"))?;
        
        let mut task = Task::from_node(node)?;
        
        if let Some(status) = params.status {
            task.status = status;
        }
        if let Some(priority) = params.priority {
            task.priority = priority;
        }
        if let Some(hours) = params.actual_hours {
            task.actual_hours = Some(hours);
        }
        if let Some(blockers) = params.blockers {
            task.blockers = blockers;
        }
        if let Some(metadata) = params.metadata {
            task.metadata.extend(metadata);
        }

        self.graph.update_node(node_idx, task.to_node()).await?;
        
        Ok("Task updated successfully".to_string())
    }

    pub async fn add_dependency(&mut self, params: AddDependencyParams) -> Result<String> {
        let from_idx = self.graph.get_node_by_name(&params.from_task)
            .ok_or_else(|| anyhow!("From task not found"))?
            .0;
        let to_idx = self.graph.get_node_by_name(&params.to_task)
            .ok_or_else(|| anyhow!("To task not found"))?
            .0;

        self.graph.connect(from_idx, to_idx, params.dependency_type).await?;
        
        Ok("Dependency added successfully".to_string())
    }

    pub async fn get_project_tasks(&self, params: GetProjectTasksParams) -> Result<String> {
        let mut tasks = Vec::new();
        
        for idx in self.graph.node_indices() {
            if let Some(node) = self.graph.get_node(idx) {
                if node.metadata.get("project") == Some(&params.project) {
                    let task = Task::from_node(node)?;
                    
                    // Filter by status and priority if specified
                    if let Some(status) = &params.status {
                        if task.status != *status {
                            continue;
                        }
                    }
                    if let Some(priority) = &params.priority {
                        if task.priority != *priority {
                            continue;
                        }
                    }
                    
                    tasks.push(task);
                }
            }
        }

        Ok(serde_json::to_string_pretty(&tasks)?)
    }

    pub async fn search_tasks(&self, params: SearchTasksParams) -> Result<String> {
        let mut tasks = Vec::new();
        
        let search_results = self.graph.search_nodes(&params.query);
        for (_, node) in search_results {
            let task = Task::from_node(node)?;
            
            // Filter by project if specified
            if let Some(project) = &params.project {
                if task.project != *project {
                    continue;
                }
            }
            
            // Filter by tags if specified
            if let Some(tags) = &params.tags {
                if !tags.iter().all(|tag| task.tags.contains(tag)) {
                    continue;
                }
            }
            
            tasks.push(task);
        }

        Ok(serde_json::to_string_pretty(&tasks)?)
    }
}

pub fn task_planning_tool_info() -> ToolInfo {
    ToolInfo {
        name: "task_planning".to_string(),
        description: Some(
            "A comprehensive project and task management system for organizing, tracking, and executing work efficiently.

            **When to Use:**
            - Planning new projects or initiatives
            - Breaking down complex work into manageable tasks
            - Managing dependencies between different pieces of work
            - Tracking progress on ongoing activities
            - Prioritizing multiple competing tasks
            - Organizing collaborative work efforts
            
            **Key Features:**
            - Task creation with rich metadata
            - Priority levels (Low, Medium, High, Critical)
            - Status tracking (NotStarted, InProgress, Blocked, Completed, Cancelled)
            - Dependency management
            - Time estimation and tracking
            - Project organization
            - Tagging system
            
            **Best Practices:**
            1. Create clear, actionable task descriptions
            2. Set realistic time estimates
            3. Establish explicit dependencies
            4. Use appropriate priority levels
            5. Keep status information current
            6. Tag tasks for easy filtering
            
            **Task Lifecycle:**
            1. Creation with initial scope
            2. Priority and time estimation
            3. Dependency mapping
            4. Progress tracking
            5. Status updates
            6. Completion or cancellation
            
            **Integration Points:**
            - Uses memory tool for historical context
            - Works with graph_tool for dependency visualization
            - Connects with sequential_thinking for task breakdown
            ".to_string()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform",
                    "enum": ["create_task", "update_task", "add_dependency", "get_project_tasks", "search_tasks"]
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the action",
                    "oneOf": [
                        {
                            "type": "object",
                            "title": "CreateTaskParams",
                            "properties": {
                                "title": {"type": "string"},
                                "description": {"type": "string"},
                                "priority": {
                                    "type": "string",
                                    "enum": ["Low", "Medium", "High", "Critical"]
                                },
                                "estimated_hours": {"type": "number"},
                                "dependencies": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                },
                                "project": {"type": "string"},
                                "tags": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                },
                                "metadata": {
                                    "type": "object",
                                    "additionalProperties": {"type": "string"}
                                }
                            },
                            "required": ["title", "description", "project"]
                        },
                        {
                            "type": "object",
                            "title": "UpdateTaskParams",
                            "properties": {
                                "task_id": {"type": "string"},
                                "status": {
                                    "type": "string",
                                    "enum": ["NotStarted", "InProgress", "Blocked", "Completed", "Cancelled"]
                                },
                                "priority": {
                                    "type": "string",
                                    "enum": ["Low", "Medium", "High", "Critical"]
                                },
                                "actual_hours": {"type": "number"},
                                "blockers": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                },
                                "metadata": {
                                    "type": "object",
                                    "additionalProperties": {"type": "string"}
                                }
                            },
                            "required": ["task_id"]
                        },
                        {
                            "type": "object",
                            "title": "AddDependencyParams",
                            "properties": {
                                "from_task": {"type": "string"},
                                "to_task": {"type": "string"},
                                "dependency_type": {"type": "string"}
                            },
                            "required": ["from_task", "to_task", "dependency_type"]
                        },
                        {
                            "type": "object",
                            "title": "GetProjectTasksParams",
                            "properties": {
                                "project": {"type": "string"},
                                "status": {
                                    "type": "string",
                                    "enum": ["NotStarted", "InProgress", "Blocked", "Completed", "Cancelled"]
                                },
                                "priority": {
                                    "type": "string",
                                    "enum": ["Low", "Medium", "High", "Critical"]
                                }
                            },
                            "required": ["project"]
                        },
                        {
                            "type": "object",
                            "title": "SearchTasksParams",
                            "properties": {
                                "query": {"type": "string"},
                                "project": {"type": "string"},
                                "tags": {
                                    "type": "array",
                                    "items": {"type": "string"}
                                }
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

pub async fn handle_task_planning_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let mut tool = TaskPlanningTool::new();

    let action = params.arguments.get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Missing or invalid 'action' field"))?;

    let action_params = params.arguments.get("params")
        .ok_or_else(|| anyhow!("Missing 'params' field"))?;

    let result = match action {
        "create_task" => {
            let params: CreateTaskParams = serde_json::from_value(action_params.clone())?;
            tool.create_task(params).await?
        },
        "update_task" => {
            let params: UpdateTaskParams = serde_json::from_value(action_params.clone())?;
            tool.update_task(params).await?
        },
        "add_dependency" => {
            let params: AddDependencyParams = serde_json::from_value(action_params.clone())?;
            tool.add_dependency(params).await?
        },
        "get_project_tasks" => {
            let params: GetProjectTasksParams = serde_json::from_value(action_params.clone())?;
            tool.get_project_tasks(params).await?
        },
        "search_tasks" => {
            let params: SearchTasksParams = serde_json::from_value(action_params.clone())?;
            tool.search_tasks(params).await?
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
