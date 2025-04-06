
use crate::ai_client::AIClient;
use crate::conversation_state::ConversationState;
use crate::host::MCPHost;
use crate::tool_parser::ToolParser;
use anyhow::{anyhow, Result}; // Removed unused Context
use console::style;
use log::{debug, error, info, warn};
use std::sync::Arc;
use shared_protocol_objects::Role; // Add Role import

/// Configuration for how the conversation logic should behave.
#[derive(Debug, Clone)]
pub struct ConversationConfig {
    /// If true, print intermediate steps (tool calls, results) to stdout.
    pub interactive_output: bool,
    /// Maximum number of tool execution iterations before aborting.
    pub max_tool_iterations: u8,
}

impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            interactive_output: false,
            max_tool_iterations: 3, // Reduced from 5 to 3
        }
    }
}

/// Processes an assistant's response, handling tool calls recursively until a final text response is reached.
///
/// This function takes the *initial* assistant response for a turn and drives the
/// subsequent tool calls and AI interactions based on that response.
///
/// # Arguments
/// * `host` - Reference to the MCPHost for accessing AI client and tool execution.
/// * `server_name` - The name of the server context for tool execution.
/// * `state` - Mutable reference to the conversation state, which will be updated.
/// * `initial_assistant_response` - The first response from the assistant in this turn.
/// * `client` - The AI client instance to use for follow-up calls.
/// * `config` - Configuration controlling interactivity and limits.
///
/// # Returns
/// * `Ok(String)` - The final text response from the assistant after all tool calls are resolved.
/// * `Err(anyhow::Error)` - If an error occurs during AI calls or tool execution.
pub async fn resolve_assistant_response(
    host: &MCPHost,
    server_name: &str,
    state: &mut ConversationState,
    initial_assistant_response: &str,
    client: Arc<dyn AIClient>,
    config: &ConversationConfig,
) -> Result<String> {
    debug!(
        "resolve_assistant_response called for server '{}'. Initial response length: {}",
        server_name,
        initial_assistant_response.len()
    );
    // Add the initial response to the state *before* processing it
    // Note: The caller (REPL or eval runner) should have already added the user message
    // and called the AI once to get this initial_assistant_response.
    // We add it here to ensure it's part of the history *before* any potential tool calls stemming from it.
    state.add_assistant_message(initial_assistant_response);

    // Use Box::pin for recursive async logic
    Box::pin(async move {
        let mut current_response = initial_assistant_response.to_string();
        let mut iterations = 0;

        loop {
            if iterations >= config.max_tool_iterations {
                warn!(
                    "Reached max tool iterations ({}) for server '{}'. Returning last response.",
                    config.max_tool_iterations, server_name
                );
                return Ok(current_response);
            }
            iterations += 1;
            debug!("Processing response iteration {} for server '{}'", iterations, server_name);


            // --- Print current response if interactive ---
            // This prints the *current* response being processed in this iteration
            if config.interactive_output {
                 // Use the dedicated formatting function that highlights tool calls
                 let formatted_display = crate::conversation_state::format_assistant_response_with_tool_calls(&current_response);
                 println!("\n{}", formatted_display); // Print formatted response
            }
            // --- End Interactive Print ---


            // --- Parse for Tool Calls ---
            let tool_calls = ToolParser::parse_tool_calls(&current_response);

            if tool_calls.is_empty() {
                // --- No Tool Calls: Final Response ---
                debug!("No tool calls found in iteration {}. Returning final response.", iterations);
                return Ok(current_response);
            }

            // --- Tool Calls Found ---
            info!(
                "Found {} tool calls in iteration {}. Executing...",
                tool_calls.len(),
                iterations
            );
            let mut all_tool_results = Vec::new();

            for tool_call in tool_calls {
                // --- Print Tool Intention if Interactive ---
                if config.interactive_output {
                    println!(
                        "\n{}",
                        style(format!("Assistant wants to call tool: {}", style(&tool_call.name).yellow())).italic()
                    );
                    println!(
                        "{}",
                        style(format!(
                            "Arguments:\n{}",
                            serde_json::to_string_pretty(&tool_call.arguments).unwrap_or_else(|_| "Invalid JSON".to_string())
                        )).dim()
                    );
                }
                // --- End Interactive Print ---

                // --- Execute Tool ---
                let tool_result_str = execute_single_tool_internal(
                    host,
                    server_name,
                    &tool_call.name,
                    tool_call.arguments.clone(), // Clone args for execution
                    config, // Pass config for potential interactive elements in execution
                )
                .await?; // Propagate errors from tool execution

                // --- Add Tool Result to State ---
                // The result message is added regardless of interactive mode, as it's part of the conversation history
                let result_msg = format!("Tool '{}' returned: {}", tool_call.name, tool_result_str.trim());
                debug!("Adding tool result message to state: {}", result_msg.lines().next().unwrap_or(""));
                // Add result to state. Note: We add it as an "assistant" message representing the tool's output in the flow.
                // Alternatively, could introduce a new Role::Tool, but Assistant works for now.
                state.add_assistant_message(&result_msg);
                all_tool_results.push(result_msg); // Keep track for potential summary prompt
            }

            // --- Get Next AI Response After Tools ---
            debug!("All tools executed for iteration {}. Getting next AI response.", iterations);
            let mut builder = client.raw_builder();

            // Add all messages *including the new tool results*
            for msg in &state.messages {
                 match msg.role {
                     Role::System => builder = builder.system(msg.content.clone()),
                     Role::User => builder = builder.user(msg.content.clone()),
                     Role::Assistant => builder = builder.assistant(msg.content.clone()),
                 }
            }

            // Add a directive prompt after tool results
            builder = builder.system(
                "Analyze the tool results provided immediately above. Based on those results and the user's original request, decide the next step:\n\
                1. Call another tool if necessary (using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format).\n\
                2. Provide a final response to the user.".to_string()
            );

            // Execute AI call
            if config.interactive_output {
                 println!("{}", style("\nThinking after tool execution...").dim());
            }
            // Capture the specific error from the AI client
            current_response = match builder.execute().await {
                Ok(next_resp) => {
                    info!("Received next AI response after tool execution (length: {}).", next_resp.len());
                    // Add this *new* assistant response to the state for the *next* loop iteration or final return
                    state.add_assistant_message(&next_resp);
                    next_resp // This becomes the response to process in the next loop
                }
                Err(e) => {
                    // Log the detailed error before returning the context error
                    error!("Detailed error getting next AI response after tools: {:?}", e);
                    // Decide how to handle this. Return error? Return last successful response?
                    // Let's propagate the error.
                    return Err(anyhow!("Failed to get AI response after tool execution: {}", e));
                }
            };
            // Loop continues with the new current_response
        } // End loop
    })
    .await
}

/// Internal helper to execute a single tool call.
async fn execute_single_tool_internal(
    host: &MCPHost,
    server_name: &str,
    tool_name: &str,
    args: serde_json::Value,
    config: &ConversationConfig,
) -> Result<String> {
    debug!("Executing tool '{}' on server '{}'", tool_name, server_name);

    // Prepare progress message (only used if interactive)
    let progress_msg = format!(
        "Calling tool '{}' on server '{}'...",
        style(tool_name).yellow(),
        style(server_name).green()
    );

    // Execute with or without progress indicator based on config
    let result_string = if config.interactive_output {
        crate::repl::with_progress(
            progress_msg, // Already styled
            host.call_tool(server_name, tool_name, args),
        )
        .await
    } else {
        // Execute directly without progress spinner
        host.call_tool(server_name, tool_name, args).await
    };

    // Process result (handle potential errors from call_tool)
    match result_string {
        Ok(output) => {
            // Truncate the raw output before formatting/printing
            let truncated_output = crate::repl::truncate_lines(&output, 150); // Use existing truncate

            // Print formatted result if interactive
            if config.interactive_output {
                // Use the formatting function from conversation_state
                println!(
                    "\n{}",
                    crate::conversation_state::format_tool_response(tool_name, &truncated_output)
                );
            }
            debug!("Tool '{}' executed successfully.", tool_name);
            Ok(truncated_output) // Return the truncated output
        }
        Err(error) => {
            let error_msg = format!("Error executing tool '{}': {}", tool_name, error);
            error!("{}", error_msg); // Log the error

            // Format error for printing if interactive
            if config.interactive_output {
                let formatted_error = format!(
                    "{} executing tool '{}': {}",
                    style("Error").red(),
                    style(tool_name).yellow(),
                    error // Use the original error for detail
                );
                println!("\n{}", formatted_error);
            }
            // Return the error message itself as the "result" string to be added to the conversation
            // This allows the AI to potentially react to the tool failure.
            Ok(error_msg)
        }
    }
}
