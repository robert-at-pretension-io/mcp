use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

use anyhow::{anyhow, Result};
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{adj::IndexType, Graph};
use serde_json::{json, Value};
use tracing::debug;
use shared_protocol_objects::{
    success_response, CallToolParams,
    CallToolResult, JsonRpcResponse,
    ToolInfo,
    ToolResponseContent
};

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct DataNode {
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) metadata: HashMap<String, String>,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    quotes: Vec<String>,
    #[serde(default)]
    child_nodes: Vec<String>,
    #[serde(default = "chrono::Utc::now")]
    date_created: chrono::DateTime<chrono::Utc>,
    #[serde(default = "chrono::Utc::now")]
    date_modified: chrono::DateTime<chrono::Utc>,
}

impl DataNode {
    pub(crate) fn new(name: String, description: String, content: String) -> Self {
        let now = chrono::Utc::now();
        DataNode {
            name,
            description,
            content,
            metadata: HashMap::new(),
            tags: Vec::new(),
            quotes: Vec::new(),
            child_nodes: Vec::new(),
            date_created: now,
            date_modified: now,
        }
    }
}

// Custom serialization for Graph
// We'll use petgraph's built-in serde support instead of custom impls

#[derive(Serialize, Deserialize)]
struct SerializableGraph {
    nodes: Vec<(usize, DataNode)>,
    edges: Vec<(usize, usize, String)>
}

#[derive(Clone)]
pub struct GraphManager {
    pub(crate) graph: Graph<DataNode, String>,
    pub(crate) root: Option<NodeIndex>,
    path: std::path::PathBuf,
}

impl GraphManager {
    fn get_graph_metadata(&self) -> String {
        let total_nodes = self.graph.node_count();
        let total_edges = self.graph.edge_count();
        let recent_nodes = self.get_recent_nodes(5)
            .iter()
            .map(|(_, node)| node.name.clone())
            .collect::<Vec<_>>();
        let top_tags = self.get_top_tags(5)
            .iter()
            .map(|(tag, count)| format!("{} ({})", tag, count))
            .collect::<Vec<_>>();

        format!(
            "\nGraph Status:\n\
             - Total nodes: {}\n\
             - Total connections: {}\n\
             - Recent nodes: {}\n\
             - Top tags: {}\n",
            total_nodes,
            total_edges,
            recent_nodes.join(", "),
            top_tags.join(", ")
        )
    }


    fn node_name_exists(&self, name: &str) -> bool {
        self.graph.node_indices().any(|idx| {
            self.graph.node_weight(idx)
                .map(|node| node.name == name)
                .unwrap_or(false)
        })
    }

    pub fn new(filename: String) -> Self {
        debug!("Creating new GraphManager with filename: {}", filename);
        
        // Always use home directory
        let path = dirs::home_dir()
            .expect("Could not find home directory")
            .join(&filename);
        
        debug!("Using graph path: {}", path.display());

        // Try loading existing graph first
        let graph = if path.exists() {
            debug!("Found existing graph file at {}", path.display());
            debug!("Found existing graph file at {}", path.display());
            let serializable = match fs::read_to_string(&path) {
                Ok(data) => {
                    debug!("Successfully loaded graph data");
                    match serde_json::from_str(&data) {
                        Ok(s) => {
                            debug!("Successfully parsed graph data");
                            s
                        }
                        Err(e) => {
                            debug!("Failed to parse graph data: {}", e);
                            SerializableGraph {
                                nodes: vec![],
                                edges: vec![]
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed to read graph file: {}", e);
                    SerializableGraph {
                        nodes: vec![],
                        edges: vec![]
                    }
                }
            };

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
                graph[NodeIndex::new(idx)] = node;
            }
            // Restore edges
            for (from, to, weight) in serializable.edges {
                graph.add_edge(NodeIndex::new(from), NodeIndex::new(to), weight);
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


    async fn save(&self) -> Result<()> {
        debug!("Starting save operation to {}", self.path.display());
        // Convert to serializable format with better error handling
        let serializable = SerializableGraph {
            nodes: self.graph.node_indices()
                .map(|idx| (idx.index(), self.graph[idx].clone()))
                .collect(),
            edges: self.graph.edge_indices()
                .map(|idx| {
                    let (a, b) = self.graph.edge_endpoints(idx)
                        .ok_or_else(|| anyhow!("Invalid edge index {}", idx.index()))?;
                    Ok((a.index(), b.index(), self.graph[idx].clone()))
                })
                .collect::<Result<Vec<_>>>()?,
        };
        let json = serde_json::to_string(&serializable)
            .map_err(|e| anyhow!("Failed to serialize graph: {}", e))?;
        debug!("Writing graph data...");
        tokio::fs::write(&self.path, json).await
            .map_err(|e| anyhow!("Failed to write graph file {}: {}", self.path.display(), e))?;
        debug!("Graph save completed successfully");
        Ok(())
    }

    pub(crate) async fn create_root(&mut self, node: DataNode) -> Result<NodeIndex> {
        if self.root.is_some() {
            let connected_nodes = self.get_most_connected_nodes(10)
                .iter()
                .map(|(_, node, count)| format!("- {} ({} connections)", node.name, count))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("Root already exists.\n\nMost connected nodes for reference:\n{}", connected_nodes));
        }
        if self.node_name_exists(&node.name) {
            let connected_nodes = self.get_most_connected_nodes(10)
                .iter()
                .map(|(_, node, count)| format!("- {} ({} connections)", node.name, count))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("A node with this name already exists.\n\nMost connected nodes for reference:\n{}", connected_nodes));
        }
        let idx = self.graph.add_node(node);
        self.root = Some(idx);
        self.save().await?;
        Ok(idx)
    }

    pub(crate) async fn create_connected_node(&mut self, node: DataNode, parent: NodeIndex, rel: String) -> Result<NodeIndex> {
        if !self.graph.node_weight(parent).is_some() {
            return Err(anyhow!("Parent node index {} not found in graph", parent.index()));
        }
        if self.node_name_exists(&node.name) {
            let connected_nodes = self.get_most_connected_nodes(10)
                .iter()
                .map(|(_, node, count)| format!("- {} ({} connections)", node.name, count))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(anyhow!("Node with name '{}' already exists.\n\nMost connected nodes for reference:\n{}", node.name, connected_nodes));
        }

        // Add node to graph
        let idx = self.graph.add_node(node.clone());
        self.graph.add_edge(parent, idx, rel.clone());

        // Update parent's child list
        self.update_parent_child_list(parent);

        self.save().await.map_err(|e| anyhow!(
            "Failed to save graph after adding node '{}' with relation '{}' to parent {}: {}",
            node.name, rel, parent.index(), e
        ))?;
        Ok(idx)
    }

    pub(crate) async fn update_node(&mut self, idx: NodeIndex, node: DataNode) -> Result<()> {
        // Only check for name uniqueness if the name is actually changing
        if let Some(current) = self.graph.node_weight(idx) {
            if current.name != node.name && self.node_name_exists(&node.name) {
                return Err(anyhow!("A node with this name already exists"));
            }
        }
        if let Some(n) = self.graph.node_weight_mut(idx) {
            let date_created = n.date_created; // Preserve original creation date
            *n = node;
            n.date_created = date_created; // Restore creation date
            n.date_modified = chrono::Utc::now();
            self.save().await?;
        }
        Ok(())
    }


    async fn delete_node(&mut self, idx: NodeIndex) -> Result<()> {
        if Some(idx) == self.root {
            return Err(anyhow!("Cannot delete root node"));
        }

        let node_name = self.graph.node_weight(idx)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("index {}", idx.index()));

        let neighbors: Vec<_> = self.graph.neighbors(idx).collect();
        let incoming: Vec<_> = self.graph.neighbors_directed(idx, petgraph::Direction::Incoming).collect();

        if neighbors.len() > 1 || incoming.len() > 1 {
            return Err(anyhow!(
                "Cannot delete node '{}' with multiple connections (has {} outgoing and {} incoming)",
                node_name, neighbors.len(), incoming.len()
            ));
        }

        // Update parent's child_nodes list
        if let Some(parent_idx) = incoming.first() {
            // First remove the node
            self.graph.remove_node(idx)
                .ok_or_else(|| anyhow!("Failed to remove node '{}' from graph", node_name))?;

            // Then update parent's child list
            self.update_parent_child_list(*parent_idx);
        } else {
            self.graph.remove_node(idx)
                .ok_or_else(|| anyhow!("Failed to remove node '{}' from graph", node_name))?;
        }

        self.graph.remove_node(idx)
            .ok_or_else(|| anyhow!("Failed to remove node '{}' from graph", node_name))?;

        self.save().await.map_err(|e| anyhow!(
            "Failed to save graph after deleting node '{}': {}",
            node_name, e
        ))?;
        Ok(())
    }


    pub(crate) fn get_node(&self, idx: NodeIndex) -> Option<&DataNode> {
        self.graph.node_weight(idx)
    }

    pub(crate) async fn connect(&mut self, from: NodeIndex, to: NodeIndex, rel: String) -> Result<()> {
        self.graph.add_edge(from, to, rel);
        Ok(self.save().await?)
    }

    // Method to get a node by its name
    pub(crate) fn get_node_by_name(&self, name: &str) -> Option<(NodeIndex, &DataNode)> {
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
        // Get actual children from graph edges
        for edge in self.graph.edges(parent) {
            let child_idx = edge.target();
            if let Some(child_node) = self.graph.node_weight(child_idx) {
                children.push((child_idx, child_node, edge.weight().clone()));
            }
        }
        children
    }

    fn update_parent_child_list(&mut self, parent: NodeIndex) {
        let children = self.get_children(parent);
        let actual_child_names: Vec<String> = children.iter()
            .map(|(_, node, _)| node.name.clone())
            .collect();

        if let Some(parent_node) = self.graph.node_weight_mut(parent) {
            if parent_node.child_nodes != actual_child_names {
                parent_node.child_nodes = actual_child_names;
                parent_node.date_modified = chrono::Utc::now();
                // Schedule save for next event loop iteration
                let graph_manager = (*self).clone();
                tokio::spawn(async move {
                    if let Err(e) = graph_manager.save().await {
                        eprintln!("Failed to save graph after updating child_nodes: {}", e);
                    }
                });
            }
        }
    }

    // Method to get all nodes matching a tag

    // Method to get all nodes with names or descriptions matching a query string
    pub(crate) fn search_nodes(&self, query: &str) -> Vec<(NodeIndex, &DataNode)> {
        let results = self.graph.node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).and_then(|node| {
                    if node.name.contains(query) || node.description.contains(query) {
                        Some((idx, node))
                    } else {
                        None
                    }
                })
            })
            .collect::<Vec<_>>();

        if results.is_empty() {
            debug!("No results found for query: {}", query);
            debug!("Current graph state: {}", self.get_graph_metadata());
        }

        results
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

    fn get_recent_nodes(&self, limit: usize) -> Vec<(NodeIndex, &DataNode)> {
        let mut nodes: Vec<_> = self.graph.node_indices()
            .filter_map(|idx| {
                self.graph.node_weight(idx).map(|node| (idx, node))
            })
            .collect();

        // Sort by modified date in descending order
        nodes.sort_by(|a, b| b.1.date_modified.cmp(&a.1.date_modified));
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


    // Method to get all node indices in the graph
    pub fn node_indices(&self) -> impl Iterator<Item = NodeIndex> + '_ {
        self.graph.node_indices()
    }
}

// Parameters for creating a new node
#[derive(Deserialize)]
struct CreateNodeParams {
    name: String,
    description: String,
    content: String,
    quotes: Option<Vec<String>>,
    is_root: Option<bool>,
    parent_name: Option<String>,
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
    new_metadata: Option<HashMap<String, String>>,
    new_quotes: Option<Vec<String>>
}

// Parameters for deleting a node
#[derive(Deserialize)]
struct DeleteNodeParams {
    node_name: String, // Use name to identify the node
}

#[derive(Deserialize)]
struct MoveNodeParams {
    node_name: String,
    new_parent_name: String,
    new_relation: String,
}

// Parameters for connecting two nodes
#[derive(Deserialize)]
struct ConnectNodesParams {
    from_node_name: String,
    to_node_name: String,
    relation: String,
}

#[derive(Deserialize)]
struct GetRecentNodesParams {
    limit: Option<usize>
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



pub fn graph_tool_info() -> ToolInfo {
    ToolInfo {
        name: "graph_tool".to_string(),
        description: Some(
            r#"Simplified Knowledge Graph Tool for managing interconnected information.

Core Commands:
- create_root(name, description, content, tags) - Create root node
- create_child(name, description, content, parent, relation, tags) - Create child node
- update_node(name, new_name, new_description, new_content, new_tags) - Update node
- delete_node(name) - Delete node
- connect_nodes(from, to, relation) - Connect nodes
- search(query) - Search nodes
- get_stats() - Get graph statistics

Example Usage:
1. Create root node:
{
  "command": "create_root",
  "name": "Project Requirements",
  "description": "Core project requirements",
  "content": "Main requirements...",
  "tags": ["requirements"]
}

2. Add child node:
{
  "command": "create_child", 
  "name": "Authentication",
  "description": "Auth requirements",
  "content": "Auth specs...",
  "parent": "Project Requirements",
  "relation": "requires",
  "tags": ["auth"]
}

3. Search:
{
  "command": "search",
  "query": "auth"
}"#.into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "enum": [
                        "create_root",
                        "create_child", 
                        "update_node",
                        "delete_node",
                        "connect_nodes",
                        "search",
                        "get_stats"
                    ]
                },
                "name": {"type": "string"},
                "description": {"type": "string"},
                "content": {"type": "string"},
                "parent": {"type": "string"},
                "relation": {"type": "string"},
                "tags": {"type": "array", "items": {"type": "string"}},
                "query": {"type": "string"}
            },
            "required": ["command"]
        }),
    }
}

pub async fn handle_graph_tool_call(
    params: CallToolParams,
    graph_manager: &mut GraphManager,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let command = params.arguments.get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Missing 'command' field"))?;

    macro_rules! get_field {
        ($name:expr) => {
            params.arguments.get($name)
                .and_then(Value::as_str)
                .map(|s| s.to_string())
        };
        ($name:expr, $default:expr) => {
            params.arguments.get($name)
                .and_then(Value::as_str)
                .map(|s| s.to_string())
                .unwrap_or($default)
        };
    }

    macro_rules! get_tags {
        () => {
            params.arguments.get("tags")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_else(Vec::new)
        };
    }

    macro_rules! return_error {
        ($msg:expr) => {{
            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: $msg.to_string(),
                    annotations: None,
                }],
                is_error: Some(true),
                _meta: None,
                progress: None,
                total: None,
            };
            return Ok(success_response(id.clone(), serde_json::to_value(tool_res)?));
        }};
    }

    match command {
        "create_root" => {
            let name = get_field!("name").ok_or_else(|| anyhow!("Missing name"))?;
            let description = get_field!("description").ok_or_else(|| anyhow!("Missing description"))?;
            let content = get_field!("content").ok_or_else(|| anyhow!("Missing content"))?;
            let tags = get_tags!();

            let mut node = DataNode::new(name, description, content);
            node.tags = tags;

            match graph_manager.create_root(node).await {
                Ok(idx) => {
                    let tool_res = CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: format!("Root node created with index {}", idx.index()),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        _meta: None,
                        progress: None,
                        total: None,
                    };
                    Ok(success_response(id, serde_json::to_value(tool_res)?))
                }
                Err(e) => return_error!(format!("Failed to create root: {}", e))
            }
        }

        "create_child" => {
            let name = get_field!("name").ok_or_else(|| anyhow!("Missing name"))?;
            let description = get_field!("description").ok_or_else(|| anyhow!("Missing description"))?;
            let content = get_field!("content").ok_or_else(|| anyhow!("Missing content"))?;
            let parent = get_field!("parent").ok_or_else(|| anyhow!("Missing parent"))?;
            let relation = get_field!("relation").ok_or_else(|| anyhow!("Missing relation"))?;
            let tags = get_tags!();

            let mut node = DataNode::new(name, description, content);
            node.tags = tags;

            if let Some((parent_idx, _)) = graph_manager.get_node_by_name(&parent) {
                match graph_manager.create_connected_node(node, parent_idx, relation).await {
                    Ok(idx) => {
                        let tool_res = CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: format!("Child node created with index {}", idx.index()),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None,
                        };
                        Ok(success_response(id, serde_json::to_value(tool_res)?))
                    }
                    Err(e) => return_error!(format!("Failed to create child: {}", e))
                }
            } else {
                return_error!(format!("Parent node '{}' not found", parent))
            }
        }
        "update_node" => {
            let name = get_field!("name").ok_or_else(|| anyhow!("Missing name"))?;
            let new_name = get_field!("new_name");
            let new_description = get_field!("new_description");
            let new_content = get_field!("new_content");
            let new_tags = get_tags!();

            if let Some((idx, node)) = graph_manager.get_node_by_name(&name) {
                let mut updated_node = node.clone();
                
                if let Some(name) = new_name {
                    updated_node.name = name;
                }
                if let Some(desc) = new_description {
                    updated_node.description = desc;
                }
                if let Some(content) = new_content {
                    updated_node.content = content;
                }
                if !new_tags.is_empty() {
                    updated_node.tags = new_tags;
                }

                match graph_manager.update_node(idx, updated_node).await {
                    Ok(_) => {
                        let tool_res = CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: format!("Node '{}' updated successfully", name),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None,
                        };
                        Ok(success_response(id, serde_json::to_value(tool_res)?))
                    }
                    Err(e) => return_error!(format!("Failed to update node '{}': {}", name, e))
                }
            } else {
                return_error!(format!("Node '{}' not found", name))
            }
        }
        "delete_node" => {
            let name = get_field!("name").ok_or_else(|| anyhow!("Missing name"))?;
            
            if let Some((idx, _)) = graph_manager.get_node_by_name(&name) {
                match graph_manager.delete_node(idx).await {
                    Ok(_) => {
                        let tool_res = CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: format!("Node '{}' deleted successfully", name),
                                annotations: None,
                            }],
                            is_error: Some(false),
                            _meta: None,
                            progress: None,
                            total: None,
                        };
                        Ok(success_response(id, serde_json::to_value(tool_res)?))
                    }
                    Err(e) => return_error!(format!("Failed to delete node '{}': {}", name, e))
                }
            } else {
                return_error!(format!("Node '{}' not found", name))
            }
        }
        "connect_nodes" => {
            let from = get_field!("from").ok_or_else(|| anyhow!("Missing 'from' node name"))?;
            let to = get_field!("to").ok_or_else(|| anyhow!("Missing 'to' node name"))?;
            let relation = get_field!("relation").ok_or_else(|| anyhow!("Missing relation"))?;

            let from_node = graph_manager.get_node_by_name(&from);
            let to_node = graph_manager.get_node_by_name(&to);

            if from_node.is_none() {
                return_error!(format!("Source node '{}' not found", from));
            }
            if to_node.is_none() {
                return_error!(format!("Target node '{}' not found", to));
            }

            let (from_idx, _) = from_node.unwrap();
            let (to_idx, _) = to_node.unwrap();

            match graph_manager.connect(from_idx, to_idx, relation.clone()).await {
                Ok(_) => {
                    let tool_res = CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: format!("Connected '{}' to '{}' with relation '{}'", from, to, relation),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        _meta: None,
                        progress: None,
                        total: None,
                    };
                    Ok(success_response(id, serde_json::to_value(tool_res)?))
                }
                Err(e) => return_error!(format!(
                    "Failed to connect '{}' to '{}' with relation '{}': {}",
                    from, to, relation, e
                ))
            }
        }
        "search" => {
            let query = get_field!("query").ok_or_else(|| anyhow!("Missing query"))?;
            let nodes = graph_manager.search_nodes(&query);
            
            if nodes.is_empty() {
                return_error!("No nodes found matching query")
            } else {
                let results = nodes.into_iter().map(|(_, node)| {
                    json!({
                        "name": node.name,
                        "description": node.description,
                        "content": node.content,
                        "tags": node.tags
                    })
                }).collect::<Vec<_>>();
                
                Ok(success_response(id, json!(results)))
            }
        }
        "get_stats" => {
            let limit = 10; // Default limit for stats
            let most_connected = graph_manager.get_most_connected_nodes(limit);
            let recent = graph_manager.get_recent_nodes(limit);
            let top_tags = graph_manager.get_top_tags(limit);

            let stats = json!({
                "most_connected": most_connected.into_iter().map(|(_, node, count)| {
                    json!({
                        "name": node.name,
                        "connection_count": count
                    })
                }).collect::<Vec<_>>(),
                "recent_nodes": recent.into_iter().map(|(_, node)| {
                    json!({
                        "name": node.name,
                        "modified": node.date_modified
                    })
                }).collect::<Vec<_>>(),
                "top_tags": top_tags.into_iter().map(|(tag, count)| {
                    json!({
                        "tag": tag,
                        "count": count
                    })
                }).collect::<Vec<_>>()
            });

            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string()),
                    annotations: None,
                }],
                is_error: Some(false),
                _meta: None,
                progress: None,
                total: None
            };
            Ok(success_response(id, serde_json::to_value(tool_res)?))
        }
        _ => return_error!(format!("Invalid action '{}'. Supported actions: create_root, create_node, update_node, delete_node, connect_nodes, get_node, get_children, get_nodes_by_tag, search_nodes, get_most_connected, get_top_tags, get_recent_nodes, get_tags_by_date", command))
    }
}
