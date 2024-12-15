use anyhow::{anyhow, Result};
use serde_json::Value;
use std::env;
use tokio::time::timeout;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use shared_protocol_objects::{ToolInfo, CallToolResult, ToolResponseContent};
use shared_protocol_objects::{success_response, error_response, JsonRpcResponse, INVALID_PARAMS};
use shared_protocol_objects::CallToolParams;

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
    let rows = match run_select_query(&user, &password, &connect_str, &args.query).await {
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

async fn run_select_query(user: &str, password: &str, connect_str: &str, query: &str) -> Result<Vec<serde_json::Value>> {
    // Example using sqlx with Oracle. If using another Oracle client, adjust accordingly.
    // Ensure you have "sqlx-oracle" feature enabled and have added dependency "sqlx = { version = ..., features = ["runtime-tokio-rustls", "oracle"]}"
    let pool = sqlx::oracle::OraclePoolOptions::new()
        .connect_timeout(Duration::from_secs(10))
        .connect(&format!("oracle://{user}:{password}@{connect_str}")).await?;

    // Timeout the query after 30 seconds
    let rows = timeout(Duration::from_secs(30), sqlx::query(query).fetch_all(&pool)).await??;

    let mut results = Vec::new();
    for row in rows {
        let mut obj = serde_json::Map::new();
        for (i, col) in row.columns().iter().enumerate() {
            let val: Value = match row.try_get_raw(i) {
                Ok(raw) => {
                    if raw.is_null() {
                        Value::Null
                    } else {
                        // We attempt conversion by type name
                        match col.type_info().name().to_lowercase().as_str() {
                            "number" | "float" | "binary_float" | "binary_double" => {
                                let v: f64 = row.try_get(i)?;
                                Value::Number(serde_json::Number::from_f64(v).unwrap_or(Value::Null.into()))
                            }
                            "varchar2" | "char" | "nchar" | "nvarchar2" | "clob" => {
                                let v: String = row.try_get(i)?;
                                Value::String(v)
                            }
                            "date" | "timestamp" => {
                                let v: String = row.try_get(i)?;
                                Value::String(v)
                            }
                            _ => {
                                // If unknown type, try string fallback
                                let v: String = row.try_get(i)?;
                                Value::String(v)
                            }
                        }
                    }
                }
                Err(_) => Value::Null
            };
            obj.insert(col.name().to_string(), val);
        }
        results.push(Value::Object(obj));
    }

    Ok(results)
}