How the Tool Marker System Works in the RMCP SDK
The RMCP (Rust Model Context Protocol) SDK employs a sophisticated macro-based system to define, register, and execute tools. This system automates much of the boilerplate code needed to handle JSON-RPC tool calls while providing type safety and schema generation.
1. Core Components
The tool marking system consists of several key components:
The #[tool] Attribute Macro
This is the primary mechanism used to mark methods as tools. It can be applied to:

Individual methods within an implementation block
The entire implementation block itself (with the tool_box option)

The tool_box! Macro
This macro creates a static registry of tools for a type, handling:

Tool registration
Schema generation
Dispatching calls to the appropriate handler

2. How Tool Methods Are Defined
Tool methods can be defined in two main ways:
Method 1: Using #[tool] on Individual Methods
rust#[derive(Debug, Clone)]
pub struct MyTool;

impl MyTool {
    #[tool(description = "A description of what this tool does")]
    async fn my_tool_method(
        &self, 
        #[tool(param)] parameter1: String,
        #[tool(param)] parameter2: i32
    ) -> String {
        // Tool implementation
        format!("Result: {} {}", parameter1, parameter2)
    }
}
Method 2: Using an Aggregated Parameter Object
rust#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MyToolParams {
    #[schemars(description = "Description of parameter1")]
    pub parameter1: String,
    pub parameter2: i32,
}

impl MyTool {
    #[tool(description = "A description of what this tool does")]
    async fn my_tool_method(&self, #[tool(aggr)] params: MyToolParams) -> String {
        // Tool implementation
        format!("Result: {} {}", params.parameter1, params.parameter2)
    }
}
3. Parameter Marking
Parameters for tools can be marked in two ways:
#[tool(param)] - Individual Parameters
This marks individual parameters that should be extracted from the tool call JSON. Each parameter becomes a field in a generated request struct.
rust#[tool(description = "Add two numbers")]
fn add(
    &self,
    #[tool(param)] 
    #[schemars(description = "First number")]
    a: i32,
    #[tool(param)]
    #[schemars(description = "Second number")]
    b: i32,
) -> String {
    (a + b).to_string()
}
#[tool(aggr)] - Aggregated Parameter
This marks a single struct that contains all parameters. The struct must implement serde::Deserialize and schemars::JsonSchema.
rust#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AddParams {
    #[schemars(description = "First number")]
    pub a: i32,
    #[schemars(description = "Second number")]
    pub b: i32,
}

#[tool(description = "Add two numbers")]
fn add(&self, #[tool(aggr)] params: AddParams) -> String {
    (params.a + params.b).to_string()
}
4. Behind the Scenes: What the Macros Generate
When you apply the #[tool] attribute to a method, the macro expands to generate several functions:
1. Tool Attribute Function
For a method named foo, a corresponding foo_tool_attr() function is generated that returns the tool's metadata:
rustfn foo_tool_attr() -> rmcp::model::Tool {
    rmcp::model::Tool {
        name: "foo".into(),
        description: Some("Description of foo tool".into()),
        input_schema: cached_schema_for_type::<FooParams>().into(),
        annotations: None
    }
}
2. Tool Call Handler Function
A foo_tool_call function is generated that:

Extracts parameters from the JSON-RPC request
Validates them against the schema
Calls the original method with the extracted parameters
Converts the result to a CallToolResult

rustasync fn foo_tool_call(
    context: rmcp::handler::server::tool::ToolCallContext<'_, Self>
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::Error> {
    use rmcp::handler::server::tool::*;
    
    // Extract the receiver (self)
    let (__rmcp_tool_receiver, context) = 
        <&Self>::from_tool_call_context_part(context)?;
        
    // For aggregated parameters
    let (Parameters(params), context) = 
        <Parameters<FooParams>>::from_tool_call_context_part(context)?;
        
    // Call the original method and convert the result
    Self::foo(__rmcp_tool_receiver, params).await.into_call_tool_result()
}
3. Tool Box Registry
When applying #[tool(tool_box)] to an impl block, a static tool_box() function is generated that creates and populates a registry of all tools:
rustfn tool_box() -> &'static rmcp::handler::server::tool::ToolBox<Self> {
    static TOOL_BOX: std::sync::OnceLock<ToolBox<Self>> = 
        std::sync::OnceLock::new();
        
    TOOL_BOX.get_or_init(|| {
        let mut tool_box = ToolBox::new();
        
        // Add each tool method to the registry
        tool_box.add(ToolBoxItem::new(
            Self::foo_tool_attr(), 
            Self::foo_tool_call
        ));
        
        // Add more tools...
        
        tool_box
    })
}
5. Integration with ServerHandler Trait
The #[tool(tool_box)] can be applied to an implementation of the ServerHandler trait to automatically implement the required methods:
rust#[tool(tool_box)]
impl ServerHandler for MyTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Tool instructions".into()),
            ..Default::default()
        }
    }
}
This expands to:
rustimpl ServerHandler for MyTool {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Tool instructions".into()),
            ..Default::default()
        }
    }
    
    async fn list_tools(
        &self,
        _: Option<rmcp::model::PaginatedRequestParam>,
        _: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::Error> {
        Ok(rmcp::model::ListToolsResult {
            next_cursor: None,
            tools: Self::tool_box().list(),
        })
    }

    async fn call_tool(
        &self,
        call_tool_request_param: rmcp::model::CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::Error> {
        let context = rmcp::handler::server::tool::ToolCallContext::new(
            self, 
            call_tool_request_param, 
            context
        );
        Self::tool_box().call(context).await
    }
}
6. Return Value Conversion
The tool system also handles converting various return types to CallToolResult through the IntoCallToolResult trait:

String → Returns success with text content
Result<T, E> → Returns success or error based on the Result
Custom types implementing IntoContents → Returns success with custom content

7. Complete Example
Here's how it all comes together in a complete example:
rust#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SumRequest {
    #[schemars(description = "First number")]
    pub a: i32,
    #[schemars(description = "Second number")]
    pub b: i32,
}

#[derive(Debug, Clone)]
pub struct Calculator;

#[tool(tool_box)]
impl Calculator {
    #[tool(description = "Calculate the sum of two numbers")]
    async fn sum(&self, #[tool(aggr)] params: SumRequest) -> String {
        (params.a + params.b).to_string()
    }

    #[tool(description = "Calculate the difference of two numbers")]
    fn sub(
        &self,
        #[tool(param)] a: i32,
        #[tool(param)] b: i32,
    ) -> String {
        (a - b).to_string()
    }
}

#[tool(tool_box)]
impl ServerHandler for Calculator {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A simple calculator".into()),
            ..Default::default()
        }
    }
}
8. Key Technical Details

Compile-Time Validation: The macros validate tool definitions at compile time.
Schema Generation: JSON schemas are automatically generated from Rust types using the schemars crate.
Parameter Extraction: The system provides type-safe parameter extraction from JSON-RPC requests.
Type Safety: The entire process is type-checked, ensuring that tool implementations match their declarations.
Caching: Schemas are cached using thread-local storage for performance.
Error Handling: The system provides structured error handling that conforms to the MCP specification.
Zero-Copy Operations: Where possible, data is passed without unnecessary copying or cloning.

The tool marker system in RMCP provides a declarative, type-safe way to define tools that conform to the Model Context Protocol, significantly reducing boilerplate code while ensuring that all tools are properly registered and documented with accurate JSON schemas.RetryClaude does not have the ability to run the code it generates yet.Claude can make mistakes. Please double-check responses.