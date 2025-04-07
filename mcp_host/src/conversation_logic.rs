
// Keep only one set of imports
use crate::ai_client::AIClient; // Removed unused AIRequestBuilder
use crate::conversation_state::ConversationState;
use crate::host::MCPHost;
use crate::tool_parser::ToolParser; // Removed unused ToolCall
use anyhow::{anyhow, Context, Result};
use console::style;
use log::{debug, error, info, warn};
use serde::Deserialize; // Added Deserialize
use serde_json; // Added serde_json
use std::sync::Arc;
use shared_protocol_objects::Role;

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
            max_tool_iterations: 20, // Reduced from 5 to 3
        }
    }
}

// --- Verification System ---

/// Outcome of the conversation resolution, including verification status.
#[derive(Debug, Clone)] // Added Clone
pub struct VerificationOutcome {
    pub final_response: String,
    pub criteria: Option<String>,
    pub verification_passed: Option<bool>,
    pub verification_feedback: Option<String>,
}

/// Structure expected from the Verifier LLM.
#[derive(Deserialize, Debug)]
struct VerificationLLMResponse {
    passes: bool,
    feedback: Option<String>,
}

/// Generates verification criteria based on the user request.
pub async fn generate_verification_criteria(host: &MCPHost, user_request: &str) -> Result<String> { // Added pub
    debug!("Generating verification criteria for request: '{}'", user_request.lines().next().unwrap_or(""));
    let client = host.ai_client().await
        .ok_or_else(|| anyhow!("No AI client active for generating criteria"))?;

    let prompt = format!(
        "Based on the following user request, list concise, verifiable criteria for a successful response. \
        Focus on key actions, information requested, and constraints mentioned. \
        Output ONLY the criteria list, one criterion per line, starting with '- '. Do not include any other text.\n\n\
        User Request:\n```\n{}\n```\n\nCriteria:",
        user_request
    );

    // Use raw_builder as we don't need tool context here
    let criteria = client.raw_builder("") // Pass empty system prompt
        .user(prompt)
        .execute()
        .await
        .context("Failed to call LLM for criteria generation")?;

    info!("Generated verification criteria (length: {})", criteria.len());
    Ok(criteria)
}

/// Verifies the proposed final response against the criteria and conversation history.
async fn verify_response(
    host: &MCPHost,
    state: &ConversationState, // Pass full state for history context
    criteria: &str,
    proposed_response: &str,
) -> Result<(bool, Option<String>)> {
    debug!("Verifying proposed response against criteria.");
    let client = host.ai_client().await
        .ok_or_else(|| anyhow!("No AI client active for verification"))?;

    // Find the index of the last user message
    let last_user_message_index = state.messages.iter().rposition(|m| m.role == Role::User);

    // Extract the original user request (the last one found)
    let original_request = last_user_message_index
        .map(|idx| state.messages[idx].content.as_str())
        .unwrap_or("Original request not found in history.");

    // Extract the sequence of assistant messages since the last user message
    let assistant_sequence = match last_user_message_index {
        Some(idx) => state.messages[idx + 1..] // Get slice of messages after the last user message
            .iter()
            .filter(|m| m.role == Role::Assistant) // Ensure we only include assistant messages
            .map(|msg| {
                // Use existing formatting to show tool calls clearly if they exist in the message content
                // Note: This assumes tool results are also stored as Assistant messages.
                // If tool results had a different role, adjust the filter/formatting.
                crate::conversation_state::format_assistant_response_with_tool_calls(&msg.content)
            })
            .collect::<Vec<String>>()
            .join("\n\n---\n\n"), // Separate messages clearly
        None => proposed_response.to_string(), // Fallback to just the proposed response if no user message found
    };

    // Ensure the final proposed response is included if the sequence is empty (edge case)
    let final_assistant_actions_and_response = if assistant_sequence.is_empty() {
        proposed_response.to_string()
    } else {
        // Check if the very last message content matches the proposed_response. If not, append it.
        // This handles cases where the loop might have exited before the final response was added to history (though unlikely with current logic).
        if state.messages.last().map_or(true, |m| m.content != proposed_response) {
             format!("{}\n\n---\n\n{}", assistant_sequence, crate::conversation_state::format_assistant_response_with_tool_calls(proposed_response))
        } else {
            assistant_sequence
        }
    };


    let prompt = format!(
        "You are a strict evaluator. Verify if the 'Assistant's Actions and Final Response' sequence below meets ALL the 'Success Criteria' based on the 'Original User Request'.\n\n\
        Original User Request:\n```\n{}\n```\n\n\
        Success Criteria:\n```\n{}\n```\n\n\
        Assistant's Actions and Final Response:\n```\n{}\n```\n\n\
        Instructions:\n\
        1. Carefully review the *entire sequence* of the assistant's actions (including tool calls/results shown) and its final response.\n\
        2. Compare this sequence against each point in the 'Success Criteria'.\n\
        3. Determine if the *outcome* of the assistant's actions and the final response *fully and accurately* satisfy *all* criteria.\n\
        4. Output ONLY a valid JSON object with the following structure:\n\
           `{{\"passes\": boolean, \"feedback\": \"string (provide concise feedback ONLY if passes is false, explaining which criteria failed and why, referencing the assistant's actions if relevant)\"}}`\n\
        5. Do NOT include any other text, explanations, or markdown formatting.",
        original_request, criteria, final_assistant_actions_and_response // Use the full sequence here
    );

    // Use raw_builder as we don't need tool context here
    let verification_result_str = client.raw_builder("") // Pass empty system prompt
        .user(prompt)
        .execute()
        .await
        .context("Failed to call LLM for verification")?;

    

    // Parse the JSON response
    let json_start = verification_result_str.find('{');
    let json_end = verification_result_str.rfind('}');

    if let (Some(start), Some(end)) = (json_start, json_end) {
         if start < end {
             let json_str = &verification_result_str[start..=end];
             debug!("Extracted verification JSON string: {}", json_str);
             match serde_json::from_str::<VerificationLLMResponse>(json_str) {
                 Ok(parsed) => {
                     info!("Verification result: passes={}", parsed.passes);
                        if let Some(ref feedback) = parsed.feedback {
                            warn!("Verification feedback: {}", feedback);
                        }
                     return Ok((parsed.passes, parsed.feedback));
                 },
                 Err(e) => {
                     error!("Failed to parse verification JSON response: {}", e);
                     return Err(anyhow!("Failed to parse verification JSON response: {}. Raw: '{}'", e, verification_result_str));
                 }
             }
         }
    }

    error!("Could not find valid JSON object in verification response.");
    Err(anyhow!("Could not find valid JSON object in verification response: '{}'", verification_result_str))
}


// --- End Verification System ---


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
/// * `Ok(VerificationOutcome)` - Contains the final response and verification details.
/// * `Err(anyhow::Error)` - If a non-recoverable error occurs.
pub async fn resolve_assistant_response(
    host: &MCPHost,
    server_name: &str,
    state: &mut ConversationState,
    initial_assistant_response: &str,
    client: Arc<dyn AIClient>,
    config: &ConversationConfig,
    criteria: &str, // Added criteria parameter
) -> Result<VerificationOutcome> { // Changed return type
    debug!(
        "resolve_assistant_response called for server '{}'. Initial response length: {}. Criteria provided: {}",
        server_name,
        initial_assistant_response.len(),
        !criteria.is_empty() // Removed duplicate initial_assistant_response.len()
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
                    "Reached max tool iterations ({}) for server '{}'. Returning last (unverified) response.",
                    config.max_tool_iterations, server_name
                );
                // Return an unverified outcome when max iterations are hit
                return Ok(VerificationOutcome {
                    final_response: current_response,
                    criteria: Some(criteria.to_string()), // Include criteria if available
                    verification_passed: None, // Indicate verification was skipped/aborted
                    verification_feedback: Some("Max tool iterations reached".to_string()),
                });
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
                // --- No Tool Calls: Attempt Verification ---
                debug!("No tool calls found in iteration {}. Performing verification.", iterations);

                if criteria.is_empty() {
                    info!("No criteria provided, skipping verification.");
                    return Ok(VerificationOutcome {
                        final_response: current_response,
                        criteria: None,
                        verification_passed: None,
                        verification_feedback: None,
                    });
                }

                match verify_response(host, state, criteria, &current_response).await {
                    Ok((passes, feedback_opt)) => {
                        if passes {
                            info!("Verification passed for server '{}'. Returning final response.", server_name);
                            return Ok(VerificationOutcome {
                                final_response: current_response,
                                criteria: Some(criteria.to_string()),
                                verification_passed: Some(true),
                                verification_feedback: feedback_opt, // Include feedback even if passed
                            });
                        } else {
                            // Verification failed, inject feedback and retry
                            warn!("Verification failed for server '{}'. Injecting feedback and retrying.", server_name);
                            if let Some(feedback) = feedback_opt.clone() { // Clone feedback for outcome
                                let feedback_msg = format!("Verification Feedback: {}", feedback);
                                state.add_system_message(&feedback_msg);
                                state.add_system_message(
                                    "Verification failed. Please analyze the feedback above and revise your previous response to meet the original request's criteria. Provide a new, complete response."
                                );

                                // Call AI Again for Revision
                                debug!("Calling AI again after verification failure.");
                                let mut builder = client.raw_builder(&state.system_prompt); // Pass correct system prompt
                                for msg in &state.messages {
                                    match msg.role {
                                        Role::System => {} // Skip system messages here, handled by injection
                                        Role::User => builder = builder.user(msg.content.clone()),
                                        Role::Assistant => builder = builder.assistant(msg.content.clone()),
                                    }
                                }

                                if config.interactive_output {
                                    println!("{}", style("\nVerification failed. Revising response...").yellow().italic());
                                }

                                match builder.execute().await {
                                    Ok(revised_response) => {
                                        info!("Received revised AI response after verification failure (length: {}).", revised_response.len());
                                        state.add_assistant_message(&revised_response);
                                        current_response = revised_response; // Update current response
                                        // Loop continues to re-evaluate the revised response
                                        continue; // Go to next loop iteration
                                    }
                                    Err(e) => {
                                        error!("Error getting revised AI response after verification failure: {:?}", e);
                                        warn!("Returning unverified response due to error during revision.");
                                        return Ok(VerificationOutcome { // Return unverified outcome
                                            final_response: current_response,
                                            criteria: Some(criteria.to_string()),
                                            verification_passed: Some(false), // Mark as failed
                                            verification_feedback: feedback_opt,
                                        });
                                    }
                                }
                            } else {
                                // Verification failed but no feedback provided
                                warn!("Verification failed without feedback for server '{}'. Returning unverified response.", server_name);
                                return Ok(VerificationOutcome { // Return unverified outcome
                                    final_response: current_response,
                                    criteria: Some(criteria.to_string()),
                                    verification_passed: Some(false), // Mark as failed
                                    verification_feedback: None,
                                });
                            }
                        }
                    }
                    Err(e) => {
                        // Verification call itself failed
                        error!("Error during verification call for server '{}': {}", server_name, e);
                        warn!("Returning unverified response due to verification error.");
                        return Ok(VerificationOutcome { // Return unverified outcome
                            final_response: current_response,
                            criteria: Some(criteria.to_string()),
                            verification_passed: None, // Indicate verification error
                            verification_feedback: Some(format!("Verification Error: {}", e)),
                        });
                    }
                }
                // --- End Verification Logic ---
            }

            // --- Tool Calls Found --- (Existing logic remains largely the same)
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
            // Pass system prompt when creating builder
            let mut builder = client.raw_builder(&state.system_prompt); // Pass correct system prompt

            // Add all messages *including the new tool results*
            for msg in &state.messages {
                 match msg.role {
                     Role::System => {} // Skip system messages here, handled by injection
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
