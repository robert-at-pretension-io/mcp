use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::env;
use tokio::time::timeout;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use shared_protocol_objects::{ToolInfo, CallToolResult, ToolResponseContent};
use shared_protocol_objects::{success_response, error_response, JsonRpcResponse, INVALID_PARAMS};
use shared_protocol_objects::CallToolParams;
use base64::Engine;
use oracle::{SqlValue};
use oracle::sql_type::OracleType;

#[derive(Debug, Deserialize, Serialize)]
struct OracleSelectParams {
    sql_query: String,
}

pub fn oracle_select_tool_info() -> ToolInfo {
    ToolInfo {
        name: "oracle_select".to_string(),
        description: Some(
            "Executes a SELECT query on an Oracle database. Only SELECT statements are allowed.
            Queries must be efficient and use best practices:
            
            1. Limit large result sets (use ROWNUM, FETCH FIRST).
            2. Avoid SELECT * when not needed.
            3. Include WHERE clauses for filtering.
            4. For metadata queries, limit results and filter by schema.
            
            Example:
            {
                \"action\": \"oracle_select\",
                \"params\": {
                    \"sql_query\": \"SELECT table_name FROM user_tables WHERE ROWNUM < 10 ORDER BY table_name\"
                }
            }".to_string()
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "sql_query": {
                    "type": "string",
                    "description": "The SELECT SQL query to execute. Must begin with SELECT."
                }
            },
            "required": ["sql_query"],
            "additionalProperties": false
        }),
    }
}

pub async fn handle_oracle_select_tool_call(
    params: CallToolParams,
    id: Option<Value>,
) -> Result<JsonRpcResponse> {
    let args: OracleSelectParams = match serde_json::from_value(params.arguments) {
        Ok(a) => a,
        Err(e) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                &format!("Invalid parameters provided. Ensure 'sql_query' is provided and is a string. Error: {}", e)
            ))
        }
    };

    let query_trimmed = args.sql_query.trim_start().to_uppercase();
    if !query_trimmed.starts_with("SELECT") {
        return Ok(error_response(
            id,
            INVALID_PARAMS,
            "Only SELECT statements are allowed. Please modify the query to start with 'SELECT'."
        ));
    }

    // Retrieve DB connection parameters with explicit error messaging
    let user = match env::var("ORACLE_USER") {
        Ok(u) => u,
        Err(_) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                "Environment variable ORACLE_USER not set. Please set ORACLE_USER before running queries."
            ))
        }
    };

    let password = match env::var("ORACLE_PASSWORD") {
        Ok(p) => p,
        Err(_) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                "Environment variable ORACLE_PASSWORD not set. Please set ORACLE_PASSWORD before running queries."
            ))
        }
    };

    let connect_str = match env::var("ORACLE_CONNECT_STRING") {
        Ok(c) => c,
        Err(_) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                "Environment variable ORACLE_CONNECT_STRING not set. Please set ORACLE_CONNECT_STRING before running queries."
            ))
        }
    };

    // Connect and run query
    let rows = match run_select_query(user, password, connect_str, args.sql_query).await {
        Ok(rows) => rows,
        Err(e) => {
            let tool_res = CallToolResult {
                content: vec![ToolResponseContent {
                    type_: "text".into(),
                    text: format!("Error executing query: {}. Consider checking:\n\
                    - That the database is reachable and credentials are correct\n\
                    - The query syntax and table/column names\n\
                    - If there's network latency or firewall issues\n\
                    - If the query is too complex or missing indexes, consider using ROWNUM or FETCH FIRST\n\
                    Original error: {}", e, e),
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
    let rows = timeout(Duration::from_secs(5), async {
        tokio::task::spawn_blocking(move || -> Result<Vec<serde_json::Value>> {
            let conn = oracle::Connection::connect(&user, &password, &connect_str)
                .with_context(|| format!("Failed to connect to Oracle using provided credentials and connection string: user={}, connect_str={}", user, connect_str))?;

            let mut stmt = conn.statement(&query).build()
                .with_context(|| format!("Failed to prepare statement. Check your SQL syntax: {}", query))?;
            let rows = stmt.query(&[])
                .with_context(|| format!("Failed to execute query. Ensure the query is valid and accessible: {}", query))?;

            let mut results = Vec::new();
            for row_result in rows {
                let row = row_result
                    .with_context(|| "Failed to fetch a row from the result set. Check if the table or data is accessible.")?;
                
                let mut obj = serde_json::Map::new();
                
                for (i, col_info) in row.column_info().iter().enumerate() {
                    let oracle_type = col_info.oracle_type();
                    let col_name = col_info.name().to_string();
                    
                    // Handle NULL values first by attempting to get an Option
                    if row.get::<_, Option<String>>(i + 1).ok() == Some(None) {
                        obj.insert(col_name, Value::Null);
                        continue;
                    }

                    let val: Value = match oracle_type {
                        // Handle string types
                        OracleType::Varchar2(_) | OracleType::Char(_) | OracleType::NVarchar2(_) | OracleType::NChar(_) => {
                            match row.get::<_, String>(i + 1) {
                                Ok(s) => Value::String(s),
                                Err(_) => Value::Null
                            }
                        },
                        
                        // Handle numeric types
                        OracleType::Number(_, _) | OracleType::Float(_) | OracleType::BinaryFloat | OracleType::BinaryDouble => {
                            // Try integer first
                            if let Ok(n) = row.get::<_, i64>(i + 1) {
                                Value::Number(n.into())
                            } else if let Ok(f) = row.get::<_, f64>(i + 1) {
                                // Then try float
                                if let Some(num) = serde_json::Number::from_f64(f) {
                                    Value::Number(num)
                                } else {
                                    Value::Null
                                }
                            } else {
                                // If both fail, try as string (for very large numbers)
                                match row.get::<_, String>(i + 1) {
                                    Ok(s) => Value::String(s),
                                    Err(_) => Value::Null
                                }
                            }
                        },
                        
                        // Handle date/timestamp types
                        OracleType::Date | OracleType::Timestamp(_) | OracleType::TimestampTZ(_) | OracleType::TimestampLTZ(_) => {
                            match row.get::<_, chrono::NaiveDateTime>(i + 1) {
                                Ok(d) => Value::String(d.to_string()),
                                Err(_) => Value::Null
                            }
                        },
                        
                        // Handle BLOB/RAW types
                        OracleType::Raw(_) | OracleType::BLOB => {
                            match row.get::<_, Vec<u8>>(i + 1) {
                                Ok(bytes) => Value::String(base64::engine::general_purpose::STANDARD.encode(bytes)),
                                Err(_) => Value::Null
                            }
                        },
                        
                        // Handle CLOB types
                        OracleType::CLOB | OracleType::NCLOB => {
                            match row.get::<_, String>(i + 1) {
                                Ok(s) => Value::String(s),
                                Err(_) => Value::Null
                            }
                        },
                        
                        // For any other types, try as string first then fall back to null
                        _ => match row.get::<_, String>(i + 1) {
                            Ok(s) => Value::String(s),
                            Err(_) => Value::Null
                        }
                    };
                    
                    obj.insert(col_name, val);
                }
                results.push(Value::Object(obj));
            }
            Ok(results)
        }).await?
    }).await.map_err(|_| {
        anyhow!("Query execution timed out after 30 seconds. Consider simplifying the query, adding indexes, or limiting the result set with ROWNUM or FETCH FIRST.")
    })??;

    Ok(rows)
}
