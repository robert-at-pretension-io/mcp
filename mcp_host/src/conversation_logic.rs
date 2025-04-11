
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
use tokio::sync::mpsc; // Added for logger channel

/// Configuration for how the conversation logic should behave.
#[derive(Clone)] // Removed Debug derive as Sender doesn't implement it
pub struct ConversationConfig {
    /// If true, print intermediate steps (tool calls, results) to stdout.
    pub interactive_output: bool,
    /// Maximum number of tool execution iterations before aborting.
    pub max_tool_iterations: u8,
    /// Optional sender for detailed logging during execution.
    pub log_sender: Option<mpsc::UnboundedSender<String>>,
}

// Manual Debug implementation
impl std::fmt::Debug for ConversationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversationConfig")
            .field("interactive_output", &self.interactive_output)
            .field("max_tool_iterations", &self.max_tool_iterations)
            .field("log_sender", &self.log_sender.is_some()) // Only show if sender exists
            .finish()
    }
}


impl Default for ConversationConfig {
    fn default() -> Self {
        Self {
            interactive_output: false,
            max_tool_iterations: 20,
            log_sender: None, // Default to no logging
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

    // Extract the sequence of messages (user and assistant) since the *original* user request
    let relevant_history_sequence = match last_user_message_index {
        Some(idx) => state.messages[idx..] // Get slice starting from the last user message
            .iter()
            // .filter(|m| m.role == Role::Assistant) // Keep user feedback messages too
            .map(|msg| {
                // Format based on role
                match msg.role {
                    Role::User => crate::conversation_state::format_chat_message(&msg.role, &msg.content),
                    Role::Assistant => crate::conversation_state::format_assistant_response_with_tool_calls(&msg.content),
                    Role::System => String::new(), // Skip system messages in this sequence
                } // The match expression is now the return value of the closure
            })
            .collect::<Vec<String>>()
            .join("\n\n---\n\n"), // Separate messages clearly
        None => proposed_response.to_string(), // Fallback to just the proposed response if no user message found
    };

    // Ensure the final proposed response is included if it wasn't the last message in the sequence
    let final_actions_and_response_for_verifier = if state.messages.last().map_or(true, |m| m.content != proposed_response) {
         format!("{}\n\n---\n\n{}", relevant_history_sequence, crate::conversation_state::format_assistant_response_with_tool_calls(proposed_response))
    } else {
        relevant_history_sequence
    };


    let prompt = format!(
        "You are a strict evaluator. Verify if the 'Relevant Conversation Sequence' below meets ALL the 'Success Criteria' based on the 'Original User Request'.\n\n\
        Original User Request:\n```\n{}\n```\n\n\
        Success Criteria:\n```\n{}\n```\n\n\
        Relevant Conversation Sequence (User messages, Assistant actions/responses, Tool results):\n```\n{}\n```\n\n\
        Instructions:\n\
        1. Carefully review the *entire sequence* including user feedback, assistant actions (tool calls/results shown), and the final response.\n\
        2. Compare this sequence against each point in the 'Success Criteria'.\n\
        3. Determine if the *outcome* of the assistant's actions and the final response *fully and accurately* satisfy *all* criteria.\n\
        4. Output ONLY a valid JSON object with the following structure:\n\
           `{{\"passes\": boolean, \"feedback\": \"string (provide concise feedback ONLY if passes is false, explaining which criteria failed and why, referencing the assistant's actions/responses if relevant)\"}}`\n\
        5. Do NOT include any other text, explanations, or markdown formatting.",
        original_request, criteria, final_actions_and_response_for_verifier // Use the full sequence here
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
    config: &ConversationConfig, // Now includes optional log_sender
    criteria: &str,
) -> Result<VerificationOutcome> {
    // --- Logging Setup ---
    let log = |msg: String| {
        if let Some(sender) = &config.log_sender {
            if let Err(e) = sender.send(msg) {
                error!("Failed to send message to conversation logger: {}", e);
            }
        }
    };
    // --- End Logging Setup ---

    log(format!("--- Resolving Assistant Response for Server: {} ---", server_name));
    log(format!("Max Tool Iterations: {}", config.max_tool_iterations));
    log(format!("Criteria Provided: {}", !criteria.is_empty()));
    if !criteria.is_empty() {
        log(format!("Criteria:\n```\n{}\n```", criteria));
    }

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
    log(format!("\n{}", crate::conversation_state::format_assistant_response_with_tool_calls(initial_assistant_response))); // Log initial response

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
                let outcome = VerificationOutcome {
                    final_response: current_response,
                    criteria: Some(criteria.to_string()),
                    verification_passed: None,
                    verification_feedback: Some("Max tool iterations reached".to_string()),
                };
                log(format!("\n--- Max Iterations Reached ({}) ---", config.max_tool_iterations));
                log(format!("Returning last response (unverified):\n```\n{}\n```", outcome.final_response));
                return Ok(outcome);
            }
            iterations += 1;
            log(format!("\n--- Iteration {} ---", iterations));
            debug!("Processing response iteration {} for server '{}'", iterations, server_name);

            // --- Print current response if interactive ---
            if config.interactive_output {
                let formatted_display = crate::conversation_state::format_assistant_response_with_tool_calls(&current_response);
                println!("\n{}", formatted_display);
            }

            // --- Parse for Tool Calls ---
            // Now returns (valid_calls, first_invalid_attempt_content)
            let (tool_calls, invalid_attempt_content) = ToolParser::parse_tool_calls(&current_response);

            if !tool_calls.is_empty() {
                // --- Valid Tool Calls Found: Execute them ---
                log(format!("\n>>> Found {} VALID tool calls. Executing...", tool_calls.len()));
                info!(
                    "Found {} tool calls in iteration {}. Executing...",
                    tool_calls.len(),
                    iterations
                );

                for tool_call in tool_calls {
                    // Log Tool Intention
                    log(format!(
                        "\n>>> Assistant wants to call tool: {}",
                        style(&tool_call.name).yellow()
                    ));
                    log(format!(
                        "Arguments:\n{}",
                        crate::conversation_state::format_json_output(
                            &serde_json::to_string_pretty(&tool_call.arguments).unwrap_or_else(|_| "Invalid JSON".to_string())
                        )
                    ));

                    // Execute Tool
                    let tool_result_str = execute_single_tool_internal(
                        host,
                        server_name,
                        &tool_call.name,
                        tool_call.arguments.clone(),
                        config,
                    )
                    .await?;

                    // Log and Add Tool Result to State
                    log(crate::conversation_state::format_tool_response(&tool_call.name, &tool_result_str));
                    let result_msg_for_state = format!("Tool '{}' returned: {}", tool_call.name, tool_result_str.trim());
                    debug!("Adding tool result message to state: {}", result_msg_for_state.lines().next().unwrap_or(""));
                    state.add_assistant_message(&result_msg_for_state);
                }

                // --- Get Next AI Response After Tools ---
                log("\n>>> Calling AI again after tool execution...".to_string());
                debug!("All tools executed for iteration {}. Getting next AI response.", iterations);
                let mut builder = client.raw_builder(&state.system_prompt);

                // Add messages from state to the builder
                for msg in &state.messages {
                    match msg.role {
                        Role::System => {} // System prompt is handled by raw_builder
                        Role::User => builder = builder.user(msg.content.clone()),
                        Role::Assistant => builder = builder.assistant(msg.content.clone()),
                    }
                }

                // Add a more directive prompt after tool results
                // This prompt is added as a *system* message in this specific call context,
                // instructing the AI on how to proceed *now* that it has tool results.
                // Note: rllm might treat system messages differently depending on the backend.
                // If issues persist, consider adding this as a user message instead.
                builder = builder.system(
                    "You have received results from the tool(s) you called previously (shown immediately above).\n\
                    Analyze these results carefully.\n\
                    Based *only* on these results and the original user request:\n\
                    1. If the results provide the necessary information to fully answer the user's original request, formulate and provide the final answer now. Do NOT call any more tools unless absolutely necessary for clarification based *specifically* on the results received.\n\
                    2. If the results are insufficient or indicate an error, decide if another *different* tool call is needed to achieve the original goal. If so, call the tool using the <<<TOOL_CALL>>>...<<<END_TOOL_CALL>>> format.\n\
                    3. If you cannot proceed further, explain why.".to_string()
                );


                if config.interactive_output {
                    println!("{}", style("\nThinking after tool execution...").dim());
                }

                current_response = match builder.execute().await {
                    Ok(next_resp) => {
                        info!("Received next AI response after tool execution (length: {}).", next_resp.len());
                        log(format!("\n{}", crate::conversation_state::format_assistant_response_with_tool_calls(&next_resp)));
                        state.add_assistant_message(&next_resp);
                        next_resp
                    }
                    Err(_e) => { // Prefix unused variable with underscore
                        error!("Detailed error getting next AI response after tools: {:?}", _e);
                        let error_msg = format!("Failed to get AI response after tool execution: {}", _e);
                        log(format!("\n--- Error Getting Next AI Response: {} ---", error_msg)); // Use error_msg here
                        return Err(anyhow!(error_msg));
                    }
                };
                // Continue to the next loop iteration to process the new current_response
                continue; // Loop back to process the new response from the AI

            } else if let Some(invalid_content) = invalid_attempt_content {
                // --- Invalid Tool Attempt Found ---
                warn!("Detected invalid tool call attempt in iteration {}. Content: {}", iterations, invalid_content);
                log("\n>>> Invalid Tool Call Attempt Detected. Providing Feedback...".to_string());

                // Inject feedback message
                let feedback_message = format!(
                    "Correction Request:\n\
                    You attempted to call a tool, but the format was incorrect. \
                    Remember to use the exact format including delimiters and a valid JSON object with 'name' (string) and 'arguments' (object) fields.\n\n\
                    Your invalid attempt contained:\n```\n{}\n```\n\nPlease correct the format and try the tool call again, or provide a text response if you no longer need the tool.",
                    invalid_content.trim()
                );
                log(format!("Injecting User Message (Invalid Tool Format Feedback):\n```\n{}\n```", feedback_message));
                state.add_user_message(&feedback_message);

                // Call LLM again for correction
                log("\n>>> Calling AI again for tool format correction...".to_string());
                debug!("Calling AI again after invalid tool format detection.");
                let mut builder = client.raw_builder(&state.system_prompt);
                for msg in &state.messages {
                    match msg.role {
                        Role::System => {}
                        Role::User => builder = builder.user(msg.content.clone()),
                        Role::Assistant => builder = builder.assistant(msg.content.clone()),
                    }
                }

                if config.interactive_output {
                     println!("{}", style("\nInvalid tool format detected. Asking AI to correct...").yellow().italic());
                }

                match builder.execute().await {
                    Ok(revised_response) => {
                        info!("Received revised AI response after invalid tool format (length: {}).", revised_response.len());
                        log(format!("\n{}", crate::conversation_state::format_assistant_response_with_tool_calls(&revised_response)));
                        state.add_assistant_message(&revised_response);
                        current_response = revised_response;
                        // Loop continues to re-evaluate the revised response
                        continue; // Go to next loop iteration
                    }
                    Err(e) => {
                        error!("Error getting revised AI response after invalid tool format: {:?}", e);
                        let error_msg = format!("Failed to get AI response after invalid tool format feedback: {}", e);
                        log(format!("\n--- Error Getting Correction: {} ---", error_msg));
                        // Decide how to handle this - maybe return the *previous* response as unverified?
                        // For now, let's return an error state.
                        return Err(anyhow!(error_msg));
                    }
                }

            } else {
                 // --- No Valid Tool Calls AND No Invalid Attempts Found: Attempt Verification ---
                 log("\n>>> Assistant response has no tool calls or invalid attempts. Proceeding to verification.".to_string());
                 debug!("No tool calls or invalid attempts found in iteration {}. Performing verification.", iterations);

                if criteria.is_empty() {
                    info!("No criteria provided, skipping verification.");
                    log("\n--- Verification Skipped (No Criteria) ---".to_string());
                    let outcome = VerificationOutcome {
                        final_response: current_response,
                        criteria: None,
                        verification_passed: None,
                        verification_feedback: None,
                    };
                    log(format!("Final Response:\n```\n{}\n```", outcome.final_response));
                    return Ok(outcome); // Verification skipped, return current response
                }

                log("\n--- Attempting Verification ---".to_string());
                match verify_response(host, state, criteria, &current_response).await {
                    Ok((passes, feedback_opt)) => {
                        log(format!("Verification Result: {}", if passes { "Passed" } else { "Failed" }));
                        if let Some(ref feedback) = feedback_opt {
                            log(format!("Verification Feedback:\n```\n{}\n```", feedback));
                        }

                        if passes {
                            info!("Verification passed for server '{}'. Returning final response.", server_name);
                            let outcome = VerificationOutcome {
                                final_response: current_response,
                                criteria: Some(criteria.to_string()),
                                verification_passed: Some(true),
                                verification_feedback: feedback_opt,
                            };
                            log("\n--- Verification Passed ---".to_string());
                            log(format!("Final Response:\n```\n{}\n```", outcome.final_response));
                            return Ok(outcome); // Verification passed, return current response
                        } else {
                            // Verification failed, inject feedback and retry
                            warn!("Verification failed for server '{}'. Injecting feedback and retrying.", server_name);
                            log("\n--- Verification Failed: Injecting Feedback ---".to_string());
                            if let Some(feedback) = feedback_opt.clone() {
                                let user_feedback_prompt = format!(
                                    "Correction Request:\n\
                                    Your previous response failed verification.\n\
                                    Feedback: {}\n\n\
                                    Please analyze this feedback carefully and revise your plan and response to fully address the original request and meet all success criteria. \
                                    You may need to use tools differently or provide more detailed information.",
                                    feedback
                                );
                                log(format!("Injecting User Message:\n```\n{}\n```", user_feedback_prompt));
                                state.add_user_message(&user_feedback_prompt);

                                log("\n>>> Calling AI again for revision...".to_string());
                                debug!("Calling AI again after verification failure (feedback as user message).");
                                let mut builder = client.raw_builder(&state.system_prompt);
                                for msg in &state.messages {
                                    match msg.role {
                                        Role::System => {}
                                        Role::User => builder = builder.user(msg.content.clone()),
                                        Role::Assistant => builder = builder.assistant(msg.content.clone()),
                                    }
                                }

                                if config.interactive_output {
                                    // Print the standard message
                                    println!("{}", style("\nVerification failed. Revising response based on feedback...").yellow().italic());
                                    // Also print the specific feedback
                                    println!("{}", style(format!("Verifier Feedback: {}", feedback)).yellow().dim());
                                }

                                match builder.execute().await {
                                    Ok(revised_response) => {
                                        info!("Received revised AI response after verification failure (length: {}).", revised_response.len());
                                        log(format!("\n{}", crate::conversation_state::format_assistant_response_with_tool_calls(&revised_response)));
                                        state.add_assistant_message(&revised_response);
                                        current_response = revised_response;
                                        // Loop continues to re-evaluate the revised response
                                        continue; // Go to next loop iteration
                                    }
                                    Err(_e) => { // Prefixed unused variable
                                        error!("Error getting revised AI response after verification failure: {:?}", _e);
                                        warn!("Returning unverified response due to error during revision.");
                                        let outcome = VerificationOutcome {
                                            final_response: current_response, // Return the response *before* the failed revision attempt
                                            criteria: Some(criteria.to_string()),
                                            verification_passed: Some(false),
                                            verification_feedback: feedback_opt,
                                        };
                                        log(format!("\n--- Error During Revision Attempt: {} ---", e));
                                        log(format!("Returning previous (failed verification) response:\n```\n{}\n```", outcome.final_response));
                                        return Ok(outcome); // Return the last known response before the error
                                    }
                                }
                            } else {
                                // Verification failed but no feedback provided
                                warn!("Verification failed without feedback for server '{}'. Returning unverified response.", server_name);
                                let outcome = VerificationOutcome {
                                    final_response: current_response,
                                    criteria: Some(criteria.to_string()),
                                    verification_passed: Some(false),
                                    verification_feedback: None,
                                };
                                log("\n--- Verification Failed (No Feedback Provided) ---".to_string());
                                log(format!("Returning unverified response:\n```\n{}\n```", outcome.final_response));
                                return Ok(outcome); // Return the unverified response
                            }
                        }
                    }
                    Err(e) => {
                        // Verification call itself failed
                        error!("Error during verification call for server '{}': {}", server_name, e);
                        warn!("Returning unverified response due to verification error.");
                        let outcome = VerificationOutcome {
                            final_response: current_response,
                            criteria: Some(criteria.to_string()),
                            verification_passed: None,
                            verification_feedback: Some(format!("Verification Error: {}", e)),
                        };
                        log(format!("\n--- Verification Call Error: {} ---", e));
                        log(format!("Returning unverified response:\n```\n{}\n```", outcome.final_response));
                        return Ok(outcome); // Return the unverified response
                    }
                }
            } // End of if/else for tool_calls.is_empty()
        } // End loop
    })
    .await
}

/// Internal helper to execute a single tool call. Handles multi-server lookup.
async fn execute_single_tool_internal(
    host: &MCPHost,
    server_context: &str, // Can be specific server name or "*all*"
    tool_name: &str,
    args: serde_json::Value,
    config: &ConversationConfig, // Now includes optional log_sender
) -> Result<String> {
    debug!("Attempting to execute tool '{}' in context '{}'", tool_name, server_context);

    // --- Determine Target Server ---
    let target_server_name = if server_context == "*all*" {
        // Find the server that provides this tool
        match host.get_server_for_tool(tool_name).await {
            Ok(name) => name,
            Err(e) => {
                // Tool not found on any server
                let error_msg = format!("Tool '{}' not found on any available server.", tool_name);
                error!("{}", error_msg);
                // Return the error message as the result for the AI to see
                return Ok(error_msg);
            }
        }
    } else {
        // Use the specific server context provided
        server_context.to_string()
    };
    debug!("Target server for tool '{}' is '{}'", tool_name, target_server_name);

    // --- Logging Setup ---
    // Removed unused 'log' closure definition
    // --- End Logging Setup ---

    // Prepare progress message (only used if interactive)
    let progress_msg = format!(
        "Calling tool '{}' on server '{}'...",
        style(tool_name).yellow(),
        style(&target_server_name).green() // Use target_server_name
    );

    // Execute with or without progress indicator based on config
    let result_string = if config.interactive_output {
        crate::repl::with_progress(
            progress_msg, // Already styled
            host.call_tool(&target_server_name, tool_name, args), // Use target_server_name
        )
        .await
    } else {
        // Execute directly without progress spinner
        host.call_tool(&target_server_name, tool_name, args).await // Use target_server_name
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
                    // Show the actual server used in the output
                    crate::conversation_state::format_tool_response(&format!("{} (on {})", tool_name, target_server_name), &truncated_output)
                );
            }
            debug!("Tool '{}' executed successfully on server '{}'.", tool_name, target_server_name);
            Ok(truncated_output) // Return the truncated output
        }
        Err(error) => {
            let error_msg = format!("Error executing tool '{}' on server '{}': {}", tool_name, target_server_name, error);
            error!("{}", error_msg); // Log the error

            // Format error for printing if interactive
            if config.interactive_output {
                let formatted_error = format!(
                    "{} executing tool '{}' on server '{}': {}",
                    style("Error").red(),
                    style(tool_name).yellow(),
                    style(&target_server_name).green(), // Include server name in error
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
