use std::fs;
use std::collections::HashMap;
use serde::{Serialize, Deserialize}; 
use petgraph::Graph;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use serde::ser::{Serializer, SerializeStruct};
use serde::de::Deserializer;
use serde_json::{json, Value};
use anyhow::{Result, anyhow};
use shared_protocol_objects::{
    JsonRpcResponse,
    CallToolParams, CallToolResult,
    ToolResponseContent,
    success_response, error_response, INVALID_PARAMS, INTERNAL_ERROR
};

#[derive(Serialize, Deserialize, Clone)]
struct DataNode {
    name: String,
    description: String,
    content: String,
    #[serde(default)]
    metadata: HashMap<String, String>,
    #[serde(default)]
    tags: Vec<String>, 
}

impl DataNode {
    fn new(name: String, description: String, content: String) -> Self {
        DataNode {
            name,
            description,
            content,
            metadata: HashMap::new(),
            tags: Vec::new()
        }
    }
}

// Custom serialization for Graph
// We'll use petgraph's built-in serde support instead of custom impls

#[derive(Serialize, Deserialize)]
struct SerializableGraph {
    nodes: Vec<(NodeIndex, DataNode)>,
    edges: Vec<(NodeIndex, NodeIndex, String)>
}

pub struct GraphManager {
    graph: Graph<DataNode, String>, 
    root: Option<NodeIndex>,
    path: std::path::PathBuf,
}

impl GraphManager {
    fn node_name_exists(&self, name: &str) -> bool {
        self.graph.node_indices().any(|idx| {
            self.graph.node_weight(idx)
                .map(|node| node.name == name)
                .unwrap_or(false)
        })
    }

    pub fn new(filename: String) -> Self {
        let path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(filename);
            
        let graph = if let Ok(data) = fs::read_to_string(&path) {
            let serializable: SerializableGraph = serde_json::from_str(&data)
                .unwrap_or_else(|_| SerializableGraph { 
                    nodes: vec![], 
                    edges: vec![] 
                });
            
            let mut graph = Graph::new();
            // Restore nodes
            for (idx, node) in serializable.nodes {
                while graph.node_count() <= idx.index() {
                    graph.add_node(DataNode::new(
                        String::new(),
                        String::new(),
                        String::new()
                    ));
                }
                graph[idx] = node;
            }
            // Restore edges
            for (from, to, weight) in serializable.edges {
                graph.add_edge(from, to, weight);
            }
            graph
        } else {
            Graph::new()
        };
        
        let root = graph.node_indices().find(|&i| {
            graph.edges_directed(i, petgraph::Direction::Incoming).count() == 0
        });
        Self { graph, root, path: path.to_owned() }
    }
    

    fn save(&self) -> Result<()> {
        // Convert to serializable format
        let serializable = SerializableGraph {
            nodes: self.graph.node_indices()
                .map(|idx| (idx, self.graph[idx].clone()))
                .collect(),
            edges: self.graph.edge_indices()
                .map(|idx| {
                    let (a, b) = self.graph.edge_endpoints(idx).unwrap();
                    (a, b, self.graph[idx].clone())
                })
                .collect()
        };
        let json = serde_json::to_string(&serializable)?;
        fs::write(&self.path, json).map_err(|e| anyhow!("Failed to write graph file: {}", e))?;
        Ok(())
    }

    fn create_root(&mut self, node: DataNode) -> Result<NodeIndex> {
        if self.root.is_some() {
            return Err(anyhow!("Root already exists"));
        }
        if self.node_name_exists(&node.name) {
            return Err(anyhow!("A node with this name already exists"));
        }
        let idx = self.graph.add_node(node);
        self.root = Some(idx);
        self.save()?;
        Ok(idx)
    }

    fn create_connected_node(&mut self, node: DataNode, parent: NodeIndex, rel: String) -> Result<NodeIndex> {
        if !self.graph.node_weight(parent).is_some() {
            return Err(anyhow!("Parent node not found"));
        }
        if self.node_name_exists(&node.name) {
            return Err(anyhow!("A node with this name already exists"));
        }
        let idx = self.graph.add_node(node);
        self.graph.add_edge(parent, idx, rel);
        self.save()?;
        Ok(idx)
    }

    fn update_node(&mut self, idx: NodeIndex, node: DataNode) -> Result<()> {
        // Only check for name uniqueness if the name is actually changing
        if let Some(current) = self.graph.node_weight(idx) {
            if current.name != node.name && self.node_name_exists(&node.name) {
                return Err(anyhow!("A node with this name already exists"));
            }
        }
        if let Some(n) = self.graph.node_weight_mut(idx) {
            *n = node;
            self.save()?;
        }
        Ok(())
    }

    fn delete_node(&mut self, idx: NodeIndex) -> Result<()> {
        if Some(idx) == self.root {
            return Err(anyhow!("Cannot delete root"));
        }
        // Check if deletion would create isolated nodes (excluding the root)
        let neighbors: Vec<_> = self.graph.neighbors(idx).collect();
        let incoming: Vec<_> = self.graph.neighbors_directed(idx, petgraph::Direction::Incoming).collect();
        
        if neighbors.len() == 1 && incoming.len() == 1 {
            self.graph.remove_node(idx);
            self.save()?;
            Ok(())
        } else {
            Err(anyhow!("Deletion would create isolated nodes or disconnect graph"))
        }
    }
    

    fn get_node(&self, idx: NodeIndex) -> Option<&DataNode> {
        self.graph.node_weight(idx)
    }

    fn connect(&mut self, from: NodeIndex, to: NodeIndex, rel: String) -> Result<()> {
        self.graph.add_edge(from, to, rel);
        Ok(self.save()?)
    }

    // Method to get a node by its name
    fn get_node_by_name(&self, name: &str) -> Option<(NodeIndex, &DataNode)> {
        self.graph.node_indices()
            .find_map(|idx| {
                self.graph.node_weight(idx).and_then(|node| {
                    if node.name == name {
                        Some((idx, node))
                    } else {
                        None
                    }
                })
            })
    }

    // Method to get all immediate children of a node
    fn get_children(&self, parent: NodeIndex) -> Vec<(NodeIndex, &DataNode, String)> {
        let mut children = Vec::new();
        for edge in self.graph.edges(parent) {
            let child_idx = edge.target();
            if let Some(child_node) = self.graph.node_weight(child_idx) {
                children.push((child_idx, child_node, edge.weight().clone()));
            }
        }
        children
    }

    // Method to get all nodes matching a tag
    fn get_nodes_by_tag(&self, tag: &str) -> Vec<(NodeIndex, &DataNode)> {
        self.graph.node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).and_then(|node| {
                    if node.tags.contains(&tag.to_string()) {
                        Some((idx, node))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    // Method to get all nodes with names or descriptions matching a query string
    fn search_nodes(&self, query: &str) -> Vec<(NodeIndex, &DataNode)> {
        self.graph.node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).and_then(|node| {
                    if node.name.contains(query) || node.description.contains(query) {
                        Some((idx, node))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    fn get_most_connected_nodes(&self, limit: usize) -> Vec<(NodeIndex, &DataNode, usize)> {
        let mut nodes: Vec<_> = self.graph.node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).map(|node| {
                    // Count both incoming and outgoing edges
                    let edge_count = self.graph.edges_directed(idx, petgraph::Direction::Incoming).count() +
                                   self.graph.edges_directed(idx, petgraph::Direction::Outgoing).count();
                    (idx, node, edge_count)
                })
            })
            .collect();
        
        // Sort by edge count in descending order
        nodes.sort_by(|a, b| b.2.cmp(&a.2));
        nodes.truncate(limit);
        nodes
    }

    fn get_top_tags(&self, limit: usize) -> Vec<(String, usize)> {
        // Create a HashMap to count tag occurrences
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        
        // Count occurrences of each tag
        for node in self.graph.node_weights() {
            for tag in &node.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        
        // Convert to vector and sort by count
        let mut tag_vec: Vec<_> = tag_counts.into_iter().collect();
        tag_vec.sort_by(|a, b| b.1.cmp(&a.1));
        tag_vec.truncate(limit);
        tag_vec
    }
}

// Parameters for creating a new node
#[derive(Deserialize)]
struct CreateNodeParams {
    name: String,
    description: String,
    content: String,
    parent_name: Option<String>, // Use name instead of index for a more user-friendly API
    relation: Option<String>,
    tags: Option<Vec<String>>,
    metadata: Option<HashMap<String, String>>
}

// Parameters for updating a node
#[derive(Deserialize)]
struct UpdateNodeParams {
    node_name: String, // Use name to identify the node
    new_name: Option<String>,
    new_description: Option<String>,
    new_content: Option<String>,
    new_tags: Option<Vec<String>>,
    new_metadata: Option<HashMap<String, String>>
}

// Parameters for deleting a node
#[derive(Deserialize)]
struct DeleteNodeParams {
    node_name: String, // Use name to identify the node
}

// Parameters for connecting two nodes
#[derive(Deserialize)]
struct ConnectNodesParams {
    from_node_name: String,
    to_node_name: String,
    relation: String,
}

#[derive(Deserialize)]
struct GetNodeParams {
    node_name: String
}

#[derive(Deserialize)]
struct GetChildrenParams {
    parent_node_name: String
}

#[derive(Deserialize)]
struct GetNodesByTagParams {
    tag: String
}

#[derive(Deserialize)]
struct SearchNodesParams {
    query: String
}

#[derive(Deserialize)]
struct GetMostConnectedParams {
    limit: Option<usize>
}

#[derive(Deserialize)]
struct GetTopTagsParams {
    limit: Option<usize>
}

// Define a struct to hold the tool information
struct GraphToolInfo {
    name: String,
    description: String,
    input_schema: Value,
}

// Create a function to build the tool information
fn graph_tool_info() -> GraphToolInfo {
    GraphToolInfo {
        name: "graph_tool".to_string(),
        description: "A tool for managing and interacting with a knowledge graph.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform.",
                    "enum": ["create_root", "create_node", "update_node", "delete_node", "connect_nodes", "get_node", "get_children", "get_nodes_by_tag", "search_nodes"]
                },
                "params": {
                    "type": "object",
                    "description": "Parameters for the action.",
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "description": {"type": "string"},
                                "content": {"type": "string"},
                                "parent_name": {"type": "string"},
                                "relation": {"type": "string"},
                                "tags": {"type": "array", "items": {"type": "string"}},
                                "metadata": {"type": "object", "additionalProperties": {"type": "string"}}
                            },
                            "required": ["name", "description", "content"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "node_name": {"type": "string"},
                                "new_name": {"type": "string"},
                                "new_description": {"type": "string"},
                                "new_content": {"type": "string"},
                                "new_tags": {"type": "array", "items": {"type": "string"}},
                                "new_metadata": {"type": "object", "additionalProperties": {"type": "string"}}
                            },
                            "required": ["node_name"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "node_name": {"type": "string"}
                            },
                            "required": ["node_name"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "from_node_name": {"type": "string"},
                                "to_node_name": {"type": "string"},
                                "relation": {"type": "string"}
                            },
                            "required": ["from_node_name", "to_node_name", "relation"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "node_name": {"type": "string"}
                            },
                            "required": ["node_name"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "parent_node_name": {"type": "string"}
                            },
                            "required": ["parent_node_name"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "tag": {"type": "string"}
                            },
                            "required": ["tag"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "query": {"type": "string"}
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

// Function to handle 'tools/call' for the graph tool
pub async fn handle_graph_tool_call(
    params: CallToolParams,
    graph_manager: &mut GraphManager,
) -> Result<JsonRpcResponse> {
    
    let tool_info = graph_tool_info(); // Get tool info here

    // Check if the tool name matches
    if params.name != tool_info.name {
        return Err(anyhow!("Tool name does not match"));
    }

    // Extract the action and its parameters
    let action = params.arguments.get("action").and_then(Value::as_str);
    let action_params = params.arguments.get("params");

    match (action, action_params) {
        (Some("create_root"), Some(params)) => {
            let create_params: CreateNodeParams = serde_json::from_value(params.clone())?;
            let node = DataNode::new(create_params.name, create_params.description, create_params.content);
            match graph_manager.create_root(node) {
                Ok(idx) => {
                    let result = json!({
                        "message": "Root node created successfully",
                        "node_index": idx.index()
                    });
                    Ok(success_response(None, json!(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: result.to_string(),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        _meta: None,
                        progress: None,
                        total: None
                    })))
                }
                Err(e) => {
                    Ok(error_response(None, INTERNAL_ERROR, &e.to_string()))
                }
            }
        }
        (Some("create_node"), Some(params)) => {
            let create_params: CreateNodeParams = serde_json::from_value(params.clone())?;
            if let Some(parent_name) = create_params.parent_name {
                if let Some((parent_idx, _)) = graph_manager.get_node_by_name(&parent_name) {
                    let mut node = DataNode::new(create_params.name, create_params.description, create_params.content);
                    
                    // Set optional fields
                    if let Some(tags) = create_params.tags {
                        node.tags = tags;
                    }
                    if let Some(metadata) = create_params.metadata {
                        node.metadata = metadata;
                    }

                    let relation = create_params.relation.ok_or_else(|| anyhow!("Missing relation for creating connected node"))?;
                    match graph_manager.create_connected_node(node, parent_idx, relation) {
                        Ok(idx) => {
                            let result = json!({
                                "message": "Node created successfully",
                                "node_index": idx.index()
                            });
                            Ok(success_response(None, json!(CallToolResult {
                                content: vec![ToolResponseContent {
                                    type_: "text".into(),
                                    text: result.to_string(),
                                    annotations: None,
                                }],
                                is_error: Some(false),
                                _meta: None,
                                progress: None,
                                total: None
                            })))
                        }
                        Err(e) => {
                            Ok(error_response(None, INTERNAL_ERROR, &e.to_string()))
                        }
                    }
                } else {
                    Ok(error_response(None, INVALID_PARAMS, "Parent node not found"))
                }
            } else {
                Ok(error_response(None, INVALID_PARAMS, "Parent name is required to create a connected node"))
            }
        }
        (Some("update_node"), Some(params)) => {
            let update_params: UpdateNodeParams = serde_json::from_value(params.clone())?;
            if let Some((idx, node)) = graph_manager.get_node_by_name(&update_params.node_name) {
                let mut updated_node = DataNode::new(
                    update_params.new_name.unwrap_or_else(|| node.name.clone()),
                    update_params.new_description.unwrap_or_else(|| node.description.clone()),
                    update_params.new_content.unwrap_or_else(|| node.content.clone()),
                );

                // Update optional fields
                if let Some(tags) = update_params.new_tags {
                    updated_node.tags = tags;
                }
                if let Some(metadata) = update_params.new_metadata {
                    updated_node.metadata = metadata;
                }

                match graph_manager.update_node(idx, updated_node) {
                    Ok(_) => {
                        let result = json!({"message": "Node updated successfully"});
                        Ok(success_response(None, json!(CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: result.to_string(),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None
                        })))
                    }
                    Err(e) => {
                        Ok(error_response(None, INTERNAL_ERROR, &e.to_string()))
                    }
                }
            } else {
                Ok(error_response(None, INVALID_PARAMS, "Node not found"))
            }
        }
        (Some("delete_node"), Some(params)) => {
            let delete_params: DeleteNodeParams = serde_json::from_value(params.clone())?;
            if let Some((idx, _)) = graph_manager.get_node_by_name(&delete_params.node_name) {
                match graph_manager.delete_node(idx) {
                    Ok(_) => {
                        let result = json!({"message": "Node deleted successfully"});
                        Ok(success_response(None, json!(CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: result.to_string(),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None
                        })))
                    }
                    Err(e) => {
                        Ok(error_response(None, INTERNAL_ERROR, &e.to_string()))
                    }
                }
            } else {
                Ok(error_response(None, INVALID_PARAMS, "Node not found"))
            }
        }
        (Some("connect_nodes"), Some(params)) => {
            let connect_params: ConnectNodesParams = serde_json::from_value(params.clone())?;
            if let (Some((from_idx, _)), Some((to_idx, _))) = (graph_manager.get_node_by_name(&connect_params.from_node_name), graph_manager.get_node_by_name(&connect_params.to_node_name)) {
                match graph_manager.connect(from_idx, to_idx, connect_params.relation) {
                    Ok(_) => {
                        let result = json!({"message": "Nodes connected successfully"});
                        Ok(success_response(None, json!(CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: result.to_string(),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None
                        })))
                    }
                    Err(e) => {
                        Ok(error_response(None, INTERNAL_ERROR, &e.to_string()))
                    }
                }
            } else {
                Ok(error_response(None, INVALID_PARAMS, "One or both nodes not found"))
            }
        }
        (Some("get_node"), Some(params)) => {
            let get_params: GetNodeParams = serde_json::from_value(params.clone())?;
            if let Some((_, node)) = graph_manager.get_node_by_name(&get_params.node_name) {
                let node_info = json!({
                    "name": node.name,
                    "description": node.description,
                    "content": node.content,
                    "tags": node.tags,
                    "metadata": node.metadata
                });
                Ok(success_response(None, json!(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: node_info.to_string(),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None
                })))
            } else {
                Ok(error_response(None, INVALID_PARAMS, "Node not found"))
            }
        }
        (Some("get_children"), Some(params)) => {
            let get_children_params: GetChildrenParams = serde_json::from_value(params.clone())?;
            if let Some((parent_idx, _)) = graph_manager.get_node_by_name(&get_children_params.parent_node_name) {
                let children = graph_manager.get_children(parent_idx);
                let children_info: Vec<_> = children.into_iter().map(|(_, child, relation)| {
                    json!({
                        "name": child.name,
                        "description": child.description,
                        "content": child.content,
                        "relation": relation,
                        "tags": child.tags,
                        "metadata": child.metadata
                    })
                }).collect();
                Ok(success_response(None, json!(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: json!(children_info).to_string(),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None
                })))
            } else {
                Ok(error_response(None, INVALID_PARAMS, "Parent node not found"))
            }
        }
        (Some("get_nodes_by_tag"), Some(params)) => {
            let get_by_tag_params: GetNodesByTagParams = serde_json::from_value(params.clone())?;
            let nodes = graph_manager.get_nodes_by_tag(&get_by_tag_params.tag);
            let nodes_info: Vec<_> = nodes.into_iter().map(|(_, node)| {
                json!({
                    "name": node.name,
                    "description": node.description,
                    "content": node.content,
                    "tags": node.tags,
                    "metadata": node.metadata
                })
            }).collect();
            Ok(success_response(None, json!(CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: json!(nodes_info).to_string(),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None
            })))
        }
        (Some("search_nodes"), Some(params)) => {
            let search_params: SearchNodesParams = serde_json::from_value(params.clone())?;
            let nodes = graph_manager.search_nodes(&search_params.query);
            let nodes_info: Vec<_> = nodes.into_iter().map(|(_, node)| {
                json!({
                    "name": node.name,
                    "description": node.description,
                    "content": node.content,
                    "tags": node.tags,
                    "metadata": node.metadata
                })
            }).collect();
            Ok(success_response(None, json!(CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: json!(nodes_info).to_string(),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None
            })))
        }
        (Some("get_most_connected"), Some(params)) => {
            let most_connected_params: GetMostConnectedParams = serde_json::from_value(params.clone())?;
            let limit = most_connected_params.limit.unwrap_or(10);
            let nodes = graph_manager.get_most_connected_nodes(limit);
            let nodes_info: Vec<_> = nodes.into_iter().map(|(_, node, edge_count)| {
                json!({
                    "name": node.name,
                    "description": node.description,
                    "content": node.content,
                    "tags": node.tags,
                    "metadata": node.metadata,
                    "connection_count": edge_count
                })
            }).collect();
            Ok(success_response(None, json!(CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: json!(nodes_info).to_string(),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None
            })))
        }
        (Some("get_top_tags"), Some(params)) => {
            let top_tags_params: GetTopTagsParams = serde_json::from_value(params.clone())?;
            let limit = top_tags_params.limit.unwrap_or(10);
            let tags = graph_manager.get_top_tags(limit);
            let tags_info: Vec<_> = tags.into_iter().map(|(tag, count)| {
                json!({
                    "tag": tag,
                    "count": count
                })
            }).collect();
            Ok(success_response(None, json!(CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: json!(tags_info).to_string(),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None
            })))
        }
        _ => Ok(error_response(None, INVALID_PARAMS, "Invalid action or parameters")),
    }
}
