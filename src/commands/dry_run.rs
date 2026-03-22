use std::collections::HashMap;

use crate::error::{McpWrapError, Result};
use crate::mcp::schema::{api_tool_to_mcp_schema, tool_to_mcp_schema};
use crate::openapi::store::ApiToolRegistry;
use crate::registry::models::ToolArgValue;
use crate::registry::store::ToolRegistry;

/// Build the command-line preview for a CLI tool call
pub fn build_cli_preview(
    command: &str,
    args: &HashMap<String, ToolArgValue>,
) -> Result<Vec<String>> {
    let tokens = shell_words::split(command)
        .map_err(|e| McpWrapError::RegistryError(format!("Cannot parse command: {}", e)))?;

    if tokens.is_empty() {
        return Err(McpWrapError::RegistryError("Empty command".to_string()));
    }

    let mut preview = tokens;

    for (key, value) in args {
        match value {
            ToolArgValue::Boolean(true) => {
                preview.push(format!("--{}", key));
            }
            ToolArgValue::Boolean(false) => {
                // Skipped — not added
            }
            _ => {
                preview.push(format!("--{}", key));
                preview.push(value.to_string());
            }
        }
    }

    Ok(preview)
}

/// Build the HTTP request preview for an API tool call
pub fn build_api_preview(
    tool: &crate::openapi::models::ApiToolDefinition,
    args: &HashMap<String, serde_json::Value>,
) -> String {
    let mut url = tool.url_template.clone();
    let mut query_params = Vec::new();
    let mut body_params = serde_json::Map::new();
    let mut headers = Vec::new();

    for param in &tool.params {
        if let Some(value) = args.get(&param.name) {
            match param.location {
                crate::openapi::models::ApiParamLocation::Path => {
                    let val_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    url = url.replace(&format!("{{{}}}", param.name), &val_str);
                }
                crate::openapi::models::ApiParamLocation::Query => {
                    let val_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    query_params.push(format!("{}={}", param.name, val_str));
                }
                crate::openapi::models::ApiParamLocation::Header => {
                    let val_str = match value {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    headers.push(format!("{}: {}", param.name, val_str));
                }
                crate::openapi::models::ApiParamLocation::Body => {
                    body_params.insert(param.name.clone(), value.clone());
                }
            }
        }
    }

    if !query_params.is_empty() {
        url = format!("{}?{}", url, query_params.join("&"));
    }

    let mut preview = format!("{} {}", tool.method, url);

    if let Some(ref auth) = tool.auth {
        let env_set = std::env::var(&auth.auth_env).is_ok();
        let status = if env_set { "set" } else { "NOT SET" };
        match auth.auth_type.as_str() {
            "bearer" => preview.push_str(&format!(
                "\n  Authorization: Bearer <${} ({})>",
                auth.auth_env, status
            )),
            "basic" => preview.push_str(&format!(
                "\n  Authorization: Basic <${} ({})>",
                auth.auth_env, status
            )),
            "header" => {
                let header_name = auth.auth_header.as_deref().unwrap_or("X-API-Key");
                preview.push_str(&format!(
                    "\n  {}: <${} ({})>",
                    header_name, auth.auth_env, status
                ));
            }
            _ => {}
        }
    }

    for header in &headers {
        preview.push_str(&format!("\n  {}", header));
    }

    if !body_params.is_empty() {
        if let Ok(body_json) = serde_json::to_string_pretty(&serde_json::Value::Object(body_params))
        {
            preview.push_str(&format!("\n  Body: {}", body_json));
        }
    }

    preview
}

/// Parse JSON arguments and convert to ToolArgValue map using the tool's param schema
fn parse_args_for_cli(
    tool: &crate::registry::models::ToolDefinition,
    args_json: &str,
) -> Result<HashMap<String, ToolArgValue>> {
    let raw: serde_json::Value = serde_json::from_str(args_json)?;
    let obj = raw.as_object().ok_or_else(|| {
        McpWrapError::RegistryError("Arguments must be a JSON object".to_string())
    })?;

    let mut result = HashMap::new();

    for (key, value) in obj {
        // Find the param in the tool schema
        let param = tool.params.iter().find(|p| p.name == *key);

        let arg_value = match param {
            Some(p) => match p.param_type {
                crate::registry::models::ParamType::Boolean => {
                    ToolArgValue::Boolean(value.as_bool().unwrap_or(false))
                }
                crate::registry::models::ParamType::Integer => {
                    ToolArgValue::Integer(value.as_i64().unwrap_or(0))
                }
                crate::registry::models::ParamType::Float => {
                    ToolArgValue::Float(value.as_f64().unwrap_or(0.0))
                }
                crate::registry::models::ParamType::String => {
                    ToolArgValue::String(value.as_str().unwrap_or("").to_string())
                }
            },
            None => {
                // Unknown param — treat as string
                ToolArgValue::String(value.as_str().unwrap_or(&value.to_string()).to_string())
            }
        };

        result.insert(key.clone(), arg_value);
    }

    Ok(result)
}

/// Run the dry-run command
pub fn run(
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
    name: &str,
    args_json: &str,
) -> Result<()> {
    // Try CLI tools first
    if let Some(tool) = registry.get(name)? {
        let schema = tool_to_mcp_schema(&tool);
        println!("--- Dry Run: {} (CLI) ---", name);
        println!();
        println!("Schema:");
        println!(
            "{}",
            serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string())
        );
        println!();

        let typed_args = parse_args_for_cli(&tool, args_json)?;
        let preview = build_cli_preview(&tool.command, &typed_args)?;

        println!("Command that would be executed:");
        println!("  {}", shell_words::join(&preview));
        println!();
        println!("Individual tokens:");
        for (i, token) in preview.iter().enumerate() {
            println!("  [{}] {}", i, token);
        }

        return Ok(());
    }

    // Try API tools
    if let Some(api_reg) = api_registry {
        if let Some(tool) = api_reg.get(name)? {
            let schema = api_tool_to_mcp_schema(&tool);
            println!("--- Dry Run: {} (API {}) ---", name, tool.method);
            println!();
            println!("Schema:");
            println!(
                "{}",
                serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string())
            );
            println!();

            let raw: serde_json::Value = serde_json::from_str(args_json)?;
            let args_map: HashMap<String, serde_json::Value> = raw
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();

            let preview = build_api_preview(&tool, &args_map);
            println!("HTTP request that would be sent:");
            println!("  {}", preview);

            return Ok(());
        }
    }

    eprintln!("Error: tool '{}' not found.", name);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openapi::models::{ApiParam, ApiParamLocation, ApiToolDefinition, AuthConfig};
    use crate::registry::models::{ParamType, ToolDefinition, ToolParam, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;

    fn make_param(name: &str, ptype: ParamType) -> ToolParam {
        ToolParam {
            name: name.to_string(),
            description: String::new(),
            param_type: ptype,
            required: false,
            default_value: None,
        }
    }

    fn make_tool(name: &str, params: Vec<ToolParam>) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            command: "python script.py".to_string(),
            description: "Test tool".to_string(),
            params,
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_build_cli_preview_no_args() {
        let args = HashMap::new();
        let preview = build_cli_preview("python script.py", &args).unwrap();
        assert_eq!(preview, vec!["python", "script.py"]);
    }

    #[test]
    fn test_build_cli_preview_string_args() {
        let mut args = HashMap::new();
        args.insert(
            "input".to_string(),
            ToolArgValue::String("file.csv".to_string()),
        );

        let preview = build_cli_preview("python script.py", &args).unwrap();
        assert!(preview.contains(&"--input".to_string()));
        assert!(preview.contains(&"file.csv".to_string()));
    }

    #[test]
    fn test_build_cli_preview_boolean_true() {
        let mut args = HashMap::new();
        args.insert("verbose".to_string(), ToolArgValue::Boolean(true));

        let preview = build_cli_preview("echo", &args).unwrap();
        assert!(preview.contains(&"--verbose".to_string()));
    }

    #[test]
    fn test_build_cli_preview_boolean_false_skipped() {
        let mut args = HashMap::new();
        args.insert("verbose".to_string(), ToolArgValue::Boolean(false));

        let preview = build_cli_preview("echo", &args).unwrap();
        assert!(!preview.contains(&"--verbose".to_string()));
    }

    #[test]
    fn test_build_cli_preview_integer_args() {
        let mut args = HashMap::new();
        args.insert("count".to_string(), ToolArgValue::Integer(42));

        let preview = build_cli_preview("echo", &args).unwrap();
        assert!(preview.contains(&"--count".to_string()));
        assert!(preview.contains(&"42".to_string()));
    }

    #[test]
    fn test_build_cli_preview_float_args() {
        let mut args = HashMap::new();
        args.insert("rate".to_string(), ToolArgValue::Float(3.14));

        let preview = build_cli_preview("echo", &args).unwrap();
        assert!(preview.contains(&"--rate".to_string()));
        assert!(preview.contains(&"3.14".to_string()));
    }

    #[test]
    fn test_build_cli_preview_empty_command() {
        let args = HashMap::new();
        let result = build_cli_preview("", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_api_preview_get() {
        let tool = ApiToolDefinition {
            name: "get_user".to_string(),
            description: "Get user".to_string(),
            method: "GET".to_string(),
            url_template: "https://api.example.com/users/{id}".to_string(),
            params: vec![ApiParam {
                name: "id".to_string(),
                description: "User ID".to_string(),
                param_type: ParamType::String,
                required: true,
                location: ApiParamLocation::Path,
            }],
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            source_spec: "spec.json".to_string(),
        };

        let mut args = HashMap::new();
        args.insert("id".to_string(), serde_json::json!("123"));

        let preview = build_api_preview(&tool, &args);
        assert!(preview.contains("GET"));
        assert!(preview.contains("https://api.example.com/users/123"));
        assert!(!preview.contains("{id}"));
    }

    #[test]
    fn test_build_api_preview_with_query() {
        let tool = ApiToolDefinition {
            name: "search".to_string(),
            description: "Search".to_string(),
            method: "GET".to_string(),
            url_template: "https://api.example.com/search".to_string(),
            params: vec![ApiParam {
                name: "q".to_string(),
                description: "Query".to_string(),
                param_type: ParamType::String,
                required: true,
                location: ApiParamLocation::Query,
            }],
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            source_spec: "spec.json".to_string(),
        };

        let mut args = HashMap::new();
        args.insert("q".to_string(), serde_json::json!("test"));

        let preview = build_api_preview(&tool, &args);
        assert!(preview.contains("q=test"));
    }

    #[test]
    fn test_build_api_preview_with_body() {
        let tool = ApiToolDefinition {
            name: "create_user".to_string(),
            description: "Create user".to_string(),
            method: "POST".to_string(),
            url_template: "https://api.example.com/users".to_string(),
            params: vec![ApiParam {
                name: "name".to_string(),
                description: "Name".to_string(),
                param_type: ParamType::String,
                required: true,
                location: ApiParamLocation::Body,
            }],
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            source_spec: "spec.json".to_string(),
        };

        let mut args = HashMap::new();
        args.insert("name".to_string(), serde_json::json!("Alice"));

        let preview = build_api_preview(&tool, &args);
        assert!(preview.contains("POST"));
        assert!(preview.contains("Body:"));
        assert!(preview.contains("Alice"));
    }

    #[test]
    fn test_build_api_preview_with_auth() {
        let tool = ApiToolDefinition {
            name: "get_data".to_string(),
            description: "Get data".to_string(),
            method: "GET".to_string(),
            url_template: "https://api.example.com/data".to_string(),
            params: vec![],
            transport: TransportType::Sse,
            auth: Some(AuthConfig {
                auth_type: "bearer".to_string(),
                auth_env: "API_TOKEN".to_string(),
                auth_header: None,
            }),
            static_headers: vec![],
            source_spec: "spec.json".to_string(),
        };

        let args = HashMap::new();
        let preview = build_api_preview(&tool, &args);
        assert!(preview.contains("Authorization: Bearer"));
        assert!(preview.contains("$API_TOKEN"));
    }

    #[test]
    fn test_parse_args_for_cli_string() {
        let tool = make_tool("t", vec![make_param("input", ParamType::String)]);
        let args = parse_args_for_cli(&tool, r#"{"input": "file.csv"}"#).unwrap();
        assert_eq!(
            args.get("input"),
            Some(&ToolArgValue::String("file.csv".to_string()))
        );
    }

    #[test]
    fn test_parse_args_for_cli_boolean() {
        let tool = make_tool("t", vec![make_param("verbose", ParamType::Boolean)]);
        let args = parse_args_for_cli(&tool, r#"{"verbose": true}"#).unwrap();
        assert_eq!(args.get("verbose"), Some(&ToolArgValue::Boolean(true)));
    }

    #[test]
    fn test_parse_args_for_cli_integer() {
        let tool = make_tool("t", vec![make_param("count", ParamType::Integer)]);
        let args = parse_args_for_cli(&tool, r#"{"count": 42}"#).unwrap();
        assert_eq!(args.get("count"), Some(&ToolArgValue::Integer(42)));
    }

    #[test]
    fn test_parse_args_for_cli_invalid_json() {
        let tool = make_tool("t", vec![]);
        let result = parse_args_for_cli(&tool, "not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_cli_tool() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool(
                "my_tool",
                vec![make_param("input", ParamType::String)],
            ))
            .unwrap();

        // Should not panic
        run(&registry, None, "my_tool", r#"{"input": "test.csv"}"#).unwrap();
    }

    #[test]
    fn test_run_no_args() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("simple", vec![])).unwrap();

        run(&registry, None, "simple", "{}").unwrap();
    }
}
