use serde_json::{json, Value};

use crate::openapi::models::ApiToolDefinition;
use crate::registry::models::{ParamType, ToolDefinition};

/// Convert a ToolDefinition into an MCP tool schema (JSON Schema format).
pub fn tool_to_mcp_schema(tool: &ToolDefinition) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in &tool.params {
        let json_type = match param.param_type {
            ParamType::String => "string",
            ParamType::Integer => "integer",
            ParamType::Float => "number",
            ParamType::Boolean => "boolean",
        };

        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), json!(json_type));
        prop.insert("description".to_string(), json!(param.description));

        if let Some(ref default) = param.default_value {
            prop.insert("default".to_string(), json!(default));
        }

        properties.insert(param.name.clone(), Value::Object(prop));

        if param.required {
            required.push(json!(param.name));
        }
    }

    json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}

/// Convert an ApiToolDefinition into an MCP tool schema.
pub fn api_tool_to_mcp_schema(tool: &ApiToolDefinition) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in &tool.params {
        let json_type = match param.param_type {
            ParamType::String => "string",
            ParamType::Integer => "integer",
            ParamType::Float => "number",
            ParamType::Boolean => "boolean",
        };

        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), json!(json_type));
        prop.insert("description".to_string(), json!(param.description));

        properties.insert(param.name.clone(), Value::Object(prop));

        if param.required {
            required.push(json!(param.name));
        }
    }

    json!({
        "name": tool.name,
        "description": tool.description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{ToolParam, TransportType};
    use chrono::Utc;

    #[test]
    fn test_tool_to_mcp_schema_basic() {
        let tool = ToolDefinition {
            name: "test_tool".to_string(),
            command: "echo".to_string(),
            description: "A test tool".to_string(),
            params: vec![
                ToolParam {
                    name: "input".to_string(),
                    description: "Input file".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    default_value: None,
                },
                ToolParam {
                    name: "width".to_string(),
                    description: "Width in pixels".to_string(),
                    param_type: ParamType::Integer,
                    required: false,
                    default_value: Some("800".to_string()),
                },
            ],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        };

        let schema = tool_to_mcp_schema(&tool);
        assert_eq!(schema["name"], "test_tool");
        assert_eq!(schema["description"], "A test tool");
        assert_eq!(schema["inputSchema"]["type"], "object");
        assert_eq!(
            schema["inputSchema"]["properties"]["input"]["type"],
            "string"
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["width"]["type"],
            "integer"
        );
        assert_eq!(
            schema["inputSchema"]["properties"]["width"]["default"],
            "800"
        );
        assert_eq!(schema["inputSchema"]["required"], json!(["input"]));
    }

    #[test]
    fn test_tool_to_mcp_schema_empty_params() {
        let tool = ToolDefinition {
            name: "simple".to_string(),
            command: "ls".to_string(),
            description: "List files".to_string(),
            params: vec![],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        };

        let schema = tool_to_mcp_schema(&tool);
        assert_eq!(schema["inputSchema"]["properties"], json!({}));
        assert_eq!(schema["inputSchema"]["required"], json!([]));
    }

    #[test]
    fn test_tool_to_mcp_schema_all_types() {
        let tool = ToolDefinition {
            name: "typed".to_string(),
            command: "cmd".to_string(),
            description: "Typed tool".to_string(),
            params: vec![
                ToolParam {
                    name: "s".to_string(),
                    description: "string".to_string(),
                    param_type: ParamType::String,
                    required: false,
                    default_value: None,
                },
                ToolParam {
                    name: "i".to_string(),
                    description: "integer".to_string(),
                    param_type: ParamType::Integer,
                    required: false,
                    default_value: None,
                },
                ToolParam {
                    name: "f".to_string(),
                    description: "float".to_string(),
                    param_type: ParamType::Float,
                    required: false,
                    default_value: None,
                },
                ToolParam {
                    name: "b".to_string(),
                    description: "boolean".to_string(),
                    param_type: ParamType::Boolean,
                    required: false,
                    default_value: None,
                },
            ],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        };

        let schema = tool_to_mcp_schema(&tool);
        assert_eq!(schema["inputSchema"]["properties"]["s"]["type"], "string");
        assert_eq!(schema["inputSchema"]["properties"]["i"]["type"], "integer");
        assert_eq!(schema["inputSchema"]["properties"]["f"]["type"], "number");
        assert_eq!(schema["inputSchema"]["properties"]["b"]["type"], "boolean");
    }
}
