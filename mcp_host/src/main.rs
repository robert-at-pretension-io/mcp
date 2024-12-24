use anyhow::Result;
use console::{style, Term};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct ToolChain {
    title: String,
    steps: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ToolChainLibrary(Vec<ToolChain>);

impl ToolChainLibrary {
    fn load() -> Result<Self> {
        let chains_json = include_str!("tool_chaining.json");
        serde_json::from_str(chains_json).map_err(|e| anyhow!("Failed to parse tool chains: {}", e))
    }

    fn get_examples(&self, limit: Option<usize>) -> String {
        let chains = match limit {
            Some(n) => self.0.iter().take(n),
            None => self.0.iter().take(self.0.len()) // Use take() with full length for None case
        };

        chains.map(|chain| {
            format!(
                "Example Workflow: {}\nSteps:\n{}\n",
                style(&chain.title).cyan().bold(),
                chain.steps.iter()
                    .enumerate()
                    .map(|(i, step)| format!("{}. {}", i + 1, step))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n\n")
    }
}
mod ai_client;
mod anthropic;
mod gemini;
mod streaming;

use log::{info,warn};
use tokio::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    command: String,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(rename = "mcpServers")]
    servers: HashMap<String, ServerConfig>,
}

use anthropic::AnthropicClient;

use ai_client::AIClient;


use shared_protocol_objects::Role;
mod conversation_state;
use conversation_state::ConversationState;
use std::io;
use anyhow::anyhow;
use log::{error,debug};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::timeout;
use uuid::Uuid;
use regex::Regex;
use lazy_static::lazy_static;

async fn with_progress<F, T>(msg: String, future: F) -> T 
where
    F: std::future::Future<Output = T>,
{
    let term = Term::stderr();
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut i = 0;
    
    // Clone the message and term for the spawned task
    let progress_msg = msg.clone();
    let progress_term = term.clone();
    
    let handle = tokio::spawn(async move {
        loop {
            // Write the spinner and message, staying on same line
            progress_term.write_str(&format!("\r{} {}", spinner[i], progress_msg))
                .unwrap_or_default();
            // Ensure the line is flushed
            progress_term.flush().unwrap_or_default();
            
            i = (i + 1) % spinner.len();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let result = future.await;
    handle.abort();
    // Clear the progress line completely
    term.clear_line().unwrap_or_default();
    result
}

// Helper functions for parsing tool calls
fn extract_json_after_position(text: &str, pos: usize) -> Option<Value> {
    if let Some(json_start) = text[pos..].find('{') {
        let start_pos = pos + json_start;
        let mut brace_count = 0;
        let mut end_pos = start_pos;
        
        for (i, c) in text[start_pos..].chars().enumerate() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start_pos + i + 1;
                        break;
                    }
                }
                _ => continue,
            }
        }

        if brace_count == 0 {
            if let Ok(json) = serde_json::from_str(&text[start_pos..end_pos]) {
                return Some(json);
            }
        }
    }
    None
}

fn find_any_json(text: &str) -> Option<Value> {
    let mut start_indices: Vec<usize> = text.match_indices('{').map(|(i, _)| i).collect();
    start_indices.sort_unstable(); // Sort in case there are multiple JSON objects

    for start in start_indices {
        let mut brace_count = 0;
        let mut end_pos = start;
        
        for (i, c) in text[start..].chars().enumerate() {
            match c {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        end_pos = start + i + 1;
                        break;
                    }
                }
                _ => continue,
            }
        }

        if brace_count == 0 {
            if let Ok(json) = serde_json::from_str(&text[start..end_pos]) {
                return Some(json);
            }
        }
    }
    None
}

fn infer_tool_from_json(json: &Value) -> Option<(String, Value)> {
    // Common patterns to identify tools
    if json.get("action").is_some() {
        return Some(("graph_tool".to_string(), json.clone()));
    }
    if json.get("query").is_some() {
        return Some(("brave_search".to_string(), json.clone()));
    }
    if json.get("url").is_some() {
        return Some(("scrape_url".to_string(), json.clone()));
    }
    if json.get("command").is_some() {
        return Some(("bash".to_string(), json.clone()));
    }
    if json.get("sequential_thinking").is_some() {
        return Some(("sequential_thinking".to_string(), json.clone()));
    }
    if json.get("memory").is_some() {
        return Some(("memory".to_string(), json.clone()));
    }
    if json.get("task_planning").is_some() {
        return Some(("task_planning".to_string(), json.clone()));
    }
    
    // If we can't infer the tool, return None
    None
}

// Main tool call parsing function
fn parse_tool_call(response: &str) -> Option<(String, Value)> {
    lazy_static! {
        // Multiple patterns to match different formats
        static ref TOOL_PATTERNS: Vec<Regex> = vec![
            // Standard format with tool keyword
            Regex::new(
                r"(?s)Let me call the `([^`]+)` tool with these parameters:\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            // Without "tool" keyword
            Regex::new(
                r"(?s)Let me call `([^`]+)` with these parameters:\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            // More variations
            Regex::new(
                r"(?s)Using the `([^`]+)` tool:?\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            Regex::new(
                r"(?s)I'll use `([^`]+)`:?\s*```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            // Tool name with JSON anywhere
            Regex::new(
                r"(?s)`([^`]+)`.*?```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
            // Just the JSON block
            Regex::new(
                r"(?s)```(?:json)?\s*(\{.*?\})\s*```"
            ).unwrap(),
        ];
    }

    // First try: Look for explicit tool name and JSON patterns
    for pattern in TOOL_PATTERNS.iter() {
        if let Some(captures) = pattern.captures(response) {
            match captures.len() {
                // Just JSON block
                2 => {
                    if let Ok(json) = serde_json::from_str(&captures[1]) {
                        // Try to infer tool from JSON content
                        return infer_tool_from_json(&json);
                    }
                },
                // Tool name and JSON
                3 => {
                    let tool_name = captures[1].to_string();
                    if let Ok(args) = serde_json::from_str(&captures[2]) {
                        return Some((tool_name, args));
                    }
                },
                _ => continue,
            }
        }
    }

    // Second try: Look for backtick-wrapped tool names and nearby JSON
    if let Some(tool_start) = response.find('`') {
        if let Some(tool_end) = response[tool_start + 1..].find('`') {
            let tool_name = response[tool_start + 1..tool_start + 1 + tool_end].to_string();
            // Look for JSON after the tool name
            if let Some(json) = extract_json_after_position(response, tool_start + tool_end) {
                return Some((tool_name, json));
            }
        }
    }

    // Third try: Look for any JSON object and try to infer the tool
    if let Some(json) = find_any_json(response) {
        return infer_tool_from_json(&json);
    }

    None
}

use shared_protocol_objects::{
    JsonRpcRequest, JsonRpcResponse, ServerCapabilities, Implementation,
    ToolInfo, CallToolResult, RequestId, ListToolsResult
};

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
    servers: Arc<Mutex<HashMap<String, ManagedServer>>>,
    client_info: Implementation,
    request_timeout: std::time::Duration,
    ai_client: Option<Box<dyn AIClient>>,
}

impl MCPHost {
    pub async fn enter_chat_mode(&self, server_name: &str) -> Result<ConversationState> {
        // Fetch tools from the server
        let tool_info_list = self.list_server_tools(server_name).await?;

        // Load tool chain examples
        let tool_chains = ToolChainLibrary::load().unwrap_or_else(|e| {
            warn!("Failed to load tool chains: {}. Using empty library.", e);
            ToolChainLibrary(vec![])
        });

        // Convert our tool list to a JSON structure
        let tools_json: Vec<serde_json::Value> = tool_info_list.iter().map(|t| {
            json!({
                "name": t.name,
                "description": t.description.as_ref().unwrap_or(&"".to_string()),
                "inputSchema": t.input_schema
            })
        }).collect();

        // Generate system prompt with tool chains
        let system_prompt = format!(
            "{}\n\nTOOL CHAINING EXAMPLES:\n{}\n\nREMEMBER:\n\
            1. Always use proper JSON-RPC format for tool calls\n\
            2. Chain tools together when appropriate\n\
            3. Consider the examples above when planning tool usage\n\
            4. Maintain context between tool calls\n\
            5. Handle tool responses appropriately",
            self.generate_system_prompt(&tools_json),
            tool_chains.get_examples(Some(5)) // Show 5 examples by default
        );

        // Create the conversation state
        let mut state = ConversationState::new(system_prompt, tool_info_list.clone());
        
        // Create a hidden instruction message that combines static guidance with dynamic tool info
        let hidden_instruction = format!(
            "[CRITICAL INSTRUCTION - FOLLOW EXACTLY]\n\
            When using tools, follow these patterns from the examples:\n\
            {}\n\
            \n\
            PROACTIVE REQUIREMENTS:\n\
            1. Create knowledge graph nodes for EVERY:\n\
               - Statement made\n\
               - Preference shared\n\
               - Opinion expressed\n\
               - Question asked\n\
               - Problem discussed\n\
               - Goal mentioned\n\
               - Experience described\n\
               - Relationship identified\n\
               - Decision\n\
               - Skill mentioned\n\
            2. BEFORE EVERY RESPONSE:\n\
               - Search existing nodes for context\n\
               - Look for related information\n\
               - Identify connection opportunities\n\
               - Prepare relevant suggestions\n\
            3. AFTER EVERY USER MESSAGE:\n\
               - Create new nodes immediately\n\
               - Connect to existing knowledge\n\
               - Update related nodes\n\
            4. PROACTIVELY OFFER:\n\
               - Related information searches\n\
               - Deeper topic exploration\n\
               - Connection to related topics\n\
               - Resource discovery\n\
            5. SUGGEST NEXT ACTIONS:\n\
               - Related topics to explore\n\
               - Relevant searches to perform\n\
               - Connections to investigate\n\
               - Resources to examine\n\
            \n\
            TOOL USAGE PATTERN:\n\
            1. ALWAYS run initial tools:\n\
               - get_top_tags\n\
               - get_tags_by_date\n\
               - get_most_connected\n\
               - search_nodes (for context)\n\
            2. AFTER EVERY USER MESSAGE:\n\
               - Create/update nodes\n\
               - Make connections\n\
               - Search for relevance\n\
            3. BEFORE EVERY RESPONSE:\n\
               - Check existing knowledge\n\
               - Search for related info\n\
               - Prepare suggestions\n\
            \n\
            CONVERSATION STYLE:\n\
            - Be proactive with suggestions\n\
            - Offer relevant searches\n\
            - Suggest related explorations\n\
            - Connect topics naturally\n\
            - Show enthusiasm for discovery\n\
            \n\
            AVAILABLE TOOLS AND THEIR REQUIRED USAGE PATTERNS:\n\
            {}\n\
            \n\
            EXAMPLE TOOL CHAINS:\n\
            {}\n\
            \n\
            CRITICAL: NEVER WAIT FOR PERMISSION TO USE TOOLS!",
            tool_chains.get_examples(Some(3)), // Show 3 examples in the hidden instruction
            tool_info_list.iter().map(|tool| {
                format!(
                    "Tool: {}\n\
                    Description: {}\n\
                    Usage Requirements: {}\n\
                    Schema: {}\n",
                    tool.name,
                    tool.description.as_ref().unwrap_or(&"".to_string()),
                    // Extract any usage requirements from description (usually in caps or after "ALWAYS")
                    tool.description.as_ref()
                        .unwrap_or(&"".to_string())
                        .lines()
                        .filter(|line| line.contains("ALWAYS") || line.contains("MUST") || line.contains("NEVER"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default()
                )
            }).collect::<Vec<_>>().join("\n\n"),
            tool_chains.get_examples(Some(3))
        );
        
        // Add the hidden instruction as a user message instead of a system message
        state.add_user_message(&hidden_instruction);
        
        // Create a hidden instruction message that combines static guidance with dynamic tool info
        let hidden_instruction = format!(
            "[ASSISTANT INSTRUCTION - FOLLOW ALL GUIDELINES STRICTLY]\n\
            GENERAL PRINCIPLES:\n\
            - Use tools proactively but don't acknowledge the response data directly in each response, instead use it to inform the conversation\n\
            - Chain tools together to build internal context\n\
            - Run preliminary tools before responding\n\
            - Follow each tool's usage patterns exactly as described\n\
            - Maintain a natural conversation flow - avoid dumping raw tool outputs to the user\n\
            - Don't wait for explicit user requests to use tools\n\n\
            INFORMATION USAGE:\n\
            - DO NOT repeat tool outputs verbatim to the user\n\
            - DO NOT say \"I see from the graph...\" - infer things naturally and provide that info to the user.\n\
            - Use retrieved information to inform and personalize responses\n\
            - Keep tool usage subtle and behind-the-scenes\n\
            - Focus on natural conversation, not data reporting\n\
            - Only reference known information when contextually relevant\n\n\
            TOOL USAGE PATTERN:\n\
            1. Gather context from tools first\n\
            2. Process and analyze results internally\n\
            3. Use insights to shape natural responses\n\
            4. Store new information continuously\n\n\
            EXAMPLE INTERACTIONS:\n\
            - BAD: \"I see from the graph that you like pizza and work as a developer\"\n\
            - GOOD: \"Since you're familiar with software development, you might find this interesting...\"\n\
            - BAD: \"According to my records, you mentioned having a dog named Max\"\n\
            - GOOD: \"How's Max doing? Still enjoying those long walks?\"\n\n\
            AVAILABLE TOOLS AND THEIR REQUIRED USAGE PATTERNS:\n{}\n\n\
            ",
            tool_info_list.iter().map(|tool| {
                format!(
                    "Tool: {}\n\
                    Description: {}\n\
                    Usage Requirements: {}\n\
                    Schema: {}\n",
                    tool.name,
                    tool.description.as_ref().unwrap_or(&"".to_string()),
                    // Extract any usage requirements from description (usually in caps or after "ALWAYS")
                    tool.description.as_ref()
                        .unwrap_or(&"".to_string())
                        .lines()
                        .filter(|line| line.contains("ALWAYS") || line.contains("MUST") || line.contains("NEVER"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    serde_json::to_string_pretty(&tool.input_schema).unwrap_or_default()
                )
            }).collect::<Vec<_>>().join("\n\n")
        );
        
        // Add the hidden instruction as a user message instead of a system message
        state.add_user_message(&hidden_instruction);

        // Add startup reminder as system message
        let startup_reminder = format!(
            "Consider these example tool chains for guidance:\n\
            {}\n\
            \n\
            DO NOT PROCEED WITH USER INTERACTION UNTIL THESE ARE COMPLETE. \n\
            WHEN TOOL CALLING, MAKE SURE YOU INCLUDE THE JSON TO MAKE THE REQUEST OR THE TOOL WILL NOT BE INVOKED!
            This is a strict requirement, not a suggestion.",
            tool_chains.get_examples(Some(15)) // Show 15 relevant examples
        );

        state.add_system_message(&startup_reminder);

        Ok(state)
    }

    fn generate_system_prompt(&self, tools: &[serde_json::Value]) -> String {
        let tools_section = serde_json::to_string_pretty(&json!({ "tools": tools })).unwrap_or("".to_string());

        let prompt = format!(r####"You are a proactive assistant with access to powerful tools. FOLLOW TOOL DESCRIPTIONS EXACTLY.

CORE RESPONSIBILITIES:
1. CONTINUOUSLY CREATE KNOWLEDGE GRAPH NODES for:
   - Every piece of user information shared
   - All preferences mentioned
   - Any problems discussed
   - Goals and aspirations
   - Experiences described
   - Skills and expertise
   - Relationships mentioned
   - Decisions made
   - Questions asked
   - Opinions expressed

2. PROACTIVELY USE TOOLS:
   - Search for relevant information before responding
   - Suggest related resources and information
   - Offer to explore topics deeper using available tools
   - Connect new information to existing knowledge
   - Keep the knowledge graph current and interconnected

3. BE SUGGESTIVE AND HELPFUL:
   - "I could search for more information about X..."
   - "Would you like me to find resources related to Y?"
   - "I notice you're interested in Z, I can explore that topic further..."
   - "Based on your previous interests in A, you might want to learn about B..."
   - "I can help you discover more about this topic using our search tools..."

MANDATORY BEHAVIOR:
1. ALWAYS create nodes for new information
2. ALWAYS connect new nodes to related existing ones
3. ALWAYS suggest relevant tool usage to users
4. ALWAYS check existing knowledge before asking questions
5. NEVER wait for user permission to maintain the knowledge graph
6. CONTINUOUSLY offer to use tools proactively

CONVERSATION FLOW:
1. Start by checking existing knowledge
2. Proactively suggest relevant information
3. Create nodes for all new information
4. Connect information across nodes
5. Offer to explore topics deeper
6. Suggest related areas to explore

TOOL USAGE EXAMPLES:
- "Let me search for recent information about that..."
- "I'll check if we have related information in our knowledge graph..."
- "I can fetch some resources about this topic..."
- "Would you like me to explore any of these related areas?"
- "I notice this connects to some other topics we've discussed..."

{tools_section}

REMEMBER:
- Create nodes for EVERYTHING the user shares
- Be proactive with tool suggestions
- Continuously offer relevant information
- Keep the knowledge graph updated
- Make connections between topics
- Suggest related explorations

TOOL CALLING FORMAT:
To call a tool, use this exact format:

Let me call [tool_name_here]
```json
{{
    params here
}}
```

# The most important thing: unless you actually follow that format and return that text, a tool will NOT be called. You can invoke MULTIPLE tools in one response by returning multiple tool calling json blocks. NEVER say you are calling a tool then leave out a json block.

When you get information, don't mention it. Just use it to subtly inform the conversation going forward. It's jarring for the user when you info dump on them stuff they already know about their lives.
"####);

        prompt
    }

    pub async fn new() -> Result<Self> {
        // Try to get the AI provider from environment
        let ai_provider = std::env::var("MCP_AI_PROVIDER").unwrap_or_else(|_| {
            info!("MCP_AI_PROVIDER not set, defaulting to 'gemini'");
            "gemini".to_string()
        });

        // Get the model name from environment or use provider-specific defaults
        // let model_name = std::env::var("MCP_AI_MODEL").unwrap_or_else(|_| {
        //     match ai_provider.as_str() {
        //         "openai" => {
        //             info!("MCP_AI_MODEL not set, defaulting to 'gpt-4-1106-preview' for OpenAI");
        //             "gpt-4-1106-preview".to_string()
        //         }
        //         "gemini" => {
        //             info!("MCP_AI_MODEL not set, defaulting to 'gemini-pro' for Gemini");
        //             "gemini-pro".to_string()
        //         }
        //         _ => "gpt-4-1106-preview".to_string()
        //     }
        // });

        // let model_name = "gemini-2.0-flash-exp".to_string();
        // info!("Initializing Gemini client with model: {}", model_name);
        // // Get Google Cloud auth token using gcloud command
        // let output = std::process::Command::new("gcloud")
        //     .args(["auth", "print-access-token"])
        //     .output()
        //     .expect("Failed to execute gcloud command");
        // let api_key = String::from_utf8(output.stdout)
        //     .expect("Invalid UTF-8 in gcloud output")
        //     .trim()
        //     .to_string();
        // info!("Got Google Cloud auth token: {}", api_key);
        // let client = GeminiClient::new(api_key, model_name);
        // let ai_client = Some(Box::new(client) as Box<dyn AIClient>);

        // let model_name = "gpt-4o".to_string();
        // info!("Initializing OpenAI client with model: {}", model_name);

        // // Retrieve the OpenAI API key from an environment variable
        // let api_key = std::env::var("OPENAI_API_KEY")
        //     .expect("OPENAI_API_KEY not set. Please provide it in the environment.");

        // info!("Got OpenAI API key: {}", api_key);
        // let client = OpenAIClient::new(api_key, model_name);
        // let ai_client = Some(Box::new(client) as Box<dyn AIClient>);

        let model_name = "claude-3-5-sonnet-20241022".to_string();
        info!("Initializing Anthropic client with model: {}", model_name);

        // Retrieve the Anthropic API key from an environment variable
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY not set. Please provide it in the environment.");

        info!("Got Anthropic API key: {}", api_key);
        let client = AnthropicClient::new(api_key, model_name);
        let ai_client = Some(Box::new(client) as Box<dyn AIClient>);



        if ai_client.is_none() {
            info!("No AI client configured. Set MCP_AI_PROVIDER and corresponding API key (OPENAI_API_KEY or GEMINI_API_KEY or ANTHROPIC_API_KEY)");
        }

        Ok(Self {
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
            fs::create_dir_all(parent)?;
        }

        // Try to read existing config or create default
        let config_str = match fs::read_to_string(config_path) {
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
                fs::write(config_path, &default_str)?;
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
                "protocolVersion": "2024-11-05"
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
        let (stdin, stdout) = {
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
            })).into(),
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

    async fn handle_assistant_response(
        &self,
        response: &str,
        server_name: &str,
        state: &mut ConversationState,
        client: &Box<dyn AIClient>,
    ) -> Result<()> {
        state.add_assistant_message(response);
        
        let mut current_response = response.to_string();
        let mut iteration = 0;
        const MAX_ITERATIONS: i32 = 15;
        
        while iteration < MAX_ITERATIONS {
            debug!("\nStarting iteration {} of response handling", iteration + 1);
            
            let mut found_tool_call = false;
            let chunks: Vec<&str> = current_response.split("```").collect();
            for (i, chunk) in chunks.iter().enumerate() {
                if i % 2 == 1 {
                    if let Some((tool_name, args)) = parse_tool_call(chunk) {
                        found_tool_call = true;
                        debug!("Found tool call in chunk {}:", i);
                        debug!("Tool: {}", tool_name);
                        debug!("Arguments: {}", serde_json::to_string_pretty(&args).unwrap_or_default());
                        
                        println!("{}", style("\nTool Call:").green().bold());
                        println!("└─ {}: {}\n", 
                            style(&tool_name).yellow(),
                            conversation_state::format_json_output(&serde_json::to_string_pretty(&args)?));
    
                        match self.call_tool(server_name, &tool_name, args).await {
                            Ok(result) => {
                                println!("{}", conversation_state::format_tool_response(&tool_name, &result));
                                // Add tool response as assistant message, trimming whitespace
                                state.add_assistant_message(&format!("Tool '{}' returned: {}", tool_name, result.trim()));
                            }
                            Err(e) => {
                                println!("{}: {}\n", style("Error").red().bold(), e);
                                // Add error as assistant message to handle gracefully, trimming whitespace
                                state.add_assistant_message(&format!("Tool '{}' error: {}", tool_name, e).trim());
                            }
                        }
                    }
                }
            }
            
            if !found_tool_call {
                break;
            }
            
            // Send all messages (including the tool results now marked as user messages) back to AI
            let mut builder = client.raw_builder();
            for msg in &state.messages {
                match msg.role {
                    Role::System => builder = builder.system(msg.content.clone()),
                    Role::User => builder = builder.user(msg.content.clone()),
                    Role::Assistant => builder = builder.assistant(msg.content.clone()),
                }
            }
    
            debug!("Sending updated conversation to AI");
            let timeout_result = with_progress("Waiting for AI response...".to_string(),
                tokio::time::timeout(std::time::Duration::from_secs(30), builder.execute())
            ).await;
    
            match timeout_result {
                Ok(execute_result) => match execute_result {
                    Ok(response_string) => {
                        println!("\n{}", conversation_state::format_chat_message(&Role::Assistant, &response_string));
                        state.add_assistant_message(&response_string);
                        current_response = response_string;
                    }
                    Err(e) => {
                        info!("Error getting response from API: {}", e);
                        break;
                    }
                },
                Err(_) => {
                    info!("API request timed out after 30 seconds");
                    break;
                }
            }
            
            iteration += 1;
        }
        
        if iteration >= MAX_ITERATIONS {
            info!("Warning: Reached maximum number of tool call iterations");
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
                        println!("{}: load_config <config_file>", style("Usage").cyan().bold());
                        continue;
                    }

                    let config_path = server_args[0];
                    match self.load_config(config_path).await {
                        Ok(()) => println!("{} {}", style("Successfully loaded configuration from").green().bold(), config_path),
                        Err(e) => println!("{}: {}", style("Error loading configuration").red().bold(), e),
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
                            println!("\n{}", style("Entering chat mode. Type 'exit' or 'quit' to leave.").cyan().bold());

                            loop {
                                println!("\n{}", style("User:").cyan().bold());
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
                                    println!("Using AI model: {}", style(client.model_name()).yellow());
                                    
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
                                            println!("\n{}: {}", style("Assistant").cyan().bold(), response);
                                            if let Err(e) = self.handle_assistant_response(&response, server_name, &mut state, client).await {
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
                    println!("\n{}", style("Available commands:").cyan().bold());
                    println!("  {}  - Load servers from config file", style("load_config <file>").yellow());
                    println!("  {}              - List running servers", style("servers").yellow());
                    println!("  {}    - Start a server", style("start <name> <command> [args]").yellow());
                    println!("  {}                  - Stop a server", style("stop <server>").yellow());
                    println!("  {}               - List tools for a server", style("tools <server>").yellow());
                    println!("  {}             - Call a tool with JSON arguments", style("call <server> <tool>").yellow());
                    println!("  {}               - Enter interactive chat mode with a server", style("chat <server>").yellow());
                    println!("  {}                         - Exit the program", style("quit").yellow());
                }
                "servers" => {
                    let servers = self.servers.lock().await;
                    println!("\n{}", style("Running servers:").cyan().bold());
                    for (name, server) in servers.iter() {
                        println!("  {} - initialized: {}", 
                            style(name).yellow(),
                            if server.initialized { style("yes").green() } else { style("no").red() }
                        );
                    }
                    // info!();
                }
                "start" => {
                    if server_args.len() < 2 {
                        info!("Usage: start <name> <command> [args...]");
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
                            // info!();
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
                                println!("\n{}", style("No results returned").yellow());
                            } else {
                                println!("\n{}", style("Result:").cyan().bold());
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
                        Err(e) => println!("{}: {}", style("Error calling tool").red().bold(), e),
                    }
                }
                "quit" => break,
                _ => info!("Unknown command. Type 'help' for available commands."),
            }
        }

        Ok(())
    }
}

mod web_interface;

use axum::Router;
use tower_http::trace::TraceLayer;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Starting mcp_host application");

    info!("Initializing MCPHost");
    let host = MCPHost::new().await?;
    info!("MCPHost initialized successfully");

    let args: Vec<String> = std::env::args().collect();
    
    // If arguments are provided, handle them first
    if args.len() > 1 {
        info!("Processing command line arguments: {:?}", &args[1..]);
        match args[1].as_str() {
            "load_config" if args.len() == 3 => {
                let config_path = &args[2];
                match host.load_config(config_path).await {
                    Ok(()) => {
                        info!("Successfully loaded configuration from {}", config_path);
                        // Continue to CLI after successful config load
                    }
                    Err(e) => {
                        warn!("Error loading configuration: {}. Starting with empty config.", e);
                        // Continue to CLI even if config load failed
                    }
                }
            }
            _ => {
                info!("Invalid command line arguments");
                info!("Usage: {} load_config <config_file>", args[0]);
                return Ok(());
            }
        }
    }

    // Check if we should run in web mode
    if args.len() > 1 && args[1] == "web" {
        info!("Starting web interface");
        
        let ai_client = host.ai_client.clone()
            .ok_or_else(|| anyhow::anyhow!("No AI client configured"))?;

        let app_state = web_interface::WebAppState::new(Arc::new(ai_client));
        let app = Router::new()
            .route("/", axum::routing::get(web_interface::root))
            .route("/ask", axum::routing::post(web_interface::ask))
            .route("/sse/:session_id", axum::routing::get(web_interface::sse_handler))
            .with_state(app_state)
            .layer(TraceLayer::new_for_http());

        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        println!("Web interface running at http://{}", addr);

        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
    } else {
        // Run in CLI mode
        info!("Starting CLI interface");
        host.run_cli().await?;

        // Stop all servers before exit
        let servers = host.servers.lock().await;
        for name in servers.keys() {
            let _ = host.stop_server(name).await;
        }
    }

    Ok(())
}
