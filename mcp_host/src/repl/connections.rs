use anyhow::{anyhow, Result};
use shared_protocol_objects::client::ReplClient;  // Updated import
use std::collections::HashMap;
use std::path::PathBuf;

/// Manages the connections to MCP servers
pub struct ServerConnections {
    servers: HashMap<String, Box<dyn ReplClient>>,
    current: Option<String>,
    config_path: Option<PathBuf>,
}

impl ServerConnections {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            current: None,
            config_path: None,
        }
    }
    
    pub fn add_server(&mut self, client: Box<dyn ReplClient>) -> Result<()> {
        let name = client.name().to_string();
        
        if self.servers.contains_key(&name) {
            return Err(anyhow!("Server '{}' already exists", name));
        }
        
        self.servers.insert(name.clone(), client);
        
        // If this is our first server, make it the current one
        if self.current.is_none() {
            self.current = Some(name);
        }
        
        Ok(())
    }
    
    pub fn remove_server(&mut self, name: &str) -> Result<Box<dyn ReplClient>> {
        let client = self.servers.remove(name)
            .ok_or_else(|| anyhow!("Server '{}' not found", name))?;
            
        // If this was the current server, clear that selection
        if self.current.as_ref().map_or(false, |s| s == name) {
            self.current = None;
        }
        
        Ok(client)
    }
    
    pub fn get_server(&self, name: &str) -> Option<&dyn ReplClient> {
        self.servers.get(name).map(|s| s.as_ref())
    }
    
    pub fn get_current_server(&self) -> Option<&dyn ReplClient> {
        self.current.as_ref().and_then(|name| self.get_server(name))
    }
    
    pub fn set_current_server(&mut self, name: Option<String>) -> Result<()> {
        if let Some(ref name) = name {
            if !self.servers.contains_key(name) {
                return Err(anyhow!("Server '{}' not found", name));
            }
        }
        
        self.current = name;
        Ok(())
    }
    
    pub fn current_server_name(&self) -> Option<&str> {
        self.current.as_deref()
    }
    
    pub fn server_names(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }
    
    pub fn set_config_path(&mut self, path: PathBuf) {
        self.config_path = Some(path);
    }
    
    pub fn config_path(&self) -> Option<&PathBuf> {
        self.config_path.as_ref()
    }
    
    pub async fn close_all(mut self) -> Result<()> {
        let mut errors = Vec::new();
        
        for (name, client) in self.servers.drain() {
            if let Err(e) = client.close().await {
                errors.push(format!("Error closing '{}': {}", name, e));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Errors while closing connections: {}", errors.join(", ")))
        }
    }
}

impl Default for ServerConnections {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_protocol_objects::client::MockReplClient;
    
    #[tokio::test]
    async fn test_server_connections() {
        let mut connections = ServerConnections::new();
        
        // Add a server
        let client1 = Box::new(MockReplClient::new("server1"));
        connections.add_server(client1).unwrap();
        
        // Check it was added and made current
        assert_eq!(connections.server_names(), vec!["server1"]);
        assert_eq!(connections.current_server_name(), Some("server1"));
        
        // Add another server
        let client2 = Box::new(MockReplClient::new("server2"));
        connections.add_server(client2).unwrap();
        
        // Check it was added but didn't change current
        assert_eq!(connections.server_names().len(), 2);
        assert!(connections.server_names().contains(&"server2".to_string()));
        assert_eq!(connections.current_server_name(), Some("server1"));
        
        // Set server2 as current
        connections.set_current_server(Some("server2".to_string())).unwrap();
        assert_eq!(connections.current_server_name(), Some("server2"));
        
        // Try to set a non-existent server
        assert!(connections.set_current_server(Some("server3".to_string())).is_err());
        
        // Remove server2
        let client = connections.remove_server("server2").unwrap();
        assert_eq!(client.name(), "server2");
        
        // Check current was cleared
        assert_eq!(connections.current_server_name(), None);
        
        // Set config path
        let path = PathBuf::from("/test/config.json");
        connections.set_config_path(path.clone());
        assert_eq!(connections.config_path(), Some(&path));
    }
}