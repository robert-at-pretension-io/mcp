pub mod server_manager;
pub mod config;
pub mod protocol;
pub mod error;

use std::sync::Arc;
use std::time::Duration;
use anyhow::Result;
use tokio::sync::Mutex;
use std::collections::HashMap;

use crate::ai_client::AIClient;
use shared_protocol_objects::Implementation;
use server_manager::ManagedServer;

pub struct MCPHost {
    pub servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    pub client_info: Implementation,
    pub request_timeout: Duration,
    ai_client: Option<Box<dyn AIClient>>,
}

impl Clone for MCPHost {
    fn clone(&self) -> Self {
        Self {
            servers: Arc::clone(&self.servers),
            client_info: self.client_info.clone(),
            request_timeout: self.request_timeout,
            ai_client: None, // AI client isn't cloneable, but that's ok for our purposes
        }
    }
}

impl MCPHost {
    /// Create a new builder
    pub fn builder() -> MCPHostBuilder {
        MCPHostBuilder::new()
    }

    /// Create a new instance with default values
    pub async fn new() -> Result<Self> {
        MCPHostBuilder::new().build().await
    }

    /// Load configuration from a file
    pub async fn load_config(&self, config_path: &str) -> Result<()> {
        self.server_manager().load_config(config_path).await
    }

    /// Configure the host with a loaded configuration
    pub async fn configure(&self, config: config::Config) -> Result<()> {
        self.server_manager().configure(config).await
    }

    /// Run the REPL interface
    pub async fn run_repl(&self) -> Result<()> {
        let mut repl = crate::repl::Repl::new(Arc::clone(&self.servers))?.with_host(self.clone());
        repl.run().await
    }

    /// Get a reference to the server manager
    fn server_manager(&self) -> server_manager::ServerManager {
        server_manager::ServerManager::new(
            Arc::clone(&self.servers),
            self.client_info.clone(),
            self.request_timeout,
        )
    }

    /// List the tools available on a server
    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<shared_protocol_objects::ToolInfo>> {
        self.server_manager().list_server_tools(server_name).await
    }

    /// Call a tool on a server
    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: serde_json::Value) -> Result<String> {
        self.server_manager().call_tool(server_name, tool_name, args).await
    }

    /// Start a server with the given command
    pub async fn start_server(&self, name: &str, command: &str, args: &[String]) -> Result<()> {
        self.server_manager().start_server(name, command, args).await
    }

    /// Stop a server
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        self.server_manager().stop_server(name).await
    }

    /// Enter chat mode with a server
    pub async fn enter_chat_mode(&self, server_name: &str) -> Result<crate::conversation_state::ConversationState> {
        // This implementation remains largely the same as in host.rs
        // Fetch tools from the server
        let tool_info_list = self.list_server_tools(server_name).await?;

        // Convert our tool list to a JSON structure - we'll use this for debugging
        let _tools_json: Vec<serde_json::Value> = tool_info_list.iter().map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description.as_ref().unwrap_or(&"".to_string()),
                "inputSchema": t.input_schema
            })
        }).collect();

        // Create the tools string first
        let tools_str = tool_info_list.iter().map(|tool| {
            format!(
                "- {}: {}\ninput schema: {:?}",
                tool.name,
                tool.description.as_ref().unwrap_or(&"".to_string()),
                tool.input_schema
            )
        }).collect::<Vec<_>>().join("");

        log::debug!("tool_str is {:?}", &tools_str);

        // Generate simplified system prompt
        let system_prompt = format!(
            "You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions.\n\
            TOOLS:\n{}",
            tools_str
        );

        // Create the conversation state
        let mut state = crate::conversation_state::ConversationState::new(system_prompt, tool_info_list.clone());
        
        // Use the new smiley-delimited JSON format for tool calling
        let smiley_instruction = crate::conversation_service::generate_smiley_tool_system_prompt(&tool_info_list);

        log::debug!("smiley_instruction is {:?}", &smiley_instruction);

        // Add the smiley instruction as a system message
        state.add_system_message(&smiley_instruction);

        Ok(state)
    }

    /// Generate a system prompt with tool information
    pub fn generate_system_prompt(&self, tools: &[serde_json::Value]) -> String {
        let tools_section = serde_json::to_string_pretty(&serde_json::json!({ "tools": tools })).unwrap_or_else(|_| "".to_string());

        format!(
            "You are a helpful assistant with access to tools. Use tools only when necessary.\n\n\
            CORE RESPONSIBILITIES:\n\
            1. Create knowledge graph nodes when important new information is shared\n\
            2. Use tools to gather additional context when needed\n\
            3. Maintain natural conversation flow\n\n\
            TOOL USAGE GUIDELINES:\n\
            - Use tools only when they would provide valuable information\n\
            - Create nodes for significant new information\n\
            - Connect information when it helps the conversation\n\
            - Suggest tool usage only when it would be genuinely helpful\n\n\
            CONVERSATION STYLE:\n\
            - Focus on natural conversation\n\
            - Use tools subtly when needed\n\
            - Avoid excessive tool usage\n\
            - Only reference tool outputs when relevant\n\n\
            {}\n\n\
            TOOL CALLING FORMAT:\n\
            When calling a tool, your ENTIRE response must be a JSON object with this format:\n\
            {{\n\
                \"tool\": \"tool_name\",\n\
                \"arguments\": {{\n\
                    ... tool parameters ...\n\
                }}\n\
            }}\n\n\
            IMPORTANT: When calling a tool, your response must be ONLY valid JSON and nothing else.",
            tools_section
        )
    }

    /// Get the AI client
    pub fn ai_client(&self) -> Option<&Box<dyn AIClient>> {
        self.ai_client.as_ref()
    }
}

/// Builder for MCPHost configuration
pub struct MCPHostBuilder {
    ai_client: Option<Box<dyn AIClient>>,
    request_timeout: Option<Duration>,
    client_info: Option<Implementation>,
}

impl MCPHostBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            ai_client: None,
            request_timeout: None,
            client_info: None,
        }
    }
    
    /// Set the AI client
    pub fn ai_client(mut self, client: Box<dyn AIClient>) -> Self {
        self.ai_client = Some(client);
        self
    }
    
    /// Set the request timeout
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }
    
    /// Set the client info
    pub fn client_info(mut self, name: &str, version: &str) -> Self {
        self.client_info = Some(Implementation {
            name: name.to_string(),
            version: version.to_string(),
        });
        self
    }
    
    /// Build the MCPHost
    pub async fn build(self) -> Result<MCPHost> {
        // Create AI client if not provided
        let ai_client = match self.ai_client {
            Some(client) => Some(client),
            None => Self::create_default_ai_client().await?,
        };
        
        let request_timeout = self.request_timeout.unwrap_or(Duration::from_secs(120));
        
        let client_info = self.client_info.unwrap_or_else(|| Implementation {
            name: "mcp-host".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        });
        
        Ok(MCPHost {
            servers: Arc::new(Mutex::new(HashMap::new())),
            client_info,
            request_timeout,
            ai_client,
        })
    }

    /// Create the default AI client based on environment variables
    async fn create_default_ai_client() -> Result<Option<Box<dyn AIClient>>> {
        // Try to get the AI provider from environment
        let model_name = "deepseek-chat".to_string();
        log::info!("Initializing DeepSeek client with model: {}", model_name);

        // Retrieve the DeepSeek API key from an environment variable
        let deepseek_key_result = std::env::var("DEEPSEEK_API_KEY");
        log::debug!("Result of reading DEEPSEEK_API_KEY: {:?}", deepseek_key_result);

        match deepseek_key_result {
            Ok(api_key) => {
                log::info!("Got DeepSeek API key (length: {})", api_key.len());
                let client = crate::deepseek::DeepSeekClient::new(api_key, model_name);
                Ok(Some(Box::new(client) as Box<dyn AIClient>))
            },
            Err(e) => {
                log::warn!("Failed to get DEEPSEEK_API_KEY: {}", e);
                // This log message is slightly misleading as we only checked DeepSeek here
                log::info!("No AI client configured. Set MCP_AI_PROVIDER and corresponding API key (OPENAI_API_KEY or GEMINI_API_KEY or ANTHROPIC_API_KEY or DEEPSEEK_API_KEY)");
                log::info!("Returning Ok(None) for default AI client creation.");
                Ok(None)
            }
        }
    }
}
