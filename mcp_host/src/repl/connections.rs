use anyhow::{anyhow, Result};
// use shared_protocol_objects::client::ReplClient;  // Updated import
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
