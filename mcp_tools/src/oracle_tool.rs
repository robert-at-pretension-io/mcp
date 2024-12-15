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
    service: Option<String>,
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
            
            You can specify which service to connect to using the 'service' parameter:
            - 'edbt' (default) for edbt.world
            - 'ecomt' for ecomt.world
            
            Example:
            {
                \"sql_query\": \"SELECT table_name FROM user_tables WHERE ROWNUM < 10\",
                \"service\": \"ecomt\"
            }".to_string()
        ),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "sql_query": {
                    "type": "string",
                    "description": "The SELECT SQL query to execute. Must begin with SELECT."
                },
                "service": {
                    "type": "string",
                    "enum": ["edbt", "ecomt"],
                    "description": "Which database service to connect to (edbt.world or ecomt.world). Defaults to edbt if not specified."
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

    let base_connect_str = match env::var("ORACLE_CONNECT_STRING") {
        Ok(c) => c,
        Err(_) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                "Environment variable ORACLE_CONNECT_STRING not set. Please set ORACLE_CONNECT_STRING before running queries."
            ))
        }
    };

    // Modify connection string based on service parameter
    let connect_str = match args.service.as_deref() {
        Some("ecomt") => "jdbc:oracle:thin:@//tst-dbs-ora.akc.org:1521/ecomt.world".to_string(),
        Some("edbt") | None => base_connect_str,
        Some(other) => {
            return Ok(error_response(
                id,
                INVALID_PARAMS,
                &format!("Invalid service '{}'. Must be either 'edbt' or 'ecomt'.", other)
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
                    
                    // Convert everything to string representation
                    let val = match row.get::<_, Option<String>>(i + 1) {
                        Ok(Some(s)) => Value::String(s),  // Got a string value
                        Ok(None) => Value::String("null".to_string()),  // NULL value
                        Err(_) => {
                            // Try getting as raw string representation for any type
                            match row.get::<_, String>(i + 1) {
                                Ok(s) => Value::String(s),
                                Err(_) => Value::String("null".to_string())  // Fallback if conversion fails
                            }
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
