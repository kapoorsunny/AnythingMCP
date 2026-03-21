use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::{McpWrapError, Result};
use crate::executor::command_executor::CommandExecutor;
use crate::mcp::schema::{api_tool_to_mcp_schema, tool_to_mcp_schema};
use crate::openapi::models::ApiToolDefinition;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::models::{ParamType, ToolArgValue, ToolDefinition, TransportType};
use crate::registry::store::ToolRegistry;

pub struct McpServerState {
    pub registry: Arc<dyn ToolRegistry>,
    pub executor: Arc<dyn CommandExecutor>,
    pub api_registry: Option<Arc<ApiToolRegistry>>,
    pub transport_filter: Option<TransportType>,
    pub progressive: bool,
}

impl McpServerState {
    pub fn new(registry: Arc<dyn ToolRegistry>, executor: Arc<dyn CommandExecutor>) -> Self {
        Self {
            registry,
            executor,
            api_registry: None,
            transport_filter: None,
            progressive: false,
        }
    }

    /// Create a server state that only serves tools matching the given transport.
    pub fn with_transport(
        registry: Arc<dyn ToolRegistry>,
        executor: Arc<dyn CommandExecutor>,
        transport: TransportType,
    ) -> Self {
        Self {
            registry,
            executor,
            api_registry: None,
            transport_filter: Some(transport),
            progressive: false,
        }
    }

    pub fn set_api_registry(&mut self, api_registry: Arc<ApiToolRegistry>) {
        self.api_registry = Some(api_registry);
    }

    pub fn set_progressive(&mut self, enabled: bool) {
        self.progressive = enabled;
    }

    fn filtered_tools(&self) -> Result<Vec<ToolDefinition>> {
        let all_tools = self.registry.list()?;
        match &self.transport_filter {
            Some(filter) => Ok(all_tools
                .into_iter()
                .filter(|t| t.transport == *filter)
                .collect()),
            None => Ok(all_tools),
        }
    }

    fn filtered_api_tools(&self) -> Result<Vec<ApiToolDefinition>> {
        let all_tools = match &self.api_registry {
            Some(r) => r.list()?,
            None => Vec::new(),
        };
        match &self.transport_filter {
            Some(filter) => Ok(all_tools
                .into_iter()
                .filter(|t| t.transport == *filter)
                .collect()),
            None => Ok(all_tools),
        }
    }

    /// Handle a JSON-RPC request asynchronously.
    /// Tool calls are offloaded to a blocking thread pool so they don't
    /// block the async runtime — allowing concurrent tool executions in SSE mode.
    pub async fn handle_request_async(self: &Arc<Self>, request: &Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        match method.as_str() {
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                let tool_name = params
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();

                // In progressive mode, handle meta-tools synchronously (no subprocess)
                if self.progressive {
                    match tool_name.as_str() {
                        "search_tools" | "get_tool_schema" => {
                            return self.handle_request(request);
                        }
                        "call_tool" => {
                            // Unwrap nested call — determine if real tool is API or CLI
                            let real_name = params
                                .get("arguments")
                                .and_then(|a| a.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            let real_args = params
                                .get("arguments")
                                .and_then(|a| a.get("arguments"))
                                .cloned()
                                .unwrap_or(json!({}));
                            let rewritten = json!({
                                "name": real_name,
                                "arguments": real_args
                            });

                            let is_api = self
                                .api_registry
                                .as_ref()
                                .and_then(|r| r.get(&real_name).ok())
                                .flatten()
                                .is_some();

                            if is_api {
                                return self.handle_call_api_tool(id, &rewritten).await;
                            } else {
                                let state = Arc::clone(self);
                                let id_for_err = id.clone();
                                return tokio::task::spawn_blocking(move || {
                                    state.handle_call_tool(id, &rewritten)
                                })
                                .await
                                .unwrap_or_else(|e| {
                                    json!({
                                        "jsonrpc": "2.0",
                                        "id": id_for_err,
                                        "error": {
                                            "code": -32603,
                                            "message": format!("Internal error: {}", e)
                                        }
                                    })
                                });
                            }
                        }
                        _ => {} // Fall through to normal handling
                    }
                }

                // Check if this is an API tool (needs async HTTP execution)
                let is_api_tool = self
                    .api_registry
                    .as_ref()
                    .and_then(|r| r.get(&tool_name).ok())
                    .flatten()
                    .is_some();

                if is_api_tool {
                    self.handle_call_api_tool(id, &params).await
                } else {
                    let state = Arc::clone(self);
                    let id_for_err = id.clone();
                    tokio::task::spawn_blocking(move || state.handle_call_tool(id, &params))
                        .await
                        .unwrap_or_else(|e| {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id_for_err,
                                "error": {
                                    "code": -32603,
                                    "message": format!("Internal error: {}", e)
                                }
                            })
                        })
                }
            }
            _ => self.handle_request(request),
        }
    }

    /// Handle a JSON-RPC request synchronously (used by stdio transport and tests).
    pub fn handle_request(&self, request: &Value) -> Value {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        match method {
            "initialize" => self.handle_initialize(id),
            "initialized" => {
                // Notification, no response needed
                Value::Null
            }
            "tools/list" => self.handle_list_tools(id),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");

                // In progressive mode, intercept meta-tool calls
                if self.progressive {
                    match tool_name {
                        "search_tools" => {
                            let query = params
                                .get("arguments")
                                .and_then(|a| a.get("query"))
                                .and_then(|q| q.as_str())
                                .unwrap_or("");
                            return json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": self.handle_search_tools(query)
                            });
                        }
                        "get_tool_schema" => {
                            let name = params
                                .get("arguments")
                                .and_then(|a| a.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            return json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": self.handle_get_tool_schema(name)
                            });
                        }
                        "call_tool" => {
                            // Unwrap the nested call: extract real name + arguments
                            let real_name = params
                                .get("arguments")
                                .and_then(|a| a.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");
                            let real_args = params
                                .get("arguments")
                                .and_then(|a| a.get("arguments"))
                                .cloned()
                                .unwrap_or(json!({}));
                            let rewritten = json!({
                                "name": real_name,
                                "arguments": real_args
                            });
                            return self.handle_call_tool(id, &rewritten);
                        }
                        _ => {
                            // Not a meta-tool — fall through to normal handling
                        }
                    }
                }

                self.handle_call_tool(id, &params)
            }
            "ping" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        }
    }

    fn handle_initialize(&self, id: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "mcpw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        })
    }

    /// Return the 3 meta-tools for progressive disclosure mode.
    fn progressive_meta_tools(&self) -> Vec<Value> {
        vec![
            json!({
                "name": "search_tools",
                "description": "Search available tools by keyword. Returns matching tool names, types, and descriptions. Use this first to find the right tool before calling it.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search keyword to match against tool names and descriptions"
                        }
                    },
                    "required": ["query"]
                }
            }),
            json!({
                "name": "get_tool_schema",
                "description": "Get the full parameter schema for a specific tool. Call this after search_tools to see what arguments a tool accepts before executing it.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Exact tool name (from search_tools results)"
                        }
                    },
                    "required": ["name"]
                }
            }),
            json!({
                "name": "call_tool",
                "description": "Execute a tool with the given arguments. Use search_tools to find a tool, get_tool_schema to see its parameters, then call_tool to execute it.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Exact tool name to execute"
                        },
                        "arguments": {
                            "type": "object",
                            "description": "Arguments to pass to the tool (see get_tool_schema for available parameters)"
                        }
                    },
                    "required": ["name"]
                }
            }),
        ]
    }

    /// Handle search_tools meta-tool: fuzzy search across all tool names and descriptions.
    fn handle_search_tools(&self, query: &str) -> Value {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        if let Ok(tools) = self.filtered_tools() {
            for tool in &tools {
                if tool.name.to_lowercase().contains(&query_lower)
                    || tool.description.to_lowercase().contains(&query_lower)
                {
                    results.push(json!({
                        "name": tool.name,
                        "type": format!("CLI ({})", tool.transport),
                        "description": tool.description,
                        "params_count": tool.params.len()
                    }));
                }
            }
        }

        if let Ok(api_tools) = self.filtered_api_tools() {
            for tool in &api_tools {
                if tool.name.to_lowercase().contains(&query_lower)
                    || tool.description.to_lowercase().contains(&query_lower)
                {
                    results.push(json!({
                        "name": tool.name,
                        "type": format!("API {} ({})", tool.method, tool.transport),
                        "description": tool.description,
                        "params_count": tool.params.len()
                    }));
                }
            }
        }

        let text = if results.is_empty() {
            format!(
                "No tools found matching '{}'. Try a broader search term.",
                query
            )
        } else {
            serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())
        };

        json!({
            "content": [{"type": "text", "text": text}],
            "isError": false
        })
    }

    /// Handle get_tool_schema meta-tool: return the full MCP schema for a specific tool.
    fn handle_get_tool_schema(&self, name: &str) -> Value {
        // Check CLI tools
        if let Ok(Some(tool)) = self.registry.get(name) {
            if let Some(ref filter) = self.transport_filter {
                if tool.transport != *filter {
                    return json!({
                        "content": [{"type": "text", "text": format!("Tool not found: {}", name)}],
                        "isError": true
                    });
                }
            }
            let schema = tool_to_mcp_schema(&tool);
            return json!({
                "content": [{"type": "text", "text": serde_json::to_string_pretty(&schema).unwrap_or_default()}],
                "isError": false
            });
        }

        // Check API tools
        if let Some(ref api_reg) = self.api_registry {
            if let Ok(Some(tool)) = api_reg.get(name) {
                if let Some(ref filter) = self.transport_filter {
                    if tool.transport != *filter {
                        return json!({
                            "content": [{"type": "text", "text": format!("Tool not found: {}", name)}],
                            "isError": true
                        });
                    }
                }
                let schema = api_tool_to_mcp_schema(&tool);
                return json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&schema).unwrap_or_default()}],
                    "isError": false
                });
            }
        }

        json!({
            "content": [{"type": "text", "text": format!("Tool not found: {}", name)}],
            "isError": true
        })
    }

    fn handle_list_tools(&self, id: Value) -> Value {
        // Progressive mode: return only the 3 meta-tools
        if self.progressive {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": self.progressive_meta_tools()
                }
            });
        }

        // Standard mode: return all tool schemas
        match self.filtered_tools() {
            Ok(tools) => {
                let mut tool_schemas: Vec<Value> = tools.iter().map(tool_to_mcp_schema).collect();

                if let Ok(api_tools) = self.filtered_api_tools() {
                    for api_tool in &api_tools {
                        tool_schemas.push(api_tool_to_mcp_schema(api_tool));
                    }
                }

                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": tool_schemas
                    }
                })
            }
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32603,
                    "message": format!("Internal error: {}", e)
                }
            }),
        }
    }

    fn handle_call_tool(&self, id: Value, params: &Value) -> Value {
        let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let start = std::time::Instant::now();

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        // Look up tool definition, respecting transport filter
        let tool = match self.registry.get(tool_name) {
            Ok(Some(t)) => {
                // Enforce transport filter: tool must match this transport
                if let Some(ref filter) = self.transport_filter {
                    if t.transport != *filter {
                        return json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": format!("Tool not found: {}", tool_name)
                            }
                        });
                    }
                }
                t
            }
            Ok(None) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": format!("Tool not found: {}", tool_name)
                    }
                });
            }
            Err(e) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32603,
                        "message": format!("Registry error: {}", e)
                    }
                });
            }
        };

        // Convert JSON arguments to ToolArgValue using param types
        let args = match convert_args(&tool, &arguments) {
            Ok(a) => a,
            Err(e) => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": format!("Invalid parameters: {}", e)
                    }
                });
            }
        };

        // Execute the command
        let response = match self.executor.execute(&tool.command, &args) {
            Ok(result) => {
                let elapsed = start.elapsed().as_millis();
                if result.exit_code == 0 {
                    crate::logger::call_ok(tool_name, elapsed);
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": result.stdout
                            }],
                            "isError": false
                        }
                    })
                } else {
                    crate::logger::call_err(
                        tool_name,
                        elapsed,
                        result.stderr.lines().next().unwrap_or(""),
                    );
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": result.stderr
                            }],
                            "isError": true
                        }
                    })
                }
            }
            Err(e) => {
                crate::logger::call_err(tool_name, start.elapsed().as_millis(), &e.to_string());
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [{
                            "type": "text",
                            "text": format!("Execution error: {}", e)
                        }],
                        "isError": true
                    }
                })
            }
        };
        response
    }

    /// Handle a call to an API tool (imported from OpenAPI spec)
    async fn handle_call_api_tool(&self, id: Value, params: &Value) -> Value {
        let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let api_registry = match &self.api_registry {
            Some(r) => r,
            None => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": format!("Tool not found: {}", tool_name)
                    }
                });
            }
        };

        let tool = match api_registry.get(tool_name) {
            Ok(Some(t)) => {
                if let Some(ref filter) = self.transport_filter {
                    if t.transport != *filter {
                        return json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32602,
                                "message": format!("Tool not found: {}", tool_name)
                            }
                        });
                    }
                }
                t
            }
            _ => {
                return json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32602,
                        "message": format!("Tool not found: {}", tool_name)
                    }
                });
            }
        };

        // Convert args using API param types
        let args = convert_api_args(&arguments);

        match crate::openapi::executor::execute_api_tool(&tool, &args).await {
            Ok(body) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{"type": "text", "text": body}],
                    "isError": false
                }
            }),
            Err(McpWrapError::ExecutionFailed { stderr, .. }) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{"type": "text", "text": stderr}],
                    "isError": true
                }
            }),
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{"type": "text", "text": format!("{}", e)}],
                    "isError": true
                }
            }),
        }
    }
}

/// Convert API tool arguments from JSON to ToolArgValue
fn convert_api_args(arguments: &Value) -> HashMap<String, ToolArgValue> {
    let mut args = HashMap::new();
    if let Some(obj) = arguments.as_object() {
        for (key, value) in obj {
            let arg = match value {
                Value::String(s) => ToolArgValue::String(s.clone()),
                Value::Bool(b) => ToolArgValue::Boolean(*b),
                Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        ToolArgValue::Integer(i)
                    } else if let Some(f) = n.as_f64() {
                        ToolArgValue::Float(f)
                    } else {
                        ToolArgValue::String(n.to_string())
                    }
                }
                _ => ToolArgValue::String(value.to_string()),
            };
            args.insert(key.clone(), arg);
        }
    }
    args
}

/// Convert MCP JSON arguments to typed ToolArgValue map using the tool's param definitions.
fn convert_args(tool: &ToolDefinition, arguments: &Value) -> Result<HashMap<String, ToolArgValue>> {
    let mut args = HashMap::new();

    if let Some(obj) = arguments.as_object() {
        for (key, value) in obj {
            // Find the param definition for type information
            let param = tool.params.iter().find(|p| p.name == *key);

            let arg_value = if let Some(param) = param {
                match param.param_type {
                    ParamType::Boolean => match value {
                        Value::Bool(b) => ToolArgValue::Boolean(*b),
                        _ => {
                            return Err(McpWrapError::InvalidArgType {
                                param: key.clone(),
                                expected: "boolean".to_string(),
                            })
                        }
                    },
                    ParamType::Integer => match value {
                        Value::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                ToolArgValue::Integer(i)
                            } else {
                                return Err(McpWrapError::InvalidArgType {
                                    param: key.clone(),
                                    expected: "integer".to_string(),
                                });
                            }
                        }
                        _ => {
                            return Err(McpWrapError::InvalidArgType {
                                param: key.clone(),
                                expected: "integer".to_string(),
                            })
                        }
                    },
                    ParamType::Float => match value {
                        Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                ToolArgValue::Float(f)
                            } else {
                                return Err(McpWrapError::InvalidArgType {
                                    param: key.clone(),
                                    expected: "float".to_string(),
                                });
                            }
                        }
                        _ => {
                            return Err(McpWrapError::InvalidArgType {
                                param: key.clone(),
                                expected: "float".to_string(),
                            })
                        }
                    },
                    ParamType::String => match value {
                        Value::String(s) => ToolArgValue::String(s.clone()),
                        _ => {
                            return Err(McpWrapError::InvalidArgType {
                                param: key.clone(),
                                expected: "string".to_string(),
                            })
                        }
                    },
                }
            } else {
                // Reject unknown params — prevents arbitrary flag injection
                return Err(McpWrapError::InvalidArgType {
                    param: key.clone(),
                    expected: "unknown parameter (not in tool schema)".to_string(),
                });
            };

            args.insert(key.clone(), arg_value);
        }
    }

    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::command_executor::ExecutionResult;
    use crate::registry::models::ToolParam;
    use chrono::Utc;
    use std::sync::Mutex;

    struct MockRegistry {
        tools: Mutex<Vec<ToolDefinition>>,
    }

    impl MockRegistry {
        fn new(tools: Vec<ToolDefinition>) -> Self {
            Self {
                tools: Mutex::new(tools),
            }
        }
    }

    impl ToolRegistry for MockRegistry {
        fn add(&self, tool: ToolDefinition) -> Result<()> {
            self.tools.lock().unwrap().push(tool);
            Ok(())
        }
        fn remove(&self, name: &str) -> Result<()> {
            let mut tools = self.tools.lock().unwrap();
            tools.retain(|t| t.name != name);
            Ok(())
        }
        fn get(&self, name: &str) -> Result<Option<ToolDefinition>> {
            let tools = self.tools.lock().unwrap();
            Ok(tools.iter().find(|t| t.name == name).cloned())
        }
        fn list(&self) -> Result<Vec<ToolDefinition>> {
            Ok(self.tools.lock().unwrap().clone())
        }
    }

    struct MockExecutor {
        result: ExecutionResult,
    }

    impl MockExecutor {
        fn success(stdout: &str) -> Self {
            Self {
                result: ExecutionResult {
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                    exit_code: 0,
                },
            }
        }

        fn failure(stderr: &str) -> Self {
            Self {
                result: ExecutionResult {
                    stdout: String::new(),
                    stderr: stderr.to_string(),
                    exit_code: 1,
                },
            }
        }
    }

    impl CommandExecutor for MockExecutor {
        fn execute(
            &self,
            _command: &str,
            _args: &HashMap<String, ToolArgValue>,
        ) -> Result<ExecutionResult> {
            Ok(ExecutionResult {
                stdout: self.result.stdout.clone(),
                stderr: self.result.stderr.clone(),
                exit_code: self.result.exit_code,
            })
        }
    }

    fn make_tool() -> ToolDefinition {
        ToolDefinition {
            name: "test_tool".to_string(),
            command: "echo hello".to_string(),
            description: "A test tool".to_string(),
            params: vec![
                ToolParam {
                    name: "message".to_string(),
                    description: "Message to echo".to_string(),
                    param_type: ParamType::String,
                    required: true,
                    default_value: None,
                },
                ToolParam {
                    name: "count".to_string(),
                    description: "Number of times".to_string(),
                    param_type: ParamType::Integer,
                    required: false,
                    default_value: Some("1".to_string()),
                },
                ToolParam {
                    name: "verbose".to_string(),
                    description: "Verbose mode".to_string(),
                    param_type: ParamType::Boolean,
                    required: false,
                    default_value: None,
                },
            ],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_handle_initialize() {
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let response = state.handle_request(&request);
        assert_eq!(response["result"]["serverInfo"]["name"], "mcpw");
        assert_eq!(response["result"]["serverInfo"]["version"], "1.0.0");
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn test_handle_list_tools() {
        let tool = make_tool();
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");
        assert_eq!(
            tools[0]["inputSchema"]["properties"]["message"]["type"],
            "string"
        );
    }

    #[test]
    fn test_handle_call_tool_success() {
        let tool = make_tool();
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("hello world")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "test_tool",
                "arguments": {
                    "message": "world"
                }
            }
        });

        let response = state.handle_request(&request);
        assert_eq!(response["result"]["content"][0]["text"], "hello world");
        assert_eq!(response["result"]["isError"], false);
    }

    #[test]
    fn test_handle_call_tool_failure() {
        let tool = make_tool();
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::failure("fatal error")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "test_tool",
                "arguments": {
                    "message": "world"
                }
            }
        });

        let response = state.handle_request(&request);
        assert_eq!(response["result"]["content"][0]["text"], "fatal error");
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn test_handle_call_tool_not_found() {
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "nonexistent",
                "arguments": {}
            }
        });

        let response = state.handle_request(&request);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_handle_ping() {
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "ping"
        });

        let response = state.handle_request(&request);
        assert!(response["result"].is_object());
    }

    #[test]
    fn test_handle_unknown_method() {
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "unknown/method"
        });

        let response = state.handle_request(&request);
        assert_eq!(response["error"]["code"], -32601);
    }

    #[test]
    fn test_convert_args_typed() {
        let tool = make_tool();
        let arguments = json!({
            "message": "hello",
            "count": 5,
            "verbose": true
        });

        let args = convert_args(&tool, &arguments).unwrap();
        assert_eq!(args["message"], ToolArgValue::String("hello".to_string()));
        assert_eq!(args["count"], ToolArgValue::Integer(5));
        assert_eq!(args["verbose"], ToolArgValue::Boolean(true));
    }

    #[test]
    fn test_convert_args_type_mismatch() {
        let tool = make_tool();
        let arguments = json!({
            "message": 123  // should be string
        });

        let result = convert_args(&tool, &arguments);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_args_unknown_param_rejected() {
        let tool = make_tool();
        let arguments = json!({
            "message": "hello",
            "unknown_flag": "malicious"
        });

        let result = convert_args(&tool, &arguments);
        assert!(result.is_err());
        match result.unwrap_err() {
            McpWrapError::InvalidArgType { param, .. } => {
                assert_eq!(param, "unknown_flag");
            }
            e => panic!("Expected InvalidArgType, got: {:?}", e),
        }
    }

    #[test]
    fn test_convert_args_integer_type_mismatch() {
        let tool = make_tool();
        let arguments = json!({
            "count": "not_a_number"
        });
        let result = convert_args(&tool, &arguments);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_args_boolean_type_mismatch() {
        let tool = make_tool();
        let arguments = json!({
            "verbose": 42
        });
        let result = convert_args(&tool, &arguments);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_args_empty() {
        let tool = make_tool();
        let arguments = json!({});
        let args = convert_args(&tool, &arguments).unwrap();
        assert!(args.is_empty());
    }

    #[test]
    fn test_transport_filter_list_stdio_only() {
        let mut stdio_tool = make_tool();
        stdio_tool.name = "stdio_tool".to_string();
        stdio_tool.transport = TransportType::Stdio;

        let mut sse_tool = make_tool();
        sse_tool.name = "sse_tool".to_string();
        sse_tool.transport = TransportType::Sse;

        let state = McpServerState::with_transport(
            Arc::new(MockRegistry::new(vec![stdio_tool, sse_tool])),
            Arc::new(MockExecutor::success("")),
            TransportType::Stdio,
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        });

        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "stdio_tool");
    }

    #[test]
    fn test_transport_filter_list_sse_only() {
        let mut stdio_tool = make_tool();
        stdio_tool.name = "stdio_tool".to_string();
        stdio_tool.transport = TransportType::Stdio;

        let mut sse_tool = make_tool();
        sse_tool.name = "sse_tool".to_string();
        sse_tool.transport = TransportType::Sse;

        let state = McpServerState::with_transport(
            Arc::new(MockRegistry::new(vec![stdio_tool, sse_tool])),
            Arc::new(MockExecutor::success("")),
            TransportType::Sse,
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        });

        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "sse_tool");
    }

    #[test]
    fn test_transport_filter_call_rejected_on_wrong_transport() {
        let mut stdio_tool = make_tool();
        stdio_tool.name = "stdio_only".to_string();
        stdio_tool.transport = TransportType::Stdio;

        // SSE-filtered server tries to call a STDIO tool
        let state = McpServerState::with_transport(
            Arc::new(MockRegistry::new(vec![stdio_tool])),
            Arc::new(MockExecutor::success("should not reach")),
            TransportType::Sse,
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "stdio_only",
                "arguments": {}
            }
        });

        let response = state.handle_request(&request);
        // Should get "Tool not found" error, not execution
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"));
    }

    #[test]
    fn test_no_filter_shows_all_tools() {
        let mut stdio_tool = make_tool();
        stdio_tool.name = "stdio_tool".to_string();
        stdio_tool.transport = TransportType::Stdio;

        let mut sse_tool = make_tool();
        sse_tool.name = "sse_tool".to_string();
        sse_tool.transport = TransportType::Sse;

        // No transport filter — shows all
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![stdio_tool, sse_tool])),
            Arc::new(MockExecutor::success("")),
        );

        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        });

        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_progressive_list_returns_meta_tools() {
        let tool = make_tool();
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );
        state.set_progressive(true);

        let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();

        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0]["name"], "search_tools");
        assert_eq!(tools[1]["name"], "get_tool_schema");
        assert_eq!(tools[2]["name"], "call_tool");
    }

    #[test]
    fn test_progressive_search_tools() {
        let tool = make_tool();
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );
        state.set_progressive(true);

        let request = json!({
            "jsonrpc": "2.0", "id": 2,
            "method": "tools/call",
            "params": {
                "name": "search_tools",
                "arguments": {"query": "test"}
            }
        });
        let response = state.handle_request(&request);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();

        assert!(text.contains("test_tool"));
        assert!(text.contains("A test tool"));
    }

    #[test]
    fn test_progressive_search_no_results() {
        let tool = make_tool();
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );
        state.set_progressive(true);

        let request = json!({
            "jsonrpc": "2.0", "id": 3,
            "method": "tools/call",
            "params": {
                "name": "search_tools",
                "arguments": {"query": "nonexistent_xyz"}
            }
        });
        let response = state.handle_request(&request);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();

        assert!(text.contains("No tools found"));
    }

    #[test]
    fn test_progressive_get_tool_schema() {
        let tool = make_tool();
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );
        state.set_progressive(true);

        let request = json!({
            "jsonrpc": "2.0", "id": 4,
            "method": "tools/call",
            "params": {
                "name": "get_tool_schema",
                "arguments": {"name": "test_tool"}
            }
        });
        let response = state.handle_request(&request);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();

        assert!(text.contains("test_tool"));
        assert!(text.contains("inputSchema"));
        assert!(text.contains("message"));
    }

    #[test]
    fn test_progressive_get_tool_schema_not_found() {
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![])),
            Arc::new(MockExecutor::success("")),
        );
        state.set_progressive(true);

        let request = json!({
            "jsonrpc": "2.0", "id": 5,
            "method": "tools/call",
            "params": {
                "name": "get_tool_schema",
                "arguments": {"name": "nonexistent"}
            }
        });
        let response = state.handle_request(&request);
        assert_eq!(response["result"]["isError"], true);
    }

    #[test]
    fn test_progressive_call_tool() {
        let tool = make_tool();
        let mut state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("hello output")),
        );
        state.set_progressive(true);

        let request = json!({
            "jsonrpc": "2.0", "id": 6,
            "method": "tools/call",
            "params": {
                "name": "call_tool",
                "arguments": {
                    "name": "test_tool",
                    "arguments": {"message": "world"}
                }
            }
        });
        let response = state.handle_request(&request);
        assert_eq!(response["result"]["isError"], false);
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello output"));
    }

    #[test]
    fn test_progressive_disabled_returns_all_tools() {
        let tool = make_tool();
        let state = McpServerState::new(
            Arc::new(MockRegistry::new(vec![tool])),
            Arc::new(MockExecutor::success("")),
        );
        // progressive is false by default

        let request = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"});
        let response = state.handle_request(&request);
        let tools = response["result"]["tools"].as_array().unwrap();

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "test_tool");
    }
}
