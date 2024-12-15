use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use tokio::time::timeout;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use shared_protocol_objects::{ToolInfo, CallToolResult, ToolResponseContent};
use shared_protocol_objects::{success_response, error_response, JsonRpcResponse, INVALID_PARAMS};
use shared_protocol_objects::CallToolParams;
use base64::Engine;

#[derive(Debug, Deserialize, Serialize)]
struct OracleSelectParams {
    query: String,
}

pub fn oracle_select_tool_info() -> ToolInfo {
    ToolInfo {
        name: "oracle_select".to_string(),
        description: Some(
            "Executes a SELECT query on an Oracle database. Only SELECT statements are allowed.
            
            usage:
            ```json
            {
                \"action\": \"oracle_select\",
                \"params\": {
                    \"query\": \"SELECT * FROM some_table WHERE ROWNUM < 10\"
                }
            }
            ```".to_string()
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The SELECT SQL query to execute"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        }),
    }
}

pub async fn handle_oracle_select_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let args: OracleSelectParams = serde_json::from_value(params.arguments)
        .map_err(|e| anyhow!("Invalid arguments: {}", e))?;

    let query_trimmed = args.query.trim_start().to_uppercase();
    if !query_trimmed.starts_with("SELECT") {
        // Only SELECT is allowed
        return Ok(error_response(id, INVALID_PARAMS, "Only SELECT statements allowed"));
    }

    // Retrieve DB connection parameters
    let user = env::var("ORACLE_USER").expect("ORACLE_USER must be set");
    let password = env::var("ORACLE_PASSWORD").expect("ORACLE_PASSWORD must be set");
    let connect_str = env::var("ORACLE_CONNECT_STRING").expect("ORACLE_CONNECT_STRING must be set");

    // Connect and run query
    let rows = match run_select_query(user, password, connect_str, args.query).await {
        Ok(rows) => rows,
        Err(e) => {
            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: format!("Error executing query: {}", e),
                    annotations: None,
                }],
                is_error: Some(true),
                _meta: None,
                progress: None,
                total: None,
            };
            return Ok(success_response(id, serde_json::to_value(tool_res)?));
        }
    };

    let tool_res = CallToolResult {
        content: vec![ToolResponseContent {
            type_: "text".into(),
            text: serde_json::to_string_pretty(&rows)?,
            annotations: None,
        }],
        is_error: Some(false),
        _meta: None,
        progress: None,
        total: None,
    };

    Ok(success_response(id, serde_json::to_value(tool_res)?))
}

async fn run_select_query(
    user: String,
    password: String,
    connect_str: String,
    query: String
) -> Result<Vec<serde_json::Value>> {
    // Execute the query with a timeout
    let rows = timeout(Duration::from_secs(30), async {
        // Since oracle crate is sync, we need to run in a blocking task
        tokio::task::spawn_blocking(move || -> Result<Vec<serde_json::Value>> {
            // Connect to Oracle
            let conn = oracle::Connection::connect(&user, &password, &connect_str)?;
            
            let mut stmt = conn.statement(&query).build()?;
            let rows = stmt.query(&[])?;
            
            let mut results = Vec::new();
            for row_result in rows {
                let row = row_result?;
                let mut obj = serde_json::Map::new();
                
                for (i, col_info) in row.column_info().iter().enumerate() {
                    let val: Value = match row.get::<_, String>(i + 1) {
                        Ok(val) => {
                            // Try to parse as number first
                            if let Ok(n) = val.parse::<f64>() {
                                if let Some(num) = serde_json::Number::from_f64(n) {
                                    Value::Number(num)
                                } else {
                                    Value::String(val)
                                }
                            } else {
                                Value::String(val)
                            }
                        }
                        Err(_) => {
                            // Try other types
                            if let Ok(n) = row.get::<_, i64>(i + 1) {
                                Value::Number(n.into())
                            } else if let Ok(f) = row.get::<_, f64>(i + 1) {
                                if let Some(num) = serde_json::Number::from_f64(f) {
                                    Value::Number(num)
                                } else {
                                    Value::Null
                                }
                            } else if let Ok(d) = row.get::<_, chrono::NaiveDateTime>(i + 1) {
                                Value::String(d.to_string())
                            } else if let Ok(bytes) = row.get::<_, Vec<u8>>(i + 1) {
                                Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
                            } else {
                                Value::Null
                            }
                        }
                    };
                    obj.insert(col_info.name().to_string(), val);
                }
                results.push(Value::Object(obj));
            }
            Ok(results)
        }).await?
    }).await??;

    Ok(rows)
}
