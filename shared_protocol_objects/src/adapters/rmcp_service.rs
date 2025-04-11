use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::process::Command;
use crate::{
    JsonRpcResponse, // Keep our response type for now, but might adapt later
    InitializeParams, InitializeResult, ToolInfo, ListToolsParams, ListToolsResult, CallToolParams, CallToolResult,
    Implementation as OurImplementation, // Alias our Implementation type
};
use rmcp::model::{
    self as sdk, // Alias SDK model namespace
    Id as SdkId, // Import SDK Id type
    ClientInfo as SdkClientInfo,
    ServerInfo as SdkServerInfo,
    ClientCapabilities as SdkClientCapabilities,
    ServerCapabilities as SdkServerCapabilities,
    Tool as SdkTool,
    CallToolResult as SdkCallToolResult,
    ListToolsResult as SdkListToolsResult,
    InitializeResult as SdkInitializeResult,
};
use rmcp::transport::{TokioChildProcess, IntoTransport}; // Use SDK transport directly here
use rmcp::{Service as SdkService, RoleClient, serve_client}; // Use SDK Service
use serde_json::Value;
use std::time::Duration; // For timeouts

// Import protocol adapter for potential conversions if needed, though Service might handle it
use super::rmcp_protocol::RmcpProtocolAdapter;
// Import telemetry helpers
use super::telemetry::{increment_request_count, increment_error_count, RequestTimer};


/// High-level service adapter that uses the RMCP SDK's Service abstraction.
/// This adapter aims to provide an interface similar to our existing client/host logic
/// but backed by the RMCP SDK's `Service`.
pub struct RmcpServiceAdapter {
    // Use the SDK's Service directly
    inner_service: Arc<SdkService<RoleClient>>,
    // Store client info used for initialization
    client_info: OurImplementation,
    // Store protocol version used
    protocol_version: String,
    // Store client capabilities used
    client_capabilities: crate::ClientCapabilities,
    // Store request timeout
    request_timeout: Duration,
}

impl RmcpServiceAdapter {
    /// Create a new service adapter from a command.
    pub async fn new(
        mut command: Command,
        client_info: OurImplementation,
        protocol_version: Option<&str>,
        client_capabilities: crate::ClientCapabilities,
        request_timeout: Duration,
    ) -> Result<Self> {
        tracing::debug!("Creating RmcpServiceAdapter with command: {:?}", command);
        // 1. Create the RMCP process transport directly from the SDK
        let transport = TokioChildProcess::new(&mut command)?
            .into_transport()?; // Use SDK's transport creation

        // 2. Create the RMCP client service using the SDK's serve_client
        // Note: serve_client likely handles the Initialize handshake internally.
        // We might need to pass initialization parameters differently if serve_client doesn't take them directly.
        // The `serve_client` function in the SDK might need inspection. Assuming it returns a Service.
        let service = serve_client(transport).await?; // This might perform initialization

        let default_version = "2024-10-07"; // Default or fallback version known to SDK
        let version_str = protocol_version.unwrap_or(default_version).to_string();

        Ok(Self {
            inner_service: Arc::new(service),
            client_info,
            protocol_version: version_str,
            client_capabilities,
            request_timeout,
        })
    }

    /// Initialize the connection (if not handled by `serve_client`).
    /// The SDK's `Service` might handle initialization implicitly upon creation.
    /// If explicit initialization is needed, call the SDK's initialize method.
    pub async fn initialize(&self) -> Result<InitializeResult> {
         let timer = RequestTimer::new("initialize");
         increment_request_count();
         tracing::info!("Explicitly calling initialize on RMCP Service Adapter");

         // Convert our client info and capabilities to SDK format
         let sdk_client_info = RmcpProtocolAdapter::convert_client_info_to_sdk(&self.client_info);
         let sdk_capabilities = RmcpProtocolAdapter::convert_capabilities_to_sdk(&self.client_capabilities)?;

         // Call the SDK's initialize method
         // The SDK's initialize might take different parameters or be parameterless if info is set elsewhere
         // Adjust this call based on the actual rmcp::Service::initialize signature.
         // Assuming it takes SdkClientInfo and SdkClientCapabilities for now.
         let sdk_result: SdkInitializeResult = self.inner_service.initialize(
             self.protocol_version.clone(), // Pass protocol version
             sdk_client_info,
             sdk_capabilities,
             // Add other parameters like processId, rootUri if required by SDK initialize
         ).await.map_err(|e| {
             increment_error_count();
             anyhow!("RMCP SDK initialize failed: {}", e)
         })?;

         // Convert the SDK result back to our format
         let our_result = InitializeResult {
             protocol_version: sdk_result.protocol_version,
             capabilities: RmcpProtocolAdapter::convert_capabilities_from_sdk(&sdk_result.capabilities)?,
             server_info: RmcpProtocolAdapter::convert_implementation_from_sdk(&sdk_result.server_info),
             instructions: sdk_result.instructions,
         };
         timer.finish();
         Ok(our_result)
    }


    /// List tools using the RMCP service.
    pub async fn list_tools(&self, params: Option<ListToolsParams>) -> Result<ListToolsResult> {
        let timer = RequestTimer::new("list_tools");
        increment_request_count();
        tracing::info!("Calling list_tools on RMCP Service Adapter");

        // Prepare SDK ListTools parameters (e.g., cursor)
        let sdk_params = sdk::ListToolsParams {
            cursor: params.and_then(|p| p.cursor),
            // Add other params if SDK requires them
        };

        // Use the RMCP service to list tools
        let sdk_result: SdkListToolsResult = self.inner_service.list_tools(sdk_params)
            .await
            .map_err(|e| {
                increment_error_count();
                anyhow!("RMCP SDK list_tools failed: {}", e)
            })?;

        // Convert the result to our format
        let our_result = ListToolsResult {
            tools: sdk_result.tools.into_iter()
                .map(RmcpProtocolAdapter::convert_tool_info_from_sdk)
                .collect::<Result<Vec<_>>>()?, // Collect results, propagating errors
            next_cursor: sdk_result.cursor,
        };
        timer.finish();
        Ok(our_result)
    }

    /// Call a tool using the RMCP service.
    pub async fn call_tool(&self, params: CallToolParams) -> Result<CallToolResult> {
        let timer = RequestTimer::new("call_tool");
        increment_request_count();
        tracing::info!("Calling call_tool ({}) on RMCP Service Adapter", params.name);

        // Prepare SDK CallTool parameters
        let sdk_params = sdk::CallToolParams {
            name: params.name,
            arguments: params.arguments, // Assuming Value maps directly
            // Add other params if SDK requires them
        };

        // Use the RMCP service to call the tool
        let sdk_result: SdkCallToolResult = self.inner_service.call_tool(sdk_params)
            .await
            .map_err(|e| {
                increment_error_count();
                anyhow!("RMCP SDK call_tool failed: {}", e)
            })?;

        // Convert the result to our format
        let our_result = CallToolResult {
             content: sdk_result.content.into_iter()
                 .map(RmcpProtocolAdapter::convert_tool_response_content_from_sdk)
                 .collect::<Result<Vec<_>>>()?, // Collect results
        };
        timer.finish();
        Ok(our_result)
    }

    /// Send a notification using the RMCP service.
    pub async fn send_notification(&self, method: String, params: Value) -> Result<()> {
        // Use the SDK's generic notification method
        self.inner_service.notification(method, params)
            .await
            .map_err(|e| anyhow!("RMCP SDK notification failed: {}", e))
    }

    /// Subscribe to notifications using the RMCP service.
    /// The SDK's `Service` might handle notifications differently (e.g., callbacks).
    /// Adjust this based on how `rmcp::Service` exposes notifications.
    pub async fn subscribe_to_notifications(&self, handler: Box<dyn Fn(sdk::Notification) + Send + Sync>) -> Result<()> {
        // This depends heavily on the SDK's API for notification handling.
        // Option 1: Direct callback registration (if available)
        // self.inner_service.on_notification(handler).await?;

        // Option 2: If Service exposes a stream of notifications
        // let stream = self.inner_service.notifications_stream().await?;
        // tokio::spawn(async move {
        //     while let Some(notification) = stream.next().await {
        //         handler(notification);
        //     }
        // });

        // Option 3: If it relies on the underlying transport's subscription (less likely with Service abstraction)
        // This might require accessing the transport layer, which contradicts using the Service abstraction.

        // Placeholder: Assume a method `on_notification` exists for demonstration
        tracing::info!("Subscribing to notifications via RMCP Service Adapter");
        self.inner_service.on_notification(handler)
             .await
             .map_err(|e| anyhow!("RMCP SDK on_notification failed: {}", e))?;


        Ok(())
    }


    /// Close the service (sends shutdown and exit).
    pub async fn close(&self) -> Result<()> {
        let timer = RequestTimer::new("close");
        increment_request_count(); // Count close as a request type action
        tracing::info!("Closing RMCP Service Adapter (sending shutdown/exit)");
        // Use the RMCP service's shutdown/cancel mechanism
        // The SDK might have separate `shutdown` and `exit` methods, or a single `close`/`cancel`.
        // Assuming `cancel` handles both shutdown and exit notification for simplicity.
        self.inner_service.cancel() // Or .shutdown().await? followed by .exit().await?
            .await
            .map_err(|e| {
                increment_error_count();
                anyhow!("RMCP SDK cancel/shutdown failed: {}", e)
            })?;
        timer.finish();
        Ok(())
    }

    // Add wrappers for other SDK Service methods as needed...
    // e.g., progress updates, workspace edits, etc.
}
