How the Tool Marker System Works in the `rmcp` SDK
The `rmcp` (Rust Model Context Protocol) SDK employs a sophisticated macro-based system (`#[tool]`, `#[tool(tool_box)]`) to define, register, and execute tools. This system automates much of the boilerplate code needed to handle JSON-RPC tool calls while providing type safety and schema generation.
1. Core Components
The tool marking system consists of several key components:
The `#[tool]` Attribute Macro
This is the primary mechanism used to mark methods as tools. It can be applied to:

*   Individual methods within an implementation block.
*   The entire implementation block itself (with the `tool_box` option, typically used on the `ServerHandler` implementation).

The `tool_box!` Macro (Internal)
This macro (used internally by `#[tool(tool_box)]`) creates a static registry of tools for a type, handling:

*   Tool registration (collecting metadata from `#[tool]` attributes).
*   Schema generation (using `schemars`).
*   Dispatching `call_tool` requests to the appropriate handler function.

2. How Tool Methods Are Defined
Tool methods are typically defined within an `impl` block for your server struct.
```rust
use rmcp::{tool, ServerHandler, model::ServerInfo};
use serde::Deserialize;
use schemars::JsonSchema;

#[derive(Debug, Clone)]
pub struct MyToolServer;

// Define parameter structs using serde and schemars
#[derive(Deserialize, JsonSchema)]
pub struct AddParams {
    #[schemars(description = "First number")]
    a: i32,
    #[schemars(description = "Second number")]
    b: i32,
}

#[derive(Deserialize, JsonSchema)]
pub struct GreetParams {
    #[schemars(description = "Name to greet")]
    name: String,
}

// Implement the tool methods within the server struct's impl block
#[tool(tool_box)] // Apply tool_box here to register methods below
impl MyToolServer {
    #[tool(description = "Adds two numbers")]
    async fn add(&self, #[tool(aggr)] params: AddParams) -> String {
        (params.a + params.b).to_string()
    }

    #[tool(description = "Greets someone")]
    async fn greet(&self, #[tool(aggr)] params: GreetParams) -> String {
        format!("Hello, {}!", params.name)
    }

    // Example with individual parameters (less common for complex tools)
    #[tool(description = "Subtracts two numbers")]
    fn subtract(
        &self,
        #[tool(param)] #[schemars(description = "Number to subtract from")] x: i32,
        #[tool(param)] #[schemars(description = "Number to subtract")] y: i32,
    ) -> String {
        (x - y).to_string()
    }
}

// Implement ServerHandler for the server struct
#[tool(tool_box)] // Apply tool_box here to auto-implement list_tools/call_tool
impl ServerHandler for MyToolServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            name: Some("My Tool Server".into()),
            // ... other info
            ..Default::default()
        }
    }
}
```
3. Parameter Marking
Parameters for tools are typically defined using a single struct marked with `#[tool(aggr)]`.
`#[tool(aggr)]` - Aggregated Parameter Struct
*   Marks a single struct argument that contains all parameters.
*   The struct **must** implement `serde::Deserialize` and `schemars::JsonSchema`.
*   The SDK automatically deserializes the JSON `arguments` object into this struct.
*   Use `#[schemars(description = "...")]` on struct fields for schema documentation.
`#[tool(param)]` - Individual Parameters (Less Common)
*   Marks individual function parameters.
*   The SDK generates an internal struct to hold these parameters.
*   Use `#[schemars(description = "...")]` directly on the parameters.

4. Behind the Scenes: What the Macros Generate
When you apply `#[tool]` and `#[tool(tool_box)]`, the macros generate code similar to this:
1.  **Tool Metadata:** For each `#[tool]` method, metadata (`rmcp::model::Tool`) is generated, including name, description, and the JSON schema derived from the parameter struct (using `schemars`).
2.  **Call Handler:** A wrapper function is generated for each tool method that handles:
    *   Deserializing and validating the incoming JSON arguments against the schema.
    *   Calling your original tool method (`add`, `greet`, etc.) with the deserialized parameters.
    *   Converting the return value of your method into an `rmcp::model::CallToolResult`.
3.  **Tool Registry (`ToolBox`):** The `#[tool(tool_box)]` on the `impl` block creates a static `ToolBox` instance. This registry stores the metadata and call handlers for all `#[tool]` methods within that block.
4.  **`ServerHandler` Implementation:** Applying `#[tool(tool_box)]` to the `impl ServerHandler for ...` block automatically generates the `list_tools` and `call_tool` methods:
    *   `list_tools`: Returns the list of `rmcp::model::Tool` metadata collected in the `ToolBox`.
    *   `call_tool`: Looks up the requested tool name in the `ToolBox` and dispatches the call to the corresponding generated handler function.

5. Return Value Conversion
The tool system automatically converts the return value of your tool methods into the required `rmcp::model::CallToolResult` using the `IntoCallToolResult` trait. Common return types and their conversions:

*   `String` -> Success result with `RawContent::Text`.
*   `Result<String, E>` (where `E: Into<rmcp::Error>`) -> Success with text or an error result.
*   `rmcp::model::Content` -> Success result with the provided content.
*   `Result<rmcp::model::Content, E>` -> Success with content or an error result.
*   `serde_json::Value` -> Success result with `RawContent::Text` containing the serialized JSON.
*   Custom types implementing `rmcp::model::IntoContents` -> Success result wrapping the custom content.

6. Key Technical Details

*   **Compile-Time Generation:** Macros generate code during compilation.
*   **Schema Generation:** `schemars` is used to create JSON schemas from Rust types.
*   **Type Safety:** Parameter extraction and return value conversion are type-checked.
*   **Error Handling:** The system provides structured error handling conforming to MCP (`rmcp::Error`).

The tool marker system in `rmcp` provides a declarative, type-safe way to define tools that conform to the Model Context Protocol, significantly reducing boilerplate code while ensuring proper registration and schema generation.
