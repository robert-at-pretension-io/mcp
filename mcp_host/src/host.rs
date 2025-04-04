use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde_json::{json, Value};
use shared_protocol_objects::{
    Implementation, JsonRpcRequest, JsonRpcResponse, RequestId, ServerCapabilities,
    CallToolResult, ListToolsResult, ToolInfo, Role
};
use std::collections::HashMap;
use std::io;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;

use crate::ai_client::AIClient;
use crate::conversation_state::ConversationState;
use crate::my_regex::build_tool_call_regex;
use crate::deepseek::DeepSeekClient;

// Server Management Types
#[derive(Debug)]
#[allow(dead_code)]
struct ManagedServer {
    name: String, 
    process: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<ChildStdout>>,
    capabilities: Option<ServerCapabilities>,
    initialized: bool,
}

pub struct MCPHost {
    pub servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    pub client_info: Implementation,
    pub request_timeout: std::time::Duration, 
    pub ai_client: Option<Box<dyn AIClient>>,
}

impl MCPHost {
    pub async fn enter_chat_mode(&self, server_name: &str) -> Result<ConversationState> {
        // Fetch tools from the server
        let tool_info_list = self.list_server_tools(server_name).await?;

        // Convert our tool list to a JSON structure
        let tools_json: Vec<serde_json::Value> = tool_info_list.iter().map(|t| {
            json!({
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

        debug!("{}",format!("tool_str is {:?}", &tools_str));

        // Generate simplified system prompt
        let system_prompt = format!(
            "You are a helpful assistant with access to tools. Use tools EXACTLY according to their descriptions.\n\
            TOOLS:\n{}",
            tools_str
        );

        // Create the conversation state
        let mut state = ConversationState::new(system_prompt, tool_info_list.clone());
        
        // Create a simplified hidden instruction with regex pattern
        let hidden_instruction = format!(
            "[GUIDANCE]\n\
            You are a helpful assistant who can call tools when useful. Follow these guidelines:\n\
            - Use tools only when additional context or information is needed\n\
            - Consider running tools if the user's request requires it\n\n\
            TOOL CALLING FORMAT:\n\
            The pattern must match this regular expression:\n\
            {}\n\n\
            TOOLS:\n{}\n
            For example:
            I'll use [tool_name] to do...
            ```json
            {{parameters go here}}
            ```

            Make sure to include that format for EVERY tool call.
            ",
            build_tool_call_regex(&tool_info_list.iter().map(|t| t.name.clone()).collect::<Vec<_>>()).as_str(),
            tools_str
        );

        debug!("{}",format!("hidden_instruction is {:?}", &hidden_instruction));


        // Add the hidden instruction as a user message
        state.add_user_message(&hidden_instruction);

        Ok(state)
    }

    fn generate_system_prompt(&self, tools: &[serde_json::Value]) -> String {
        let tools_section = serde_json::to_string_pretty(&json!({ "tools": tools })).unwrap_or_else(|_| "".to_string());

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
            To call a tool, use this format:\n\n\
            Let me call [tool_name_here]\n\
            ```json\n\
            {{\n\
                params here\n\
            }}\n\
            ```\n\n\
            Remember to use proper JSON format when calling tools.",
            tools_section
        )
    }

    pub async fn new() -> Result<MCPHost> {
        // Try to get the AI provider from environment

        let model_name = "deepseek-chat".to_string();
        info!("Initializing DeepSeek client with model: {}", model_name);

        // Retrieve the DeepSeek API key from an environment variable
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .expect("DEEPSEEK_API_KEY not set. Please provide it in the environment.");

        info!("Got DeepSeek API key: {}", api_key);
        let client = DeepSeekClient::new(api_key, model_name);
        let ai_client = Some(Box::new(client) as Box<dyn AIClient>);

        if ai_client.is_none() {
            info!("No AI client configured. Set MCP_AI_PROVIDER and corresponding API key (OPENAI_API_KEY or GEMINI_API_KEY or ANTHROPIC_API_KEY)");
        }

        Ok(MCPHost {
            servers: Arc::new(Mutex::new(HashMap::new())),
            client_info: Implementation {
                name: "mcp-host".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            request_timeout: std::time::Duration::from_secs(120), // Increased timeout for long-running operations
            ai_client,
        })
    }

    pub async fn load_config(&self, config_path: &str) -> Result<()> {
        info!("Loading configuration from: {}", config_path);
        
        // Ensure config directory exists
        if let Some(parent) = std::path::Path::new(config_path).parent() {
            info!("Creating config directory if it doesn't exist: {}", parent.display());
            std::fs::create_dir_all(parent)?;
        }

        // Define the Config struct here
        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        struct ServerConfig {
            command: String,
            #[serde(default)]
            env: HashMap<String, String>,
        }

        #[derive(Debug, serde::Deserialize, serde::Serialize)]
        struct Config {
            #[serde(rename = "mcpServers")]
            servers: HashMap<String, ServerConfig>,
        }

        // Try to read existing config or create default
        let config_str = match std::fs::read_to_string(config_path) {
            Ok(content) => {
                info!("Found existing config file");
                content
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("Config file not found, creating default");
                let default_config = Config {
                    servers: HashMap::new(),
                };
                let default_str = serde_json::to_string_pretty(&default_config)?;
                std::fs::write(config_path, &default_str)?;
                default_str
            }
            Err(e) => return Err(e.into()),
        };

        info!("Parsing configuration JSON");
        let config: Config = serde_json::from_str(&config_str)?;
        
        info!("Found {} servers in config", config.servers.len());
        for (name, server_config) in config.servers {
            // Start each configured server
            let mut command = Command::new(&server_config.command);
            
            // Set environment variables if specified
            for (key, value) in server_config.env {
                command.env(key, value);
            }
            
            self.start_server_with_command(&name, command).await?;
        }
        
        Ok(())
    }

    async fn start_server_with_command(&self, name: &str, mut command: Command) -> Result<()> {
        info!("Starting server '{}' with command: {:?}", name, command);
        command.stdin(Stdio::piped())
               .stdout(Stdio::piped())
               .stderr(Stdio::piped());

        info!("Spawning server process");
        let mut child = command.spawn()?;
        let child_stdin = child.stdin.take().expect("Failed to get stdin");
        let stdin = Arc::new(Mutex::new(ChildStdin::from_std(child_stdin)?));

        let stdout = child.stdout.take().expect("Failed to get stdout");
        let stdout = Arc::new(Mutex::new(ChildStdout::from_std(stdout)?));

        let server = ManagedServer {
            name: name.to_string(),
            process: child,
            stdin,
            stdout,
            capabilities: None,
            initialized: false,
        };

        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.to_string(), server);
        }

        self.initialize_server(name).await?;

        Ok(())
    }

    pub async fn start_server(&self, name: &str, command: &str, args: &[String]) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        self.start_server_with_command(name, cmd).await
    }

    async fn initialize_server(&self, name: &str) -> Result<()> {
        info!("Initializing server '{}'", name);
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String(Uuid::new_v4().to_string()).into(),
            method: "initialize".to_string(),
            params: Some(json!({
                "capabilities": {
                    "roots": { "listChanged": true },
                    "sampling": {}
                },
                "clientInfo": self.client_info,
                "protocolVersion": "2025-03-26"
            })),
        };

        let response = self.send_request(name, request).await?;

        // Check for error response
        if let Some(error) = response.error {
            error!("RPC Error {}: {}", error.code, error.message);
            return Err(anyhow!("RPC Error {}: {}", error.code, error.message));
        }

        if let Some(result) = response.result {
            let capabilities: ServerCapabilities = serde_json::from_value(result)?;
            let mut servers = self.servers.lock().await;
            if let Some(server) = servers.get_mut(name) {
                server.capabilities = Some(capabilities);
                server.initialized = true;
            }
        }

        // Send initialized notification
        let notification = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String(Uuid::new_v4().to_string()).into(),
            method: "notifications/initialized".to_string(),
            params: None,
        };

        self.send_request(name, notification).await?;

        Ok(())
    }

    async fn send_request(&self, server_name: &str, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        debug!("\n=== Starting send_request ===");
        debug!("Server: {}", server_name);
        debug!("Request method: {}", request.method);
        let request_str = serde_json::to_string(&request)? + "\n";
        debug!("DEBUG: Sending request: {}", request_str.trim());
        
        // Create channels for stdin/stdout communication
        let (tx, mut rx) = mpsc::channel(1);
        
        // Get the server's I/O handles
        let (stdin, stdout): (Arc<Mutex<ChildStdin>>, Arc<Mutex<ChildStdout>>) = {
            let servers = self.servers.lock().await;
            let server = servers.get(server_name)
                .ok_or_else(|| anyhow::anyhow!("Server not found: {}", server_name))?;
            
            (Arc::clone(&server.stdin), Arc::clone(&server.stdout))
        };

        debug!("Spawning async task for request/response handling");
        // Write request and read response in a separate task
        tokio::spawn(async move {
            debug!("Async task started");
            // Write request
            {
                let request_bytes = request_str.as_bytes().to_vec(); // Clone the data
                debug!("Acquiring stdin lock");
                let mut stdin_guard = stdin.lock().await;
                debug!("Acquired stdin lock");
                if let Err(e) = stdin_guard.write_all(&request_bytes).await {
                    let _ = tx.send(Err(anyhow::anyhow!("Failed to write to stdin: {}", e))).await;
                    return;
                }
                if let Err(e) = stdin_guard.flush().await {
                    let _ = tx.send(Err(anyhow::anyhow!("Failed to flush stdin: {}", e))).await;
                    return;
                }
                // stdin_guard is dropped here
            }

            // Read response
            debug!("Starting response read");
            let mut response_line = String::new();
            {
                let mut stdout_guard = stdout.lock().await;
                let mut reader = BufReader::new(&mut *stdout_guard);
                
                match reader.read_line(&mut response_line).await {
                    Ok(0) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Server closed connection"))).await;
                    }
                    Ok(_) => {
                        debug!("DEBUG: Received response: {}", response_line.trim());
                        match serde_json::from_str(&response_line) {
                            Ok(response) => { let _ = tx.send(Ok(response)).await; }
                            Err(e) => { 
                                let _ = tx.send(Err(anyhow::anyhow!("Failed to parse response '{}': {}", response_line.trim(), e))).await; 
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Failed to read response: {}", e))).await;
                    }
                }
                // stdout_guard is dropped here
            }
        });

        // Wait for response with timeout
        match timeout(self.request_timeout, rx.recv()).await {
            Ok(Some(result)) => result,
            Ok(None) => Err(anyhow::anyhow!("Response channel closed")),
            Err(_) => Err(anyhow::anyhow!("Request timed out")),
        }
    }

    pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<ToolInfo>> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String(Uuid::new_v4().to_string()).into(),
            method: "tools/list".to_string(),
            params: None,
        };

        info!("Sending tool call request to server");
        let response = self.send_request(server_name, request).await?;
        info!("Received response from server");
        let tools: ListToolsResult = serde_json::from_value(response.result.unwrap_or_default())?;
        Ok(tools.tools)
    }

    pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: Value) -> Result<String> {
        debug!("call_tool started");
        debug!("Server: {}", server_name);
    
            
        debug!("Tool: {}", tool_name);
        debug!("Arguments: {}", serde_json::to_string_pretty(&args).unwrap_or_default());
        
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String(Uuid::new_v4().to_string()).into(),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": tool_name,
                "arguments": args
            })),
        };

        let response = self.send_request(server_name, request).await?;
        let result: CallToolResult = serde_json::from_value(response.result.unwrap_or_default())?;

        let mut output = String::new();
        for content in result.content {
            match content {
                content => {
                    match content.type_.as_str() {
                        "text" => {
                            output.push_str(&content.text);
                            output.push('\n');
                        }
                        _ => {
                            output.push_str(&format!("Unknown content type: {}\n", content.type_));
                        }
                    }
                }
            }
        }

        Ok(output)
    }

    pub async fn stop_server(&self, name: &str) -> Result<()> {
        let mut servers = self.servers.lock().await;
        if let Some(mut server) = servers.remove(name) {
            server.process.kill()?;
        }
        Ok(())
    }

    pub async fn run_cli(&self) -> Result<()> {
        info!("MCP Host CLI - Enter 'help' for commands");

        let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();

        while let Ok(Some(line)) = lines.next_line().await {
            let args: Vec<&str> = line.trim().split_whitespace().collect();
            if args.is_empty() {
                continue;
            }

            let command = args[0];
            let server_args = &args[1..];

            match command {
                "load_config" => {
                    if server_args.len() != 1 {
                        println!("Usage: load_config <config_file>");
                        continue;
                    }

                    let config_path = server_args[0];
                    match self.load_config(config_path).await {
                        Ok(()) => println!("Successfully loaded configuration from {}", config_path),
                        Err(e) => println!("Error loading configuration: {}", e),
                    }
                },
                "chat" => {
                    if server_args.len() != 1 {
                        info!("Usage: chat <server>");
                        continue;
                    }

                    let server_name = server_args[0];
                    match self.enter_chat_mode(server_name).await {
                        Ok(mut state) => {
                            println!("\nEntering chat mode. Type 'exit' or 'quit' to leave.");

                            loop {
                                println!("\nUser:");
                                let mut input = String::new();
                                std::io::stdin().read_line(&mut input)?;
                                let user_input = input.trim();
                                if user_input.eq_ignore_ascii_case("exit") || user_input.eq_ignore_ascii_case("quit") {
                                    info!("Exiting chat mode.");
                                    break;
                                }

                                state.add_user_message(user_input);

                                // Check if we have an AI client
                                if let Some(client) = &self.ai_client {
                                    println!("Using AI model: {}", client.model_name());
                                    
                                    let mut builder = client.raw_builder();
                                    
                                    // Combine all system messages into one
                                    let system_messages: Vec<String> = state.messages.iter()
                                        .filter_map(|msg| {
                                            if let Role::System = msg.role {
                                                Some(msg.content.clone())
                                            } else {
                                                None
                                            }
                                        })
                                        .collect();
                                    
                                    if !system_messages.is_empty() {
                                        builder = builder.system(system_messages.join("\n\n"));
                                    }

                                    // Add only the most recent user and assistant messages
                                    let mut recent_messages = state.messages.iter()
                                        .filter(|msg| matches!(msg.role, Role::User | Role::Assistant))
                                        .rev()
                                        .take(2)
                                        .collect::<Vec<_>>();
                                    recent_messages.reverse();

                                    for msg in recent_messages {
                                        match msg.role {
                                            Role::User => builder = builder.user(msg.content.clone()),
                                            Role::Assistant => builder = builder.assistant(msg.content.clone()),
                                            _ => {}
                                        }
                                    }

                                    match builder.execute().await {
                                        Ok(response_str) => {
                                            let response = response_str.as_str();
                                            println!("\nAssistant: {}", response);
                                            if let Err(e) = crate::conversation_service::handle_assistant_response(self, &response, server_name, &mut state, client, None).await {
                                                info!("Error handling assistant response: {}", e);
                                            }
                                        }
                                        Err(e) => info!("Error getting response: {}", e),
                                    }
                                } else {
                                    info!("Error: No AI client configured. Set OPENAI_API_KEY or GEMINI_API_KEY environment variable.");
                                    break;
                                }
                            }
                        }
                        Err(e) => info!("Error entering chat mode: {}", e),
                    }
                }
                "help" => {
                    println!("\nAvailable commands:");
                    println!("  load_config <file>  - Load servers from config file");
                    println!("  servers              - List running servers");
                    println!("  start <n> <command> [args]    - Start a server");
                    println!("  stop <server>                  - Stop a server");
                    println!("  tools <server>               - List tools for a server");
                    println!("  call <server> <tool>             - Call a tool with JSON arguments");
                    println!("  chat <server>               - Enter interactive chat mode with a server");
                    println!("  quit                         - Exit the program");
                }
                "servers" => {
                    let servers = self.servers.lock().await;
                    println!("\nRunning servers:");
                    for (name, server) in servers.iter() {
                        println!("  {} - initialized: {}", 
                            name,
                            if server.initialized { "yes" } else { "no" }
                        );
                    }
                }
                "start" => {
                    if server_args.len() < 2 {
                        info!("Usage: start <n> <command> [args...]");
                        continue;
                    }

                    let server_name = server_args[0];
                    let server_command = server_args[1];
                    let server_extra_args = server_args[2..].to_vec().into_iter().map(String::from).collect::<Vec<_>>();

                    match self.start_server(server_name, server_command, &server_extra_args).await {
                        Ok(()) => info!("Started server '{}'", server_name),
                        Err(e) => info!("Error starting server: {}", e),
                    }
                }
                "stop" => {
                    if server_args.len() != 1 {
                        info!("Usage: stop <server>");
                        continue;
                    }

                    let server_name = server_args[0];
                    match self.stop_server(server_name).await {
                        Ok(()) => info!("Stopped server '{}'", server_name),
                        Err(e) => info!("Error stopping server: {}", e),
                    }
                }
                "tools" => {
                    if server_args.len() != 1 {
                        info!("Usage: tools <server>");
                        continue;
                    }

                    let server_name = server_args[0];
                    match self.list_server_tools(server_name).await {
                        Ok(tools) => {
                            info!("\nAvailable tools for {}:", server_name);
                            for tool in tools {
                                info!("  {} - {}", tool.name, tool.description.unwrap_or_default());
                                let schema = tool.input_schema;
                                info!("    Arguments schema:");
                                info!("{}", serde_json::to_string_pretty(&schema)?
                                    .split('\n')
                                    .map(|line| format!("      {}", line))
                                    .collect::<Vec<_>>()
                                    .join("\n"));
                            }
                        }
                        Err(e) => info!("Error: {}", e),
                    }
                }
                "call" => {
                    if server_args.len() != 2 {
                        info!("Usage: call <server> <tool>");
                        continue;
                    }

                    let server_name = server_args[0];
                    let tool_name = server_args[1];

                    info!("Enter arguments (JSON):");
                    let mut json_input = String::new();
                    let stdin = io::stdin(); // Standard input stream
                    stdin.read_line(&mut json_input)?;

                    let args_value: Value = match serde_json::from_str(&json_input) {
                        Ok(v) => v,
                        Err(e) => {
                            info!("Invalid JSON: {}", e);
                            continue;
                        }
                    };

                    match self.call_tool(server_name, tool_name, args_value).await {
                        Ok(result) => {
                            if result.trim().is_empty() {
                                println!("\nNo results returned");
                            } else {
                                println!("\nResult:");
                                if result.trim().starts_with('{') || result.trim().starts_with('[') {
                                    // Pretty print JSON
                                    if let Ok(json) = serde_json::from_str::<Value>(&result) {
                                        println!("```json\n{}\n```", serde_json::to_string_pretty(&json)?);
                                    } else {
                                        println!("{}", result);
                                    }
                                } else {
                                    println!("{}", result);
                                }
                            }
                        }
                        Err(e) => println!("Error calling tool: {}", e),
                    }
                }
                "quit" => break,
                _ => info!("Unknown command. Type 'help' for available commands."),
            }
        }

        Ok(())
    }
}