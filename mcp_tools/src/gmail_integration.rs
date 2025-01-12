use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, io};
use anyhow::{anyhow, Result};
use reqwest::Client;
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine as _;
use tracing::{debug, error};

use shared_protocol_objects::{
    CallToolParams, CallToolResult, JsonRpcResponse, ToolInfo, ToolResponseContent,
    success_response, error_response
};

/// Minimal struct for storing tokens.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GmailToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub token_type: String,
    pub scope: Option<String>,
}

/// Basic config for OAuth
#[derive(Debug, Serialize, Deserialize)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    #[serde(default = "default_auth_uri")]
    pub auth_uri: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_auth_uri() -> String {
    "https://accounts.google.com/o/oauth2/v2/auth".to_string()
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".to_string()
}

fn default_scopes() -> Vec<String> {
    vec![
        "https://www.googleapis.com/auth/gmail.send".to_string(),
        "https://www.googleapis.com/auth/gmail.readonly".to_string(),
        "https://www.googleapis.com/auth/gmail.modify".to_string(),
    ]
}

impl GoogleOAuthConfig {
    pub fn from_env() -> Result<Self> {
        // Check all required environment variables upfront
        let missing_vars: Vec<&str> = vec![
            "GOOGLE_OAUTH_CLIENT_ID",
            "GOOGLE_OAUTH_CLIENT_SECRET", 
            "GOOGLE_OAUTH_REDIRECT_URI"
        ].into_iter()
        .filter(|&var| std::env::var(var).is_err())
        .collect();

        if !missing_vars.is_empty() {
            return Err(anyhow!(
                "Missing required environment variables:\n{}\n\nPlease set these variables before using Gmail integration.",
                missing_vars.join("\n")
            ));
        }

        Ok(Self {
            client_id: std::env::var("GOOGLE_OAUTH_CLIENT_ID").unwrap(),
            client_secret: std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").unwrap(),
            redirect_uri: std::env::var("GOOGLE_OAUTH_REDIRECT_URI").unwrap(),
            ..Default::default()
        })
    }
}

impl Default for GoogleOAuthConfig {
    fn default() -> Self {
        Self {
            client_id: "".into(),
            client_secret: "".into(),
            redirect_uri: "".into(),
            auth_uri: default_auth_uri(),
            token_uri: default_token_uri(),
            scopes: default_scopes(),
        }
    }
}

/// OAuth 2.0 token response from Google
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
    pub scope: String,
    #[serde(rename = "token_type")]
    pub token_type: String,
}

/// Parameters accepted by our Gmail tool.
#[derive(Debug, Serialize, Deserialize)]
struct GmailParams {
    /// "auth_init", "auth_exchange", "send_message", "list_messages", "read_message", "search_messages"
    action: String,

    /// For "auth_exchange"
    code: Option<String>,

    /// For "send_message"
    to: Option<String>,
    subject: Option<String>,
    body: Option<String>,

    /// For "read_message"
    message_id: Option<String>,

    /// For pagination, listing, etc.
    page_size: Option<u32>,

    /// For "search_messages"
    search_query: Option<String>,
}

/// Metadata about an email message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMetadata {
    pub id: String,
    pub thread_id: String,
    pub subject: Option<String>,
    pub from: Option<String>,
    pub snippet: Option<String>,
}

/// Return a static `ToolInfo` describing the input JSON schema for your Gmail tool.
pub fn gmail_tool_info() -> ToolInfo {
    ToolInfo {
        name: "gmail_tool".to_string(),
        description: Some("Gmail integration tool for OAuth 2.0 login, search, and send/receive operations.".into()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: 'auth_init', 'auth_exchange', 'send_message', 'list_messages', 'read_message', 'search_messages'"
                },
                "code": {"type": "string", "description": "Authorization code (if 'auth_exchange')."},
                "to": {"type": "string", "description": "Recipient email for sending messages."},
                "subject": {"type": "string", "description": "Subject of the email to send."},
                "body": {"type": "string", "description": "Body of the email to send."},
                "message_id": {"type": "string", "description": "Message ID to read."},
                "page_size": {"type": "number", "description": "How many messages to list, for 'list_messages'."},
                "search_query": {
                    "type": "string", 
                    "description": "Gmail search query. Examples: 'is:unread', 'from:someone@example.com', 'subject:important', 'after:2024/01/01', 'has:attachment'. Default: 'is:unread'. See https://support.google.com/mail/answer/7190?hl=en for more search operators."
                }
            },
            "required": ["action"]
        }),
    }
}

/// Handle the actual logic for each "action"
pub async fn handle_gmail_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    debug!("handle_gmail_tool_call invoked with params: {:?}", params);

    // Parse JSON arguments into our GmailParams struct
    let gmail_params: GmailParams = serde_json::from_value(params.arguments)
        .map_err(|e| anyhow!("Invalid GmailParams: {}", e))?;

    match gmail_params.action.as_str() {
        "auth_init" => {
            // Check if we already have a valid token
            if let Ok(Some(_token)) = read_cached_token() {
                let content = "Already authorized! No need to re-authenticate.\nUse other Gmail actions directly.";
                Ok(success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: content.to_string(),
                            annotations: None,
                        }],
                        is_error: None,
                        _meta: None,
                        progress: None,
                        total: None,
                    })?,
                ))
            } else {
                // 1. Generate an OAuth 2.0 URL for user consent
                let config = GoogleOAuthConfig::from_env()
                    .map_err(|e| anyhow!("Failed to load OAuth config: {}", e))?;
                let auth_url = build_auth_url(&config)
                    .map_err(|e| anyhow!("Failed to build auth URL: {}", e))?;
                let content = format!("Navigate to this URL to authorize:\n\n{}", auth_url);

                Ok(success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: content,
                            annotations: None,
                        }],
                        is_error: None,
                        _meta: None,
                        progress: None,
                        total: None,
                    })?,
                ))
            }
        }

        "auth_exchange" => {
            // 2. Exchange the authorization code for an access/refresh token
            let code = gmail_params
                .code
                .clone()
                .ok_or_else(|| anyhow!("'code' is required for 'auth_exchange'"))?;
            let config = GoogleOAuthConfig::from_env()
                .map_err(|e| anyhow!("Failed to load OAuth config: {}", e))?;
            let token_response = exchange_code_for_token(&config, &code).await?;

            // Store the token on disk
            let gmail_token = GmailToken {
                access_token: token_response.access_token,
                refresh_token: token_response.refresh_token,
                expires_in: token_response.expires_in,
                token_type: token_response.token_type,
                scope: Some(token_response.scope),
            };
            store_cached_token(&gmail_token)?;

            let success_message = "OAuth exchange successful! Access token acquired and stored.";
            Ok(success_response(
                id,
                serde_json::to_value(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: success_message.to_string(),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None,
                })?,
            ))
        }

        "send_message" => {
            // 3. Send an email (check for cached token)
            let token = match read_cached_token()? {
                Some(t) => t,
                None => {
                    return Ok(missing_auth_response(
                        id, 
                        "No OAuth token found. Please do 'auth_init' + 'auth_exchange' first."
                    ));
                }
            };

            let to = gmail_params
                .to
                .clone()
                .ok_or_else(|| anyhow!("'to' is required for 'send_message'"))?;
            let subject = gmail_params
                .subject
                .clone()
                .ok_or_else(|| anyhow!("'subject' is required for 'send_message'"))?;
            let body = gmail_params
                .body
                .clone()
                .ok_or_else(|| anyhow!("'body' is required for 'send_message'"))?;

            send_gmail_message(&token.access_token, &to, &subject, &body).await?;

            Ok(success_response(
                id,
                serde_json::to_value(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: format!("Email to '{}' sent successfully.", to),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None,
                })?,
            ))
        }

        "list_messages" => {
            // 4. List user’s messages
            let token = match read_cached_token()? {
                Some(t) => t,
                None => {
                    return Ok(missing_auth_response(
                        id, 
                        "No OAuth token found. Please do 'auth_init' + 'auth_exchange' first."
                    ));
                }
            };

            let page_size = gmail_params.page_size.unwrap_or(10);
            let messages = list_gmail_messages(&token.access_token, page_size).await?;

            // Format a text output
            let mut output = String::new();
            for (i, msg_id) in messages.iter().enumerate() {
                output.push_str(&format!("{}: {}\n", i + 1, msg_id));
            }
            if output.is_empty() {
                output = "No messages found.".to_string();
            }

            Ok(success_response(
                id,
                serde_json::to_value(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: output,
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None,
                })?,
            ))
        }

        "read_message" => {
            // 5. Read message content by ID
            let token = match read_cached_token()? {
                Some(t) => t,
                None => {
                    return Ok(missing_auth_response(
                        id, 
                        "No OAuth token found. Please do 'auth_init' + 'auth_exchange' first."
                    ));
                }
            };

            let msg_id = gmail_params
                .message_id
                .clone()
                .ok_or_else(|| anyhow!("'message_id' is required for 'read_message'"))?;

            let msg_body = read_gmail_message(&token.access_token, &msg_id).await?;

            Ok(success_response(
                id,
                serde_json::to_value(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: format!("Message ID: {}\n\n{}", msg_id, msg_body),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None,
                })?,
            ))
        }

        "search_messages" => {
            // 6. Search for messages that match a query (unread, etc.) and return metadata
            let token = match read_cached_token()? {
                Some(t) => t,
                None => {
                    return Ok(missing_auth_response(
                        id, 
                        "No OAuth token found. Please do 'auth_init' + 'auth_exchange' first."
                    ));
                }
            };

            // Default to "is:unread" if no query is provided
            let query = gmail_params
                .search_query
                .clone()
                .unwrap_or_else(|| "is:unread".to_string());
            let page_size = gmail_params.page_size.unwrap_or(10);

            // Call our new metadata function
            let messages = search_gmail_messages_with_metadata(
                &token.access_token, &query, page_size
            ).await?;

            // Convert to JSON or text
            let json_output = serde_json::to_string_pretty(&messages)?;

            Ok(success_response(
                id,
                serde_json::to_value(CallToolResult {
                    content: vec![ToolResponseContent {
                        type_: "text".into(),
                        text: format!("Found {} messages matching '{}':\n{}", 
                                    messages.len(), query, json_output),
                        annotations: None,
                    }],
                    is_error: Some(false),
                    _meta: None,
                    progress: None,
                    total: None,
                })?,
            ))
        }

        _ => {
            // Invalid action
            Err(anyhow!("Invalid action '{}'", gmail_params.action))
        }
    }
}

/// Build the Google OAuth 2.0 authorization URL to get the user’s consent.
fn build_auth_url(config: &GoogleOAuthConfig) -> Result<String> {
    let scopes_str = config.scopes.join(" ");
    Ok(format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        config.auth_uri,
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&config.redirect_uri),
        urlencoding::encode(&scopes_str),
    ))
}

/// Exchange an auth code for an access token & refresh token
async fn exchange_code_for_token(config: &GoogleOAuthConfig, code: &str) -> Result<TokenResponse> {
    let client = Client::new();
    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("code", code),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("grant_type", "authorization_code"),
    ];

    let response = client
        .post(&config.token_uri)
        .form(&params)
        .send()
        .await?
        .json::<TokenResponse>()
        .await?;

    Ok(response)
}

/// Send a Gmail message
pub async fn send_gmail_message(access_token: &str, to: &str, subject: &str, body: &str) -> Result<()> {
    let client = Client::new();
    let email_content = format!("From: me\r\nTo: {}\r\nSubject: {}\r\n\r\n{}", to, subject, body);
    let encoded_email = base64::encode(email_content.as_bytes());

    let payload = serde_json::json!({
        "raw": encoded_email
    });

    let resp = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(access_token)
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        error!("Gmail send error: {}", msg);
        return Err(anyhow!("Failed to send email: {}", msg));
    }

    Ok(())
}

/// List message IDs from user’s Gmail
pub async fn list_gmail_messages(access_token: &str, page_size: u32) -> Result<Vec<String>> {
    let client = Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?pageSize={}",
        page_size
    );

    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let messages = match resp.get("messages") {
        Some(arr) => arr
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect(),
        None => vec![],
    };

    Ok(messages)
}

/// Read the raw text of a single message
pub async fn read_gmail_message(access_token: &str, message_id: &str) -> Result<String> {
    let client = Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=raw",
        message_id
    );

    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // The "raw" field is base64-URL-coded
    let raw = resp
        .get("raw")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("No 'raw' field in Gmail message"))?;

    // Decode base64 (URL-safe variant)
    let bytes = URL_SAFE.decode(raw)?;
    let decoded = String::from_utf8(bytes)?;

    Ok(decoded)
}

/// Search for messages matching `query` and return basic metadata for each.
pub async fn search_gmail_messages_with_metadata(
    access_token: &str,
    query: &str,
    page_size: u32,
) -> Result<Vec<EmailMetadata>> {
    // 1. First, list the matching messages
    let client = Client::new();
    let list_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}&maxResults={}",
        urlencoding::encode(query),
        page_size
    );

    let list_resp = client
        .get(&list_url)
        .bearer_auth(access_token)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    // The "messages" field is an array of objects with "id" and "threadId"
    let messages = match list_resp.get("messages") {
        Some(arr) => arr.as_array().unwrap_or(&vec![]).to_owned(),
        None => vec![],
    };

    // 2. For each message, fetch metadata
    let mut results = Vec::new();
    for msg in messages {
        let msg_id = match msg.get("id").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };
        let thread_id = msg
            .get("threadId")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // GET message with `format=metadata`
        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
            msg_id
        );
        let metadata_resp = client
            .get(&msg_url)
            .bearer_auth(access_token)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        // Extract snippet
        let snippet = metadata_resp
            .get("snippet")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // We find subject/from in `payload.headers[]`
        let mut subject = None;
        let mut from = None;

        if let Some(payload) = metadata_resp.get("payload") {
            if let Some(headers) = payload.get("headers").and_then(|h| h.as_array()) {
                for header in headers {
                    if let (Some(name), Some(value)) = (header.get("name"), header.get("value")) {
                        if let (Some(name_str), Some(value_str)) = (name.as_str(), value.as_str()) {
                            match name_str.to_lowercase().as_str() {
                                "subject" => subject = Some(value_str.to_string()),
                                "from" => from = Some(value_str.to_string()),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Build our struct
        let email_meta = EmailMetadata {
            id: msg_id.to_string(),
            thread_id,
            subject,
            from,
            snippet,
        };
        results.push(email_meta);
    }

    Ok(results)
}

//
// -------------------- TOKEN STORAGE LOGIC --------------------
//

/// Get the path to `~/token_store/gmail_token.json`, creating the directory if needed.
fn get_token_store_path() -> Result<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("Unable to determine the user's home directory"))?;
    
    let token_store_dir = home_dir.join("token_store");
    if !token_store_dir.exists() {
        fs::create_dir_all(&token_store_dir)
            .map_err(|e| anyhow!("Failed to create token_store dir: {}", e))?;
    }

    let token_file = token_store_dir.join("gmail_token.json");
    Ok(token_file)
}

/// Read the token from the token store (if it exists).
fn read_cached_token() -> Result<Option<GmailToken>> {
    let token_file = get_token_store_path()?;
    if !token_file.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&token_file)?;
    let token: GmailToken = serde_json::from_str(&data)?;
    Ok(Some(token))
}

/// Write the token to the token store.
fn store_cached_token(token: &GmailToken) -> Result<()> {
    let token_file = get_token_store_path()?;
    let data = serde_json::to_string_pretty(token)?;
    fs::write(token_file, data)?;
    Ok(())
}

/// Helper that returns a "please authenticate" response.
fn missing_auth_response(id: Option<Value>, msg: &str) -> JsonRpcResponse {
    success_response(
        id,
        json!(CallToolResult {
            content: vec![ToolResponseContent {
                type_: "text".into(),
                text: msg.to_string(),
                annotations: None,
            }],
            is_error: Some(true),
            _meta: None,
            progress: None,
            total: None,
        }),
    )
}
