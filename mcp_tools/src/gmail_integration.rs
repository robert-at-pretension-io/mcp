use serde::{ Deserialize, Serialize };
use serde_json::{ json, Value };
use std::path::PathBuf;
use std::{ fs, time }; // Removed unused io
use anyhow::{ anyhow, Result };
use reqwest::Client;
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine as _; // Import the Engine trait
use tracing::{ debug, error };

use shared_protocol_objects::{
    CallToolParams,
    CallToolResult,
    JsonRpcResponse,
    ToolInfo,
    ToolResponseContent,
    success_response,
    // Removed unused error_response
};

/// Minimal struct for storing tokens.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GmailToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,           // typically in seconds
    pub token_type: String,
    pub scope: Option<String>,

    /// When did we obtain this token? (Unix timestamp, seconds)
    /// We'll use this to check if it's expired or about to expire.
    #[serde(default)]
    pub obtained_at: i64,
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
        "https://www.googleapis.com/auth/gmail.modify".to_string()
    ]
}

impl GoogleOAuthConfig {
    pub fn from_env() -> Result<Self> {
        // Check all required environment variables upfront
        let missing_vars: Vec<&str> = vec![
            "GOOGLE_OAUTH_CLIENT_ID",
            "GOOGLE_OAUTH_CLIENT_SECRET",
            "GOOGLE_OAUTH_REDIRECT_URI"
        ]
            .into_iter()
            .filter(|&var| std::env::var(var).is_err())
            .collect();

        if !missing_vars.is_empty() {
            return Err(
                anyhow!(
                    "Missing required environment variables:\n{}\n\nPlease set these variables before using Gmail integration.",
                    missing_vars.join("\n")
                )
            );
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
    /// "auth_init", "auth_exchange", "send_message", "list_messages", "read_message", "search_messages", "modify_message"
    action: String,

    /// For "auth_exchange"
    code: Option<String>,

    /// For "send_message"
    to: Option<String>,
    subject: Option<String>,
    body: Option<String>,

    /// For "read_message" or "modify_message"
    message_id: Option<String>,

    /// For pagination, listing, etc.
    page_size: Option<u32>,

    /// For "search_messages"
    search_query: Option<String>,

    // --- Fields for "modify_message" ---
    #[serde(default)]
    archive: bool,
    #[serde(default)]
    mark_read: bool,
    #[serde(default)]
    mark_unread: bool,
    #[serde(default)]
    star: bool,
    #[serde(default)]
    unstar: bool,
}

/// Metadata about an email message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMetadata {
    pub id: String,
    pub thread_id: String,
    pub subject: Option<String>,
    pub from: Option<String>,
    /// Store the "To" header if present
    pub to: Option<String>,
    pub snippet: Option<String>,
}

/// Return a static `ToolInfo` describing the input JSON schema for your Gmail tool.
pub fn gmail_tool_info() -> ToolInfo {
    ToolInfo {
        name: "gmail_tool".to_string(),
        description: Some(
            "Gmail integration tool for OAuth 2.0 login, search, send/receive, and label operations. Make sure to explicitly provide the authorization URL to the user.".into()
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform: 'auth_init', 'auth_exchange', 'send_message', 'list_messages', 'read_message', 'search_messages', 'modify_message'"
                },
                "code": {"type": "string", "description": "Authorization code (if 'auth_exchange')."},
                "to": {"type": "string", "description": "Recipient email for sending messages."},
                "subject": {"type": "string", "description": "Subject of the email to send."},
                "body": {"type": "string", "description": "Body of the email to send."},
                "message_id": {"type": "string", "description": "Message ID to read or modify."},
                "page_size": {"type": "number", "description": "How many messages to list, for 'list_messages'."},
                "search_query": {
                    "type": "string", 
                    "description": "Gmail search query. Examples: 'is:unread', 'from:someone@example.com', 'subject:important', 'after:2024/01/01', 'has:attachment'. Default: 'is:unread'."
                },
                "archive": {
                    "type": "boolean",
                    "description": "If true, remove 'INBOX' label from the message (archive)."
                },
                "mark_read": {
                    "type": "boolean",
                    "description": "If true, remove 'UNREAD' label from the message."
                },
                "mark_unread": {
                    "type": "boolean",
                    "description": "If true, add 'UNREAD' label to the message."
                },
                "star": {
                    "type": "boolean",
                    "description": "If true, add 'STARRED' label to the message."
                },
                "unstar": {
                    "type": "boolean",
                    "description": "If true, remove 'STARRED' label from the message."
                }
            },
            "required": ["action"]
        }),
        annotations: None, // Added missing field
    }
}

/// Handle the actual logic for each "action"
pub async fn handle_gmail_tool_call(
    params: CallToolParams,
    id: Option<Value>
) -> Result<JsonRpcResponse> {
    debug!("handle_gmail_tool_call invoked with params: {:?}", params);

    // Parse JSON arguments into our GmailParams struct
    let gmail_params: GmailParams = serde_json
        ::from_value(params.arguments)
        .map_err(|e| anyhow!("Invalid GmailParams: {}", e))?;

    match gmail_params.action.as_str() {
        "auth_init" => {
            // If we already have a token in store, skip auth
            if let Ok(Some(_token)) = read_cached_token() {
                let content = "Already authorized! No need to re-authenticate.\nUse other Gmail actions directly.";
                Ok(
                    success_response(
                        id,
                        serde_json::to_value(CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: content.to_string(),
                                annotations: None,
                            }],
                            is_error: None,
                            // Removed _meta, progress, total fields
                        })?
                    )
                )
            } else {
                let config = GoogleOAuthConfig::from_env()
                    .map_err(|e| anyhow!("Failed to load OAuth config: {}", e))?;
                let auth_url = build_auth_url(&config)
                    .map_err(|e| anyhow!("Failed to build auth URL: {}", e))?;

                let content = format!(
                    "Navigate to this URL to authorize:\n\n{}",
                    auth_url
                );

                Ok(
                    success_response(
                        id,
                        serde_json::to_value(CallToolResult {
                            content: vec![ToolResponseContent {
                                type_: "text".into(),
                                text: content,
                                annotations: None,
                            }],
                            is_error: None,
                            // Removed _meta, progress, total fields
                        })?
                    )
                )
            }
        }

        "auth_exchange" => {
            // Exchange an authorization code for an access token & refresh token
            let code = gmail_params.code
                .clone()
                .ok_or_else(|| anyhow!("'code' is required for 'auth_exchange'"))?;

            let config = GoogleOAuthConfig::from_env()
                .map_err(|e| anyhow!("Failed to load OAuth config: {}", e))?;

            let token_response = exchange_code_for_token(&config, &code).await?;

            let now_secs = current_epoch()?;
            let gmail_token = GmailToken {
                access_token: token_response.access_token,
                refresh_token: token_response.refresh_token,
                expires_in: token_response.expires_in,
                token_type: token_response.token_type,
                scope: Some(token_response.scope),
                obtained_at: now_secs,
            };

            // Store the token on disk
            store_cached_token(&gmail_token)?;

            let success_message = "OAuth exchange successful! Access token acquired and stored.";
            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: success_message.to_string(),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        "send_message" => {
            // Make sure we have a valid token first
            let token = match get_or_refresh_token().await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(
                        missing_auth_response(
                            id,
                            &format!(
                                "Failed to get a valid token: {}. Please do 'auth_init' + 'auth_exchange'.",
                                e
                            )
                        )
                    )
                }
            };

            let to = gmail_params.to
                .clone()
                .ok_or_else(|| anyhow!("'to' is required for 'send_message'"))?;
            let subject = gmail_params.subject
                .clone()
                .ok_or_else(|| anyhow!("'subject' is required for 'send_message'"))?;
            let body = gmail_params.body
                .clone()
                .ok_or_else(|| anyhow!("'body' is required for 'send_message'"))?;

            send_gmail_message(&token.access_token, &to, &subject, &body).await?;

            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: format!("Email to '{}' sent successfully.", to),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        "list_messages" => {
            let token = match get_or_refresh_token().await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(
                        missing_auth_response(
                            id,
                            &format!(
                                "Failed to get a valid token: {}. Please re-authenticate.",
                                e
                            )
                        )
                    )
                }
            };

            let page_size = gmail_params.page_size.unwrap_or(10);
            let messages = list_gmail_messages_with_metadata(&token.access_token, page_size).await?;

            let mut output = String::new();
            if messages.is_empty() {
                output.push_str("No messages found.");
            } else {
                for (i, msg) in messages.iter().enumerate() {
                    output.push_str(&format!(
                        "{index}. ID: {id}\n   From: {from}\n   To: {to}\n   Subject: {subject}\n   Snippet: {snippet}\n\n",
                        index = i + 1,
                        id = msg.id,
                        from = msg.from.as_deref().unwrap_or("Unknown"),
                        to = msg.to.as_deref().unwrap_or("Unknown"),
                        subject = msg.subject.as_deref().unwrap_or("(No subject)"),
                        snippet = msg.snippet.as_deref().unwrap_or("(No preview available)")
                    ));
                }
            }

            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: output,
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        "read_message" => {
            let token = match get_or_refresh_token().await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(
                        missing_auth_response(
                            id,
                            &format!(
                                "Failed to get a valid token: {}. Please re-authenticate.",
                                e
                            )
                        )
                    )
                }
            };

            let msg_id = gmail_params.message_id
                .clone()
                .ok_or_else(|| anyhow!("'message_id' is required for 'read_message'"))?;

            let msg_body = read_gmail_message(&token.access_token, &msg_id).await?;

            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: format!("Message ID: {}\n\n{}", msg_id, msg_body),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        "search_messages" => {
            let token = match get_or_refresh_token().await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(
                        missing_auth_response(
                            id,
                            &format!("Failed to get a valid token: {}.", e)
                        )
                    )
                }
            };

            let query = gmail_params.search_query
                .clone()
                .unwrap_or_else(|| "is:unread".to_string());
            let page_size = gmail_params.page_size.unwrap_or(10);

            let messages = search_gmail_messages_with_metadata(
                &token.access_token,
                &query,
                page_size
            ).await?;

            let json_output = serde_json::to_string_pretty(&messages)?;

            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: format!(
                                "Found {} messages matching '{}':\n{}",
                                messages.len(),
                                query,
                                json_output
                            ),
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        "modify_message" => {
            let token = match get_or_refresh_token().await {
                Ok(t) => t,
                Err(e) => {
                    return Ok(
                        missing_auth_response(
                            id,
                            &format!("Failed to get a valid token: {}.", e)
                        )
                    )
                }
            };

            let msg_id = gmail_params.message_id
                .clone()
                .ok_or_else(|| anyhow!("'message_id' is required for 'modify_message'"))?;

            // Decide which labels to add or remove
            let mut add_labels = Vec::new();
            let mut remove_labels = Vec::new();

            if gmail_params.archive {
                // Archiving => remove "INBOX"
                remove_labels.push("INBOX".to_string());
            }
            if gmail_params.mark_read {
                // Mark as read => remove "UNREAD"
                remove_labels.push("UNREAD".to_string());
            }
            if gmail_params.mark_unread {
                // Mark as unread => add "UNREAD"
                add_labels.push("UNREAD".to_string());
            }
            if gmail_params.star {
                // Star => add "STARRED"
                add_labels.push("STARRED".to_string());
            }
            if gmail_params.unstar {
                // Unstar => remove "STARRED"
                remove_labels.push("STARRED".to_string());
            }

            modify_gmail_message_labels(&token.access_token, &msg_id, &add_labels, &remove_labels).await?;

            let summary = format!(
                "Modified message {}. Added labels: {:?}, removed labels: {:?}",
                msg_id, add_labels, remove_labels
            );

            Ok(
                success_response(
                    id,
                    serde_json::to_value(CallToolResult {
                        content: vec![ToolResponseContent {
                            type_: "text".into(),
                            text: summary,
                            annotations: None,
                        }],
                        is_error: Some(false),
                        // Removed _meta, progress, total fields
                    })?
                )
            )
        }

        _ => {
            // Invalid action
            Err(anyhow!("Invalid action '{}'", gmail_params.action))
        }
    }
}

/// ---------------------------------------
/// Helper: Build the Google OAuth 2.0 authorization URL
/// ---------------------------------------
fn build_auth_url(config: &GoogleOAuthConfig) -> Result<String> {
    let scopes_str = config.scopes.join(" ");
    Ok(
        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
            config.auth_uri,
            urlencoding::encode(&config.client_id),
            urlencoding::encode(&config.redirect_uri),
            urlencoding::encode(&scopes_str)
        )
    )
}

/// ---------------------------------------
/// Helper: Exchange an auth code for an access/refresh token
/// ---------------------------------------
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
        .send().await?
        .json::<TokenResponse>().await?;

    Ok(response)
}

/// ---------------------------------------
/// Helper: Refresh access token using refresh_token
/// ---------------------------------------
async fn refresh_access_token(token: &GmailToken) -> Result<GmailToken> {
    let config = GoogleOAuthConfig::from_env()
        .map_err(|e| anyhow!("Failed to load OAuth config: {}", e))?;

    if token.refresh_token.is_none() {
        return Err(anyhow!("No refresh token stored. Cannot refresh."));
    }

    let client = Client::new();
    let params = [
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("refresh_token", token.refresh_token.as_ref().unwrap().as_str()),
        ("grant_type", "refresh_token"),
    ];

    let response = client
        .post(&config.token_uri)
        .form(&params)
        .send().await?
        .json::<TokenResponse>().await?;

    let now_secs = current_epoch()?;
    // If Google doesn't return a new refresh_token, we keep the old one
    let new_refresh_token = if response.refresh_token.is_some() {
        response.refresh_token
    } else {
        token.refresh_token.clone()
    };

    // Build a new GmailToken
    let new_token = GmailToken {
        access_token: response.access_token,
        refresh_token: new_refresh_token,
        expires_in: response.expires_in,
        token_type: response.token_type,
        scope: Some(response.scope),
        obtained_at: now_secs,
    };

    // Persist it
    store_cached_token(&new_token)?;

    Ok(new_token)
}

/// ---------------------------------------
/// Helper: Return a guaranteed valid (not expired) token.
/// - If existing token is still valid, return it.
/// - If expired or near expiry, refresh.
/// - If refresh fails, return error.
/// ---------------------------------------
async fn get_or_refresh_token() -> Result<GmailToken> {
    let mut token = read_cached_token()?
        .ok_or_else(|| anyhow!("No token found on disk."))?;

    // If we are within N seconds of expiry, refresh the token.
    // For safety, let's refresh if < 60 seconds remain.
    let now_secs = current_epoch()?;
    let expiry_time = token.obtained_at + token.expires_in;
    let time_left = expiry_time - now_secs;

    if time_left < 60 {
        debug!("Access token near or past expiry, attempting refresh...");
        token = refresh_access_token(&token).await?;
    } else {
        debug!("Access token is still valid with {}s left.", time_left);
    }

    Ok(token)
}

/// ---------------------------------------
/// Send a Gmail message
/// ---------------------------------------
pub async fn send_gmail_message(
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str
) -> Result<()> {
    let client = Client::new();
    let email_content = format!("From: me\r\nTo: {}\r\nSubject: {}\r\n\r\n{}", to, subject, body);
    // Use the Engine trait for encoding
    let encoded_email = URL_SAFE.encode(email_content.as_bytes());

    let payload = serde_json::json!({
        "raw": encoded_email
    });

    let resp = client
        .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
        .bearer_auth(access_token)
        .json(&payload)
        .send().await?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        error!("Gmail send error: {}", msg);
        return Err(anyhow!("Failed to send email: {}", msg));
    }

    Ok(())
}

/// ---------------------------------------
/// List the user's Gmail messages (no query),
/// retrieving metadata for each
/// ---------------------------------------
pub async fn list_gmail_messages_with_metadata(
    access_token: &str,
    page_size: u32
) -> Result<Vec<EmailMetadata>> {
    let client = Client::new();

    // 1. List messages (no query), limited by page_size
    let list_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?pageSize={}",
        page_size
    );

    let list_resp = client
        .get(&list_url)
        .bearer_auth(access_token)
        .send().await?
        .json::<serde_json::Value>().await?;

    // "messages" is an array of { id, threadId }
    let messages = match list_resp.get("messages") {
        Some(arr) => arr.as_array().unwrap_or(&vec![]).to_owned(),
        None => vec![],
    };

    // 2. For each message, fetch `format=metadata` to parse subject, from, to, snippet
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

        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
            msg_id
        );
        let metadata_resp = client
            .get(&msg_url)
            .bearer_auth(access_token)
            .send().await?
            .json::<serde_json::Value>().await?;

        // Extract snippet
        let snippet = metadata_resp
            .get("snippet")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut subject = None;
        let mut from = None;
        let mut to = None;

        // Inspect payload.headers[] for Subject, From, To
        if let Some(payload) = metadata_resp.get("payload") {
            if let Some(headers) = payload.get("headers").and_then(|h| h.as_array()) {
                for header in headers {
                    if let (Some(name), Some(value)) = (header.get("name"), header.get("value")) {
                        if let (Some(name_str), Some(value_str)) = (name.as_str(), value.as_str()) {
                            match name_str.to_lowercase().as_str() {
                                "subject" => {
                                    subject = Some(value_str.to_string());
                                }
                                "from" => {
                                    from = Some(value_str.to_string());
                                }
                                "to" => {
                                    to = Some(value_str.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        let email_meta = EmailMetadata {
            id: msg_id.to_string(),
            thread_id,
            subject,
            from,
            to,
            snippet,
        };
        results.push(email_meta);
    }

    Ok(results)
}

/// ---------------------------------------
/// Read the raw text of a single message
/// ---------------------------------------
pub async fn read_gmail_message(access_token: &str, message_id: &str) -> Result<String> {
    let client = Client::new();
    let url = format!("https://gmail.googleapis.com/gmail/v1/users/me/messages/{}", message_id);

    let resp = client
        .get(&url)
        .bearer_auth(access_token)
        .send().await?
        .json::<serde_json::Value>().await?;

    let payload = resp
        .get("payload")
        .ok_or_else(|| anyhow!("No payload in Gmail message"))?;

    // Try to get the body directly from the top-level payload
    if let Some(body_data) = payload
        .get("body")
        .and_then(|b| b.get("data"))
        .and_then(|d| d.as_str())
    {
        let bytes = URL_SAFE.decode(body_data)?;
        return Ok(String::from_utf8(bytes)?);
    }

    // Otherwise, look in payload.parts for the first text/plain
    if let Some(parts) = payload.get("parts").and_then(|p| p.as_array()) {
        for part in parts {
            if let Some(mime_type) = part.get("mimeType").and_then(|m| m.as_str()) {
                if mime_type == "text/plain" {
                    if let Some(body_data) = part
                        .get("body")
                        .and_then(|b| b.get("data"))
                        .and_then(|d| d.as_str())
                    {
                        let bytes = URL_SAFE.decode(body_data)?;
                        return Ok(String::from_utf8(bytes)?);
                    }
                }
            }
        }
    }

    Err(anyhow!("Could not find message body"))
}

/// ---------------------------------------
/// Search for messages matching `query`
/// and return basic metadata
/// ---------------------------------------
pub async fn search_gmail_messages_with_metadata(
    access_token: &str,
    query: &str,
    page_size: u32
) -> Result<Vec<EmailMetadata>> {
    let client = Client::new();
    let list_url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}&maxResults={}",
        urlencoding::encode(query),
        page_size
    );

    let list_resp = client
        .get(&list_url)
        .bearer_auth(access_token)
        .send().await?
        .json::<serde_json::Value>().await?;

    let messages = match list_resp.get("messages") {
        Some(arr) => arr.as_array().unwrap_or(&vec![]).to_owned(),
        None => vec![],
    };

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

        let msg_url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
            msg_id
        );
        let metadata_resp = client
            .get(&msg_url)
            .bearer_auth(access_token)
            .send().await?
            .json::<serde_json::Value>().await?;

        let snippet = metadata_resp
            .get("snippet")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut subject = None;
        let mut from = None;
        let mut to = None;

        if let Some(payload) = metadata_resp.get("payload") {
            if let Some(headers) = payload.get("headers").and_then(|h| h.as_array()) {
                for header in headers {
                    if let (Some(name), Some(value)) = (header.get("name"), header.get("value")) {
                        if let (Some(name_str), Some(value_str)) = (name.as_str(), value.as_str()) {
                            match name_str.to_lowercase().as_str() {
                                "subject" => {
                                    subject = Some(value_str.to_string());
                                }
                                "from" => {
                                    from = Some(value_str.to_string());
                                }
                                "to" => {
                                    to = Some(value_str.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        results.push(EmailMetadata {
            id: msg_id.to_string(),
            thread_id,
            subject,
            from,
            to,
            snippet,
        });
    }

    Ok(results)
}

/// ---------------------------------------
/// Modify labels on a single message
/// (archive, mark as read/unread, star, etc.)
/// ---------------------------------------
pub async fn modify_gmail_message_labels(
    access_token: &str,
    message_id: &str,
    add_label_ids: &[String],
    remove_label_ids: &[String],
) -> Result<()> {
    let client = Client::new();
    let url = format!(
        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}/modify",
        message_id
    );

    let payload = serde_json::json!({
        "addLabelIds": add_label_ids,
        "removeLabelIds": remove_label_ids
    });

    let resp = client
        .post(url)
        .bearer_auth(access_token)
        .json(&payload)
        .send().await?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        error!("Gmail modify labels error: {}", msg);
        return Err(anyhow!("Failed to modify labels: {}", msg));
    }

    Ok(())
}

/// ---------------------------------------
/// TOKEN STORAGE + Utility
/// ---------------------------------------

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

fn read_cached_token() -> Result<Option<GmailToken>> {
    let token_file = get_token_store_path()?;
    if !token_file.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(&token_file)?;
    let token: GmailToken = serde_json::from_str(&data)?;
    Ok(Some(token))
}

fn store_cached_token(token: &GmailToken) -> Result<()> {
    let token_file = get_token_store_path()?;
    let data = serde_json::to_string_pretty(token)?;
    fs::write(token_file, data)?;
    Ok(())
}

/// Return the current Unix epoch time in seconds
fn current_epoch() -> Result<i64> {
    let now = time::SystemTime::now()
        .duration_since(time::UNIX_EPOCH)
        .map_err(|e| anyhow!("Failed to get system time: {}", e))?;
    Ok(now.as_secs() as i64)
}

/// Return a "please authenticate" response
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
            // Removed _meta, progress, total fields
        })
    )
}
