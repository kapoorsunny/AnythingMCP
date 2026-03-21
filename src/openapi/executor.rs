use std::collections::HashMap;

use crate::error::{McpWrapError, Result};
use crate::openapi::models::{ApiParamLocation, ApiToolDefinition};
use crate::registry::models::ToolArgValue;

/// Execute an API tool by making an HTTP request.
pub async fn execute_api_tool(
    tool: &ApiToolDefinition,
    args: &HashMap<String, ToolArgValue>,
) -> Result<String> {
    let client = reqwest::Client::new();

    // Build URL with path parameters substituted
    let mut url = tool.url_template.clone();
    for param in &tool.params {
        if param.location == ApiParamLocation::Path {
            if let Some(value) = args.get(&param.name) {
                url = url.replace(&format!("{{{}}}", param.name), &value.to_string());
            } else if param.required {
                return Err(McpWrapError::InvalidArgType {
                    param: param.name.clone(),
                    expected: "required path parameter".to_string(),
                });
            }
        }
    }

    // Build query parameters
    let mut query_params = Vec::new();
    for param in &tool.params {
        if param.location == ApiParamLocation::Query {
            if let Some(value) = args.get(&param.name) {
                query_params.push((param.name.clone(), value.to_string()));
            }
        }
    }

    // Build request body from body params
    let mut body_map = serde_json::Map::new();
    for param in &tool.params {
        if param.location == ApiParamLocation::Body {
            if let Some(value) = args.get(&param.name) {
                let json_value = match value {
                    ToolArgValue::String(s) => serde_json::Value::String(s.clone()),
                    ToolArgValue::Integer(i) => serde_json::Value::Number((*i).into()),
                    ToolArgValue::Float(f) => serde_json::Value::Number(
                        serde_json::Number::from_f64(*f).unwrap_or(0.into()),
                    ),
                    ToolArgValue::Boolean(b) => serde_json::Value::Bool(*b),
                };
                body_map.insert(param.name.clone(), json_value);
            }
        }
    }

    // Build the request
    let mut request = match tool.method.as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        "HEAD" => client.head(&url),
        other => {
            return Err(McpWrapError::ExecutionFailed {
                exit_code: -1,
                stderr: format!("Unsupported HTTP method: {}", other),
            });
        }
    };

    // Add query params
    if !query_params.is_empty() {
        request = request.query(&query_params);
    }

    // Add body for non-GET methods
    if !body_map.is_empty() {
        request = request
            .header("Content-Type", "application/json")
            .json(&serde_json::Value::Object(body_map));
    }

    // Add auth header
    if let Some(ref auth) = tool.auth {
        let secret = std::env::var(&auth.auth_env).map_err(|_| McpWrapError::ExecutionFailed {
            exit_code: -1,
            stderr: format!(
                "Auth env var '{}' not set. Set it before starting the server.",
                auth.auth_env
            ),
        })?;

        match auth.auth_type.as_str() {
            "bearer" => {
                request = request.header("Authorization", format!("Bearer {}", secret));
            }
            "header" => {
                let header_name = auth.auth_header.as_deref().unwrap_or("X-API-Key");
                request = request.header(header_name, &secret);
            }
            "basic" => {
                // Env var should contain pre-encoded base64 credentials (base64 of "user:pass")
                request = request.header("Authorization", format!("Basic {}", secret));
            }
            _ => {}
        }
    }

    // Add custom header params from tool parameters
    for param in &tool.params {
        if param.location == ApiParamLocation::Header {
            if let Some(value) = args.get(&param.name) {
                request = request.header(&param.name, value.to_string());
            }
        }
    }

    // Add static headers (from import --header flags)
    for header in &tool.static_headers {
        // Try env var first, then literal value
        let value = header
            .env_var
            .as_ref()
            .and_then(|env| std::env::var(env).ok())
            .or_else(|| header.value.clone());

        if let Some(val) = value {
            request = request.header(&header.name, val);
        }
    }

    // Execute with timeout
    let response = tokio::time::timeout(std::time::Duration::from_secs(30), request.send())
        .await
        .map_err(|_| McpWrapError::ExecutionFailed {
            exit_code: -1,
            stderr: "HTTP request timed out after 30 seconds".to_string(),
        })?
        .map_err(|e| McpWrapError::ExecutionFailed {
            exit_code: -1,
            stderr: format!("HTTP request failed: {}", e),
        })?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| McpWrapError::ExecutionFailed {
            exit_code: -1,
            stderr: format!("Failed to read response body: {}", e),
        })?;

    if status.is_success() {
        Ok(body)
    } else {
        Err(McpWrapError::ExecutionFailed {
            exit_code: status.as_u16() as i32,
            stderr: body,
        })
    }
}
