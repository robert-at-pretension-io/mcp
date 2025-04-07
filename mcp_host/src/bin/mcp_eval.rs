use anyhow::{Result, Context, anyhow};
use mcp_host::MCPHost;
// Removed duplicate imports below
// use anyhow::{Result, Context, anyhow};
// use mcp_host::MCPHost;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap; // Added HashMap import
use std::path::{PathBuf}; // Removed unused Path import
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use log::{info, error, debug};
use shellexpand; // Added shellexpand
use shared_protocol_objects::Role; // Import Role

// Define a serializable version of the Message struct
#[derive(Serialize, Debug, Clone)]
struct SerializableMessage {
    role: String, // Store role as string for simpler serialization
    content: String,
}

impl From<&mcp_host::conversation_state::Message> for SerializableMessage {
    fn from(msg: &mcp_host::conversation_state::Message) -> Self {
        let role_str = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
        }.to_string();
        Self {
            role: role_str,
            content: msg.content.clone(),
        }
    }
}


#[derive(Deserialize, Debug, Clone)]
struct ProviderConfig {
    name: String,
    model: String,
    api_key: Option<String>, // Optional override
}

#[derive(Deserialize, Debug)]
struct EvalConfig {
    mcp_host_config: String,
    tasks_dir: String,
    grading_prompt_path: String,
    output_path: String,
    task_timeout_secs: u64,
    grading_timeout_secs: u64,
    providers: Vec<ProviderConfig>,
}

#[derive(Serialize, Debug)]
struct EvalResult {
    task_file: String,
    performing_provider: String,
    performing_model: String,
    response: String, // Final response text
    conversation_history: Vec<SerializableMessage>, // Full conversation history
    grading_provider: String,
    grading_model: String,
    grade: Option<Value>, // Store parsed JSON grade
    execution_duration_secs: f64,
    grading_duration_secs: f64,
    execution_error: Option<String>,
    grading_error: Option<String>,
    // Verification fields
    verification_criteria: Option<String>,
    verification_passed: Option<bool>,
    verification_feedback: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Basic logging setup (consider using the setup from main_repl if needed)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // Load .env file
    dotenvy::dotenv().ok();

    info!("Starting MCP Evaluation Runner");

    // --- Load Configuration ---
    let config_path = "eval_config.toml";
    let config_str = fs::read_to_string(config_path)
        .await
        .with_context(|| format!("Failed to read eval config file: {}", config_path))?;
    let config: EvalConfig = toml::from_str(&config_str)
        .with_context(|| format!("Failed to parse eval config file: {}", config_path))?;

    info!("Loaded configuration: {} providers, tasks from '{}'", config.providers.len(), config.tasks_dir);

    let grading_prompt_template = fs::read_to_string(&config.grading_prompt_path)
        .await
        .with_context(|| format!("Failed to read grading prompt file: {}", config.grading_prompt_path))?;

    // --- Setup MCP Host ---
    // Use the config path specified in eval_config.toml
    let host_config_path = PathBuf::from(shellexpand::tilde(&config.mcp_host_config).into_owned());
    info!("Setting up MCPHost with config: {:?}", host_config_path);
    let host = MCPHost::builder()
        .config_path(host_config_path.clone()) // Clone the path here
        .client_info("mcp-eval-runner", "0.1.0")
        .build()
        .await
        .context("Failed to build MCPHost")?;

    // Apply initial config to start servers defined in mcp_host_config.json
    let initial_host_config = { host.config.lock().await.clone() };
    if let Err(e) = host.apply_config(initial_host_config).await {
         error!("Failed to apply initial server configuration: {}. Tool servers might not be running.", e);
         // Decide whether to continue or exit
         // return Err(e.into());
    } else {
         info!("Applied initial server configuration.");
         // Add log to check server count after applying config
         let server_count = host.servers.lock().await.len();
         info!("MCPHost has {} servers configured after applying initial config.", server_count);
         if server_count == 0 {
             error!("No tool servers were loaded from the config '{}'. Evaluation tasks requiring tools will fail.", host_config_path.display());
         }
    }


    // --- Prepare Output File ---
    let output_path = PathBuf::from(&config.output_path);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let output_file = Arc::new(Mutex::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&output_path)
            .await
            .with_context(|| format!("Failed to open output file: {:?}", output_path))?,
    ));

    // --- Iterate Through Tasks ---
    let mut task_paths = Vec::new();
    let mut read_dir = fs::read_dir(&config.tasks_dir).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            task_paths.push(path);
        }
    }
    info!("Found {} tasks.", task_paths.len());

    for task_path in task_paths {
        let task_file_name = task_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        info!("--- Processing Task: {} ---", task_file_name);

        let user_request = fs::read_to_string(&task_path)
            .await
            .with_context(|| format!("Failed to read task file: {:?}", task_path))?;

        // Store (Option<VerificationOutcome>, Option<history>, Option<error>, duration) keyed by provider_model
        let mut task_results: HashMap<String, (Option<VerificationOutcome>, Option<Vec<mcp_host::conversation_state::Message>>, Option<String>, f64)> = HashMap::new();

        // --- Execute Task with each Performer LLM ---
        for performer_config in &config.providers {
            let performer_id = format!("{}/{}", performer_config.name, performer_config.model);
            info!("Executing task with Performer: {}", performer_id);

            // Set the performer LLM
            if let Err(e) = set_provider_and_model(&host, performer_config).await {
                error!("Failed to set performer {}: {}", performer_id, e);
                // Insert None for Outcome and History on setup error
                task_results.insert(performer_id.clone(), (None, None, Some(format!("Failed to set provider/model: {}", e)), 0.0));
                continue;
            }

            // Execute the task simulation
            let start_time = Instant::now();
            let execution_result = tokio::time::timeout(
                Duration::from_secs(config.task_timeout_secs),
                execute_task_simulation(&host, &user_request)
            ).await;
            let duration = start_time.elapsed().as_secs_f64();

            // Removed: let mut outcome_opt: Option<VerificationOutcome> = None; // Outcome is now stored directly in task_results

            match execution_result {
                // Handle the new return type (VerificationOutcome, Vec<Message>)
                Ok(Ok((outcome, history))) => {
                    info!("Performer {} finished task in {:.2}s. Verification passed: {:?}", performer_id, duration, outcome.verification_passed);
                    // Insert the tuple (Option<Outcome>, Option<History>, Option<Error>, Duration)
                    task_results.insert(performer_id.clone(), (Some(outcome), Some(history), None, duration));
                }
                Ok(Err(e)) => {
                    error!("Performer {} failed task execution: {}", performer_id, e);
                    let error_msg = format!("Task execution error: {}", e);
                    // Insert None for Outcome and History on error
                    task_results.insert(performer_id.clone(), (None, None, Some(error_msg), duration));
                }
                Err(_) => {
                    error!("Performer {} timed out after {}s", performer_id, config.task_timeout_secs);
                    let timeout_msg = format!("Task execution timed out after {}s", config.task_timeout_secs);
                    // Insert None for Outcome and History on timeout
                    task_results.insert(performer_id.clone(), (None, None, Some(timeout_msg), duration));
                }
            }
        }

        // --- Grade each Response with each Grader LLM ---
        // task_results now stores: (Option<VerificationOutcome>, Option<history>, Option<error>, duration)

        for (performer_id, (outcome_opt, history_opt, execution_error, execution_duration)) in &task_results {
            let parts: Vec<&str> = performer_id.split('/').collect();
            let performing_provider = parts.get(0).cloned().unwrap_or("unknown");
            let performing_model = parts.get(1).cloned().unwrap_or("unknown");

            // Extract final response from outcome if available, otherwise use empty string
            let final_response = outcome_opt.as_ref().map_or("".to_string(), |o| o.final_response.clone());

            // Convert history to serializable format, filtering out system messages
            let serializable_history = history_opt
                .as_ref()
                .map(|hist| {
                    hist.iter()
                        .filter(|msg| msg.role != Role::System) // Filter out system messages
                        .map(SerializableMessage::from)
                        .collect()
                })
                .unwrap_or_else(Vec::new);

            for grader_config in &config.providers {
                let grader_id = format!("{}/{}", grader_config.name, grader_config.model);
                info!("Grading response from {} using Grader: {}", performer_id, grader_id);

                // Set the grader LLM
                if let Err(e) = set_provider_and_model(&host, grader_config).await {
                    error!("Failed to set grader {}: {}", grader_id, e);
                    let result = EvalResult {
                        task_file: task_file_name.clone(),
                        performing_provider: performing_provider.to_string(),
                        performing_model: performing_model.to_string(),
                        response: final_response.clone(),
                        conversation_history: serializable_history.clone(), // Add history
                        grading_provider: grader_config.name.clone(),
                        grading_model: grader_config.model.clone(),
                        grade: None,
                        execution_duration_secs: *execution_duration,
                        grading_duration_secs: 0.0,
                        execution_error: execution_error.clone(),
                        grading_error: Some(format!("Failed to set grader provider/model: {}", e)),
                        // Add verification fields (default/empty on grader setup error)
                        verification_criteria: None,
                        verification_passed: None,
                        verification_feedback: None,
                    };
                    write_result(&output_file, &result).await?;
                    continue; // Skip grading for this grader
                }

                // Grade the response
                let start_time = Instant::now();
                let grading_result = tokio::time::timeout(
                    Duration::from_secs(config.grading_timeout_secs),
                    grade_response(&host, &user_request, &final_response, &grading_prompt_template) // Pass reference &final_response
                ).await;
                let grading_duration = start_time.elapsed().as_secs_f64();

                let (grade, grading_error) = match grading_result {
                    Ok(Ok(parsed_grade)) => {
                        info!("Grader {} finished grading in {:.2}s", grader_id, grading_duration);
                        (Some(parsed_grade), None)
                    }
                    Ok(Err(e)) => {
                        error!("Grader {} failed grading: {}", grader_id, e);
                        (None, Some(format!("Grading error: {}", e)))
                    }
                    Err(_) => {
                        error!("Grader {} timed out after {}s", grader_id, config.grading_timeout_secs);
                        (None, Some(format!("Grading timed out after {}s", config.grading_timeout_secs)))
                    }
                };

                // --- Prepare Result ---
                let result = EvalResult {
                    task_file: task_file_name.clone(),
                    performing_provider: performing_provider.to_string(),
                    performing_model: performing_model.to_string(),
                    response: final_response.clone(),
                    conversation_history: serializable_history.clone(), // Add history
                    grading_provider: grader_config.name.clone(),
                    grading_model: grader_config.model.clone(),
                    grade,
                    execution_duration_secs: *execution_duration,
                    grading_duration_secs: grading_duration,
                    execution_error: execution_error.clone(),
                    grading_error,
                    // Add verification fields from the outcome_opt retrieved from task_results
                    verification_criteria: outcome_opt.as_ref().and_then(|o| o.criteria.clone()),
                    verification_passed: outcome_opt.as_ref().and_then(|o| o.verification_passed),
                    verification_feedback: outcome_opt.as_ref().and_then(|o| o.verification_feedback.clone()),
                };
                // Write the result immediately after grading attempt
                write_result(&output_file, &result).await?;
            }
        }
        // Removed the final loop that wrote all results at once

        info!("--- Finished Task: {} ---", task_file_name);
    }

    info!("Evaluation complete. Results saved to {:?}", output_path);
    Ok(())
}

async fn set_provider_and_model(host: &MCPHost, config: &ProviderConfig) -> Result<()> {
    // Override API key from config if provided
    if let Some(key) = &config.api_key {
        let key_var = MCPHost::get_api_key_var(&config.name)
            .ok_or_else(|| anyhow!("Cannot determine env var for provider {}", config.name))?;
        info!("Temporarily setting API key for {} from eval_config", config.name);
        std::env::set_var(key_var, key);
        // Note: This sets it for the whole process. Consider more isolated ways if needed.
    }

    host.set_active_provider(&config.name).await?;
    host.set_active_model(&config.name, &config.model).await?;
    Ok(())
}

use mcp_host::conversation_logic::VerificationOutcome; // Corrected import path

/// Simulates a single chat turn for task execution, allowing tool use.
/// Returns the final VerificationOutcome AND the full conversation history.
/// Uses the shared conversation_logic module.
async fn execute_task_simulation(host: &MCPHost, user_request: &str) -> Result<(VerificationOutcome, Vec<mcp_host::conversation_state::Message>)> {
    info!("Simulating task execution for request: '{}'", user_request.lines().next().unwrap_or(""));
    // 1. Get active client
    let client = host.ai_client().await
        .ok_or_else(|| anyhow!("No AI client active for task execution"))?;

    // 2. Determine server name for tools
    let server_name = {
        let servers_guard = host.servers.lock().await;
        servers_guard.keys().next().cloned()
            .ok_or_else(|| anyhow!("No tool servers configured/running for simulation"))? // Return error if no server
    };
    info!("Using server '{}' for tool context in simulation.", server_name);

    // 3. Create initial conversation state
    let mut state = host.enter_chat_mode(&server_name).await?;

    // --- Generate Verification Criteria FIRST ---
    let criteria_result = mcp_host::conversation_logic::generate_verification_criteria(host, user_request).await; // Corrected path
    let mut final_user_input = user_request.to_string();
    let criteria_for_verification: String; // Store clean criteria for verification step

    match criteria_result {
        Ok(c) if !c.is_empty() => {
            log::debug!("Generated criteria for eval:\n{}", c);
            criteria_for_verification = c.clone(); // Store for later verification
            // Append criteria to the user input that the LLM will see
            final_user_input.push_str(&format!(
                "\n\n---\n**Note:** Your response will be evaluated against the following criteria:\n{}\n---",
                c
            ));
            log::debug!("Appended criteria to user input for eval LLM.");
        }
        Ok(_) => {
            // Criteria generation succeeded but was empty
            log::debug!("Generated criteria were empty for eval.");
            criteria_for_verification = String::new();
        }
        Err(e) => {
            log::warn!("Failed to generate verification criteria for eval: {}. Proceeding without verification.", e);
            criteria_for_verification = String::new(); // Use empty criteria if generation fails
        }
    }
    // --- End Criteria Generation ---

    // 4. Add potentially modified user request to state
    state.add_user_message(&final_user_input); // Use the input with appended criteria

    // 5. Build and execute the *initial* AI request
    let initial_response = {
         // Pass system prompt when creating builder
         let mut builder = client.raw_builder(&state.system_prompt);
         for msg in state.messages.iter() {
             match msg.role {
                 // System messages are handled by injection now, skip adding them here
                 shared_protocol_objects::Role::System => {} // Skip system message from state
                 shared_protocol_objects::Role::User => builder = builder.user(msg.content.clone()),
                 shared_protocol_objects::Role::Assistant => builder = builder.assistant(msg.content.clone()),
             }
         }
         debug!("Executing initial AI request for simulation...");
         // Capture the specific error from the AI client
         match builder.execute().await {
             Ok(resp) => resp,
             Err(e) => {
                 // Log the detailed error before returning the context error
                 error!("Detailed error during initial AI execution: {:?}", e);
                 return Err(e).context("Initial AI execution failed during simulation");
             }
         }
    };
    debug!("Received initial AI response for simulation (length: {})", initial_response.len());

    // 6. Resolve the rest of the turn using the shared logic (non-interactive)
    // Use mcp_host::conversation_logic instead of crate::
    // Use default config which now has max_tool_iterations = 3
    let config = mcp_host::conversation_logic::ConversationConfig {
        interactive_output: false, // <<< Key difference: Non-interactive
        ..Default::default() // Use default for max_tool_iterations
    };

    // Use mcp_host::conversation_logic instead of crate::
    let outcome = mcp_host::conversation_logic::resolve_assistant_response(
        host,
        &server_name,
        &mut state, // Pass mutable state
        &initial_response, // Pass the first response
        client, // Pass the client Arc
        &config,
        &criteria_for_verification, // Pass the clean criteria string
    )
    .await
    .context("Failed to resolve assistant response during simulation")?;

    info!(
        "Task simulation finished. Verification passed: {:?}. Final response length: {}, History length: {}",
        outcome.verification_passed, outcome.final_response.len(), state.messages.len()
    );
    // Return the VerificationOutcome AND the conversation history messages
    Ok((outcome, state.messages))
}


/// Sends the request/response pair to the grading LLM and parses the grade.
async fn grade_response(
    host: &MCPHost,
    user_request: &str,
    assistant_response: &str,
    grading_prompt_template: &str,
) -> Result<Value> {
    info!("Grading response...");
    let client = host.ai_client().await
        .ok_or_else(|| anyhow!("No AI client active for grading"))?;

    // Prepare the grading prompt
    let prompt = grading_prompt_template
        .replace("{{USER_REQUEST}}", user_request)
        .replace("{{ASSISTANT_RESPONSE}}", assistant_response);

    // Execute the grading request
    // Use a builder that requests JSON output if the model supports it
    // Pass an empty system prompt for grading, as the tool prompt isn't relevant here.
    let builder = client.raw_builder("").user(prompt);

    // TODO: Add logic to request JSON mode if client.capabilities().supports_json_mode
    // This might involve specific parameters depending on the underlying LLM API.
    // For now, we rely on the prompt instructions.
    // Example (conceptual):
    // if client.capabilities().supports_json_mode {
    //     builder = builder.config(GenerationConfig { response_format: Some("json_object"), ..Default::default() });
    // }

    debug!("Executing grading request...");
    // Capture the specific error from the AI client
    let grade_response_str = match builder.execute().await {
        Ok(resp) => resp,
        Err(e) => {
            // Log the detailed error before returning the context error
            error!("Detailed error during grading AI execution: {:?}", e);
            return Err(e).context("Grading AI execution failed");
        }
    };
    debug!("Received grading response (length: {})", grade_response_str.len());

    // Attempt to parse the JSON from the response
    // Find the start of the JSON block (e.g., after "Your JSON Evaluation:\n")
    let json_start = grade_response_str.rfind('{');
    let json_end = grade_response_str.rfind('}');

    if let (Some(start), Some(end)) = (json_start, json_end) {
         if start < end {
             let json_str = &grade_response_str[start..=end];
             debug!("Extracted JSON string: {}", json_str);
             match serde_json::from_str(json_str) {
                 Ok(json_value) => {
                     info!("Successfully parsed grade JSON.");
                     return Ok(json_value);
                 },
                 Err(e) => {
                     error!("Failed to parse JSON grade from response: {}", e);
                     return Err(anyhow!("Failed to parse JSON grade from response: {}. Raw response: '{}'", e, grade_response_str));
                 }
             }
         }
    }

    error!("Could not find valid JSON object in grading response.");
    Err(anyhow!("Could not find valid JSON object in grading response: '{}'", grade_response_str))
}

async fn write_result(file: &Arc<Mutex<fs::File>>, result: &EvalResult) -> Result<()> {
    let mut json_str = serde_json::to_string(result)?;
    json_str.push('\n'); // Add newline for JSON Lines format

    let mut guard = file.lock().await;
    guard.write_all(json_str.as_bytes()).await?;
    guard.flush().await?; // Ensure it's written immediately
    Ok(())
}
