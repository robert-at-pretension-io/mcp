use serde::{ Deserialize, Serialize };
use serde_json::json;
use std::path::PathBuf;
use std::{ fs, time };
use anyhow::{ anyhow, Result };
use reqwest::Client;
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine as _; // Import the Engine trait
use tracing::{ debug, error };
use schemars::JsonSchema;
use rmcp::tool;

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

/// Parameters accepted by our Gmail tool for authentication initialization
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthInitParams {
    // No parameters needed for auth_init
}

/// Parameters accepted by our Gmail tool for authentication exchange
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AuthExchangeParams {
    #[schemars(description = "Authorization code from Google OAuth flow")]
    pub code: String,
}

/// Parameters accepted by our Gmail tool for sending messages
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SendMessageParams {
    #[schemars(description = "Recipient email for sending messages")]
    pub to: String,
    
    #[schemars(description = "Subject of the email to send")]
    pub subject: String,
    
    #[schemars(description = "Body of the email to send")]
    pub body: String,
}

/// Parameters accepted by our Gmail tool for listing messages
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListMessagesParams {
    #[serde(default = "default_page_size")]
    #[schemars(description = "Number of messages to list (default: 10)")]
    pub page_size: u32,
}

fn default_page_size() -> u32 {
    10
}

/// Parameters accepted by our Gmail tool for reading messages
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadMessageParams {
    #[schemars(description = "Message ID to read")]
    pub message_id: String,
}

/// Parameters accepted by our Gmail tool for searching messages
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchMessagesParams {
    #[serde(default = "default_search_query")]
    #[schemars(description = "Gmail search query. Examples: 'is:unread', 'from:someone@example.com', 'subject:important'")]
    pub search_query: String,
    
    #[serde(default = "default_page_size")]
    #[schemars(description = "Number of search results to return (default: 10)")]
    pub page_size: u32,
}

fn default_search_query() -> String {
    "is:unread".to_string()
}

/// Parameters accepted by our Gmail tool for modifying messages
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ModifyMessageParams {
    #[schemars(description = "Message ID to modify")]
    pub message_id: String,
    
    #[serde(default)]
    #[schemars(description = "If true, remove 'INBOX' label from the message (archive)")]
    pub archive: bool,
    
    #[serde(default)]
    #[schemars(description = "If true, remove 'UNREAD' label from the message")]
    pub mark_read: bool,
    
    #[serde(default)]
    #[schemars(description = "If true, add 'UNREAD' label to the message")]
    pub mark_unread: bool,
    
    #[serde(default)]
    #[schemars(description = "If true, add 'STARRED' label to the message")]
    pub star: bool,
    
    #[serde(default)]
    #[schemars(description = "If true, remove 'STARRED' label from the message")]
    pub unstar: bool,
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

#[derive(Debug, Clone)]
pub struct GmailTool;

impl GmailTool {
    pub fn new() -> Self {
        Self
    }
}

#[tool(tool_box)]
impl GmailTool {
    /// Initialize Gmail OAuth authentication flow
    #[tool(description = "Initiates OAuth authentication flow for Gmail. Provides a URL for user to authorize access.")]
    pub async fn auth_init(&self, #[tool(aggr)] _params: AuthInitParams) -> String {
        // If we already have a token in store, skip auth
        if let Ok(Some(_token)) = read_cached_token() {
            return "Already authorized! No need to re-authenticate.\nUse other Gmail actions directly.".to_string();
        } else {
            match GoogleOAuthConfig::from_env() {
                Ok(config) => match build_auth_url(&config) {
                    Ok(auth_url) => format!("Navigate to this URL to authorize:\n\n{}", auth_url),
                    Err(e) => format!("Failed to build auth URL: {}", e),
                },
                Err(e) => format!("Failed to load OAuth config: {}", e),
            }
        }
    }

    /// Exchange authorization code for access token
    #[tool(description = "Exchanges OAuth authorization code for access token. Use after completing the auth_init step.")]
    pub async fn auth_exchange(&self, #[tool(aggr)] params: AuthExchangeParams) -> String {
        let config = match GoogleOAuthConfig::from_env() {
            Ok(c) => c,
            Err(e) => return format!("Failed to load OAuth config: {}", e),
        };

        let token_response = match exchange_code_for_token(&config, &params.code).await {
            Ok(r) => r,
            Err(e) => return format!("Failed to exchange code for token: {}", e),
        };

        let now_secs = match current_epoch() {
            Ok(t) => t,
            Err(e) => return format!("Failed to get current time: {}", e),
        };
        
        let gmail_token = GmailToken {
            access_token: token_response.access_token,
            refresh_token: token_response.refresh_token,
            expires_in: token_response.expires_in,
            token_type: token_response.token_type,
            scope: Some(token_response.scope),
            obtained_at: now_secs,
        };

        // Store the token on disk
        match store_cached_token(&gmail_token) {
            Ok(_) => "OAuth exchange successful! Access token acquired and stored.".to_string(),
            Err(e) => format!("Failed to store token: {}", e),
        }
    }

    /// Send a Gmail message
    #[tool(description = "Sends an email message from your Gmail account. Requires prior authorization.")]
    pub async fn send_message(&self, #[tool(aggr)] params: SendMessageParams) -> String {
        // Make sure we have a valid token first
        let token = match get_or_refresh_token().await {
            Ok(t) => t,
            Err(e) => return format!(
                "Failed to get a valid token: {}. Please do 'auth_init' + 'auth_exchange'.",
                e
            ),
        };

        match send_gmail_message(&token.access_token, &params.to, &params.subject, &params.body).await {
            Ok(_) => format!("Email to '{}' sent successfully.", params.to),
            Err(e) => format!("Failed to send email: {}", e),
        }
    }

    /// List Gmail messages
    #[tool(description = "Lists recent messages from your Gmail inbox. Requires prior authorization.")]
    pub async fn list_messages(&self, #[tool(aggr)] params: ListMessagesParams) -> String {
        let token = match get_or_refresh_token().await {
            Ok(t) => t,
            Err(e) => return format!(
                "Failed to get a valid token: {}. Please re-authenticate.",
                e
            ),
        };

        match list_gmail_messages_with_metadata(&token.access_token, params.page_size).await {
            Ok(messages) => {
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
                output
            },
            Err(e) => format!("Failed to list messages: {}", e),
        }
    }

    /// Read a Gmail message
    #[tool(description = "Reads the content of a specific Gmail message. Requires message ID and prior authorization.")]
    pub async fn read_message(&self, #[tool(aggr)] params: ReadMessageParams) -> String {
        let token = match get_or_refresh_token().await {
            Ok(t) => t,
            Err(e) => return format!(
                "Failed to get a valid token: {}. Please re-authenticate.",
                e
            ),
        };

        match read_gmail_message(&token.access_token, &params.message_id).await {
            Ok(msg_body) => format!("Message ID: {}\n\n{}", params.message_id, msg_body),
            Err(e) => format!("Failed to read message: {}", e),
        }
    }

    /// Search Gmail messages
    #[tool(description = "Searches Gmail messages using Gmail search syntax. Requires prior authorization.")]
    pub async fn search_messages(&self, #[tool(aggr)] params: SearchMessagesParams) -> String {
        let token = match get_or_refresh_token().await {
            Ok(t) => t,
            Err(e) => return format!("Failed to get a valid token: {}.", e),
        };

        match search_gmail_messages_with_metadata(&token.access_token, &params.search_query, params.page_size).await {
            Ok(messages) => {
                match serde_json::to_string_pretty(&messages) {
                    Ok(json_output) => format!(
                        "Found {} messages matching '{}':\n{}",
                        messages.len(),
                        params.search_query,
                        json_output
                    ),
                    Err(e) => format!("Error formatting results: {}", e),
                }
            },
            Err(e) => format!("Failed to search messages: {}", e),
        }
    }

    /// Modify Gmail message labels
    #[tool(description = "Modifies Gmail message labels (archive, mark read/unread, star). Requires prior authorization.")]
    pub async fn modify_message(&self, #[tool(aggr)] params: ModifyMessageParams) -> String {
        let token = match get_or_refresh_token().await {
            Ok(t) => t,
            Err(e) => return format!("Failed to get a valid token: {}.", e),
        };

        // Decide which labels to add or remove
        let mut add_labels = Vec::new();
        let mut remove_labels = Vec::new();

        if params.archive {
            // Archiving => remove "INBOX"
            remove_labels.push("INBOX".to_string());
        }
        if params.mark_read {
            // Mark as read => remove "UNREAD"
            remove_labels.push("UNREAD".to_string());
        }
        if params.mark_unread {
            // Mark as unread => add "UNREAD"
            add_labels.push("UNREAD".to_string());
        }
        if params.star {
            // Star => add "STARRED"
            add_labels.push("STARRED".to_string());
        }
        if params.unstar {
            // Unstar => remove "STARRED"
            remove_labels.push("STARRED".to_string());
        }

        match modify_gmail_message_labels(&token.access_token, &params.message_id, &add_labels, &remove_labels).await {
            Ok(_) => format!(
                "Modified message {}. Added labels: {:?}, removed labels: {:?}",
                params.message_id, add_labels, remove_labels
            ),
            Err(e) => format!("Failed to modify message: {}", e),
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

    let payload = json!({
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

    let payload = json!({
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