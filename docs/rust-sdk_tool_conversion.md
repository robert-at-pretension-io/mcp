Conversion Plan: Migrating from `shared_protocol_objects` to the Official Rust MCP SDK (`rmcp`)
1. Project Dependencies
Remove `shared_protocol_objects` from `Cargo.toml` and add `rmcp`:
```toml
[workspace.dependencies]
# Remove: shared_protocol_objects = { path = "../shared_protocol_objects" }
# Add:
rmcp = { version = "0.1.5", features = ["server", "client", "transport-child-process", "transport-io"] } # Adjust features as needed
# Keep other dependencies
```
2. Core Type Mappings
| Old (`shared_protocol_objects`) | New (`rmcp`)                                                              | Notes                                                                 |
| :------------------------------ | :------------------------------------------------------------------------ | :-------------------------------------------------------------------- |
| `ToolInfo`                      | `rmcp::model::Tool`                                                       | Schema is `Arc<JsonObject>`                                           |
| `CallToolParams`                | `rmcp::model::CallToolRequestParam`                                       | Arguments are `Option<JsonObject>`                                    |
| `CallToolResult`                | `rmcp::model::CallToolResult`                                             | Contains `Vec<rmcp::model::Content>`                                  |
| `Content`                       | `rmcp::model::Content` / `rmcp::model::RawContent`                        | `Content` wraps `RawContent` (enum: Text, Image, Resource, Audio) |
| `JsonRpcRequest`                | `rmcp::model::JsonRpcRequest`                                             |                                                                       |
| `JsonRpcResponse`               | `rmcp::model::JsonRpcResponse`                                            |                                                                       |
| `JsonRpcNotification`           | `rmcp::model::JsonRpcNotification`                                        |                                                                       |
| `rpc::Transport` trait          | `rmcp::transport::IntoTransport` trait / Specific transport types         | e.g., `rmcp::transport::child_process::TokioChildProcess`             |
| `rpc::McpClient`                | `rmcp::service::Peer<RoleClient>`                                         | `Peer` handles communication after `serve_client`                     |
| `rpc::ProcessTransport`         | `rmcp::transport::child_process::TokioChildProcess`                       | Used with `serve_client`                                              |
| `Role` enum                     | `rmcp::model::Role` (User, Assistant)                                     | `System` role needs separate handling (e.g., in `ConversationState`)  |
| `Error` / `anyhow::Error`       | `rmcp::Error` / `rmcp::ServiceError`                                      | Use SDK's error types                                                 |

3. Tool Implementation Changes
Use `rmcp`'s `#[tool]` and `ServerHandler` system.
A. Define Parameter Structs:
```rust
use serde::Deserialize;
use schemars::JsonSchema;

#[derive(Deserialize, JsonSchema)]
pub struct MyToolParams {
    #[schemars(description = "Description of parameter1")]
    pub parameter1: String,
    pub parameter2: i32,
}
```
B. Implement the Tool Logic:
```rust
use rmcp::{tool, ServerHandler, model::ServerInfo};

#[derive(Debug, Clone)]
pub struct MyTool;

impl MyTool {
    pub fn new() -> Self { Self } // Add constructor if needed
}

#[tool(tool_box)] // Apply tool_box to the impl block
impl MyTool {
    #[tool(description = "A description of what this tool does")]
    async fn my_tool_method(
        &self,
        #[tool(aggr)] params: MyToolParams // Use #[tool(aggr)] for the struct
    ) -> String { // Return String, Result<String>, Content, Result<Content>, etc.
        // Tool implementation
        format!("Result: {} {}", params.parameter1, params.parameter2)
    }

    // Add other tool methods here...
}

// Optionally implement ServerHandler if this struct represents the whole server
// If combining tools, implement ServerHandler on the container struct (see step 4)
#[tool(tool_box)] // Apply tool_box to ServerHandler impl as well
impl ServerHandler for MyTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: Some("MyTool Server".into()),
            version: Some("1.0.0".into()),
            instructions: Some("Instructions for using this tool server.".into()),
            ..Default::default()
        }
    }
}
```
4. Main Server Implementation (`mcp_tools/src/main.rs`)
Combine multiple tools into a single server handler.
```rust
use rmcp::{ServiceExt, ServerHandler, model::ServerInfo, transport::stdio, tool};
// Import your tool structs (BashTool, ScrapingBeeTool, etc.)
use mcp_tools::bash::{BashTool, BashParams};
use mcp_tools::scraping_bee::{ScrapingBeeTool, ScrapingBeeParams};
// ... other tool imports

#[derive(Debug, Clone)]
struct McpToolServer {
    bash_tool: BashTool,
    scraping_tool: ScrapingBeeTool,
    // ... other tool instances
}

impl McpToolServer {
    fn new() -> Self {
        Self {
            bash_tool: BashTool::new(),
            scraping_tool: ScrapingBeeTool::new(),
            // ... initialize other tools
        }
    }
}

// Implement tool methods by delegating to the contained instances
#[tool(tool_box)]
impl McpToolServer {
    #[tool(description = "Executes bash shell commands...")] // Copy description
    async fn bash(&self, #[tool(aggr)] params: BashParams) -> String {
        self.bash_tool.bash(params).await // Delegate
    }

    #[tool(description = "Web scraping tool...")] // Copy description
    async fn scrape_url(&self, #[tool(aggr)] params: ScrapingBeeParams) -> String {
        self.scraping_tool.scrape_url(params).await // Delegate
    }

    // ... implement delegation for all other tools
}

// Implement ServerHandler for the container struct
#[tool(tool_box)] // This automatically implements list_tools and call_tool
impl ServerHandler for McpToolServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: Some("MCP Tools Server (SDK)".into()),
            version: Some(env!("CARGO_PKG_VERSION").into()),
            description: Some("Provides various tools like bash execution and web scraping.".into()),
            instructions: Some("Use 'call' with tool name and parameters.".into()),
            ..Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging...

    info!("Setting up tools with rmcp SDK...");
    let mcp_server = McpToolServer::new(); // Create the container server
    info!("McpToolServer created with tools.");

    // Serve the McpToolServer instance using stdio
    info!("Initializing RMCP server...");
    let server = mcp_server.serve(stdio()).await?;
    info!("RMCP server started successfully.");

    // Keep the server running
    info!("Server is running, waiting for requests...");
    server.waiting().await?; // Wait for the server task to complete

    info!("MCP server shutdown complete.");
    Ok(())
}

```
5. Host Implementation (`mcp_host`)
A. Update `MCPHost` and `ServerManager`:
   - Change `client_info` type to `rmcp::model::Implementation`.
   - Change tool list types to `Vec<rmcp::model::Tool>`.
B. Update `start_server_with_components`:
   - Replace `ProcessTransport::new` with `rmcp::transport::child_process::TokioChildProcess::new`.
   - Replace manual client creation and initialization with `rmcp::service::serve_client`.
   - The result of `serve_client` is `RunningService`, which contains the `Peer`. Wrap the `Peer` in your `McpClient` struct (in `production` mod).
   - Get capabilities from `RunningService::peer_info()`.
```rust
// Inside mcp_host/src/host/server_manager.rs

// In start_server_with_components:
// ... prepare tokio_command_spawn ...

#[cfg(not(test))]
let (process, client, capabilities) = {
    let process = tokio_command_spawn.spawn()
        .map_err(|e| anyhow!("Failed to spawn process for server '{}': {}", name, e))?;
    let process_id = process.id();
    info!("Process spawned successfully for server '{}', PID: {:?}", name, process_id);

    // Create transport using the *original* command components
    let mut transport_cmd = TokioCommand::new(program); // Use original program path
    transport_cmd.args(args).envs(envs)
                 .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let transport = TokioChildProcess::new(&mut transport_cmd)
         .map_err(|e| anyhow!("Failed to create TokioChildProcess transport for server '{}': {}", name, e))?;
    info!("TokioChildProcess transport created for server '{}'.", name);

    // Serve the client handler (use `()` for default behavior)
    let running_service = serve_client((), transport).await
        .map_err(|e| anyhow!("Failed to serve client and create Peer for server '{}': {}", name, e))?;
    info!("RunningService (including Peer) created for server '{}'.", name);

    let peer = running_service.peer().clone(); // Get the Peer
    let capabilities = running_service.peer_info().map(|info| info.capabilities.clone()); // Get capabilities

    let client = production::McpClient::new(peer); // Wrap the Peer
    info!("McpClient created for server '{}'.", name);

    (process, client, capabilities)
};

// ... rest of the function (insert into map) ...
```
C. Update `list_server_tools` and `call_tool`:
   - These methods in `ServerManager` should now delegate to the `McpClient` wrapper, which in turn delegates to the `Peer`.
```rust
// Inside mcp_host/src/host/server_manager.rs

pub async fn list_server_tools(&self, server_name: &str) -> Result<Vec<RmcpTool>> {
    let servers = self.servers.lock().await;
    let server = servers.get(server_name)
        .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
    server.client.list_tools().await // Delegate to McpClient wrapper
}

pub async fn call_tool(&self, server_name: &str, tool_name: &str, args: Value) -> Result<String> {
    let servers = self.servers.lock().await;
    let server = servers.get(server_name)
        .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
    let result = server.client.call_tool(tool_name, args).await?; // Delegate to McpClient wrapper
    Ok(format_tool_result(&result)) // Format the RmcpCallToolResult
}
```
D. Update `format_tool_result`:
   - Adapt the function to handle `rmcp::model::CallToolResult` and `rmcp::model::RawContent` variants correctly.
```rust
// Inside mcp_host/src/host/server_manager.rs
pub fn format_tool_result(result: &RmcpCallToolResult) -> String {
    let mut output = String::new();
    if result.is_error.unwrap_or(false) {
        output.push_str("TOOL ERROR:\n");
    }
    for content in &result.content { // content is RmcpContent
        match &content.raw { // Access inner RmcpRawContent
            RmcpRawContent::Text(text_content) => {
                // ... (handle text/JSON formatting)
            }
            RmcpRawContent::Image { .. } => {
                output.push_str("[Image content - display not supported]");
            }
            RmcpRawContent::Resource { .. } => {
                output.push_str("[Resource content - display not supported]");
            }
            RmcpRawContent::Audio { .. } => {
                output.push_str("[Audio content - display not supported]");
            }
        }
        output.push('\n');
    }
    output.trim_end().to_string()
}
```
E. Update `ConversationState` and `conversation_logic`:
   - Ensure the local `Role` enum (User, Assistant, System) is used internally.
   - When building requests for the AI client (`rllm`), map the local `Role` to the appropriate builder methods (`.user()`, `.assistant()`, `.system()`).
   - Use `rmcp::model::Tool` when storing/passing tool lists.
F. Update REPL (`repl/mod.rs`, `repl/command.rs`, `repl/helper.rs`):
   - Use `rmcp::model::Tool` in `ReplHelper`.
   - Ensure commands like `tools` and `call` interact correctly with `ServerManager` which now uses `rmcp` types.

6. Step-by-Step Migration Plan

1.  **Dependency Update:** Modify `Cargo.toml`.
2.  **Tool Server (`mcp_tools`):**
    *   Convert individual tool implementations (`bash.rs`, `scraping_bee.rs`, etc.) to use `#[tool]` macros and return `String` or `Result<String, rmcp::Error>`. Remove old `Tool` trait impls.
    *   Create the `McpToolServer` struct in `main.rs`.
    *   Implement delegating tool methods in `McpToolServer` using `#[tool(tool_box)]`.
    *   Implement `ServerHandler` for `McpToolServer` using `#[tool(tool_box)]`.
    *   Replace the old `main` function logic with `mcp_server.serve(stdio()).await?` and `server.waiting().await?`.
3.  **Host (`mcp_host`):**
    *   Replace `shared_protocol_objects` imports with `rmcp` imports (using aliases helps).
    *   Refactor `ServerManager`'s `start_server_with_components` to use `TokioChildProcess` and `serve_client`.
    *   Update the `McpClient` wrapper in `server_manager.rs` (production mod) to wrap `Peer<RoleClient>`.
    *   Update `list_server_tools`, `call_tool`, and `format_tool_result` in `ServerManager`.
    *   Update `MCPHost` struct and methods (`list_all_tools`, `enter_chat_mode`, etc.) to use `rmcp::model::Tool`.
    *   Update `ConversationState` to use `rmcp::model::Tool`. Ensure local `Role` enum mapping works when building AI requests.
    *   Update `ReplHelper` to use `rmcp::model::Tool`.
    *   Verify REPL commands function correctly.
4.  **Testing:** Update tests to reflect the new SDK usage and types. Remove tests relying on `shared_protocol_objects`.
5.  **Documentation:** Update markdown files (`docs/`) to reflect the `rmcp` SDK usage.
6.  **Cleanup:** Remove the `shared_protocol_objects` directory and any remaining unused code.

7. Benefits of Migration

*   **Standardization:** Aligns with the official MCP specification and ecosystem.
*   **Simplified Code:** Less boilerplate for tool definition and server setup due to SDK macros and abstractions.
*   **Type Safety:** Leverages the SDK's well-defined types and schema generation.
*   **Maintainability:** Future protocol updates are handled by the SDK maintainers.
*   **Interoperability:** Easier integration with other MCP-compatible clients and servers.
