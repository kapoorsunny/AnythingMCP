use std::sync::Arc;

use serde_json::json;

use crate::error::Result;
use crate::executor::command_executor::ProcessCommandExecutor;
use crate::mcp::schema::tool_to_mcp_schema;
use crate::mcp::server::McpServerState;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::store::ToolRegistry;

pub fn run(
    registry: Arc<dyn ToolRegistry>,
    api_registry: Option<Arc<ApiToolRegistry>>,
    name: &str,
    args_json: &str,
    progressive: bool,
) -> Result<()> {
    if progressive {
        return run_progressive(registry, api_registry, name, args_json);
    }

    // Standard mode: direct tool call
    let tool = match registry.get(name)? {
        Some(t) => t,
        None => {
            // Check API tools
            if let Some(ref api_reg) = api_registry {
                if let Ok(Some(_)) = api_reg.get(name) {
                    eprintln!("Error: '{}' is an API tool. Use --progressive to test API tools, or test via the server.", name);
                    std::process::exit(1);
                }
            }
            eprintln!("Error: tool '{}' not found.", name);
            std::process::exit(1);
        }
    };

    let schema = tool_to_mcp_schema(&tool);
    println!("--- Tool Schema ---");
    println!(
        "{}",
        serde_json::to_string_pretty(&schema).unwrap_or_else(|_| "{}".to_string())
    );
    println!();

    let arguments: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON arguments: {}", e);
            eprintln!("Expected format: '{{\"key\": \"value\"}}'");
            std::process::exit(1);
        }
    };

    println!("--- Request ---");
    println!("  Tool: {}", name);
    println!("  Args: {}", args_json);
    println!();

    let executor = Arc::new(ProcessCommandExecutor::new());
    let state = McpServerState::new(registry, executor);

    let mcp_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    });

    let response = state.handle_request(&mcp_request);
    print_result(&response);
    Ok(())
}

/// Progressive mode: simulates the 3-step flow an LLM would follow.
fn run_progressive(
    registry: Arc<dyn ToolRegistry>,
    api_registry: Option<Arc<ApiToolRegistry>>,
    name: &str,
    args_json: &str,
) -> Result<()> {
    let executor = Arc::new(ProcessCommandExecutor::new());
    let mut state = McpServerState::new(registry, executor);
    if let Some(api_reg) = api_registry {
        state.set_api_registry(api_reg);
    }
    state.set_progressive(true);

    // Step 1: search_tools
    println!("Step 1: search_tools(query=\"{}\")", name);
    let search_request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "search_tools",
            "arguments": {"query": name}
        }
    });
    let search_response = state.handle_request(&search_request);
    let search_text = extract_text(&search_response);

    // Parse results to check if tool was found
    let found: Vec<serde_json::Value> = serde_json::from_str(&search_text).unwrap_or_default();

    if found.is_empty() {
        println!("  No tools found matching '{}'.", name);
        std::process::exit(1);
    }

    for tool in &found {
        println!(
            "  Found: {} — {} ({})",
            tool["name"].as_str().unwrap_or(""),
            tool["description"].as_str().unwrap_or(""),
            tool["type"].as_str().unwrap_or("")
        );
    }

    // Use the first match (or exact match if available)
    let tool_name = found
        .iter()
        .find(|t| t["name"].as_str() == Some(name))
        .or_else(|| found.first())
        .and_then(|t| t["name"].as_str())
        .unwrap_or(name);

    println!();

    // Step 2: get_tool_schema
    println!("Step 2: get_tool_schema(name=\"{}\")", tool_name);
    let schema_request = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "get_tool_schema",
            "arguments": {"name": tool_name}
        }
    });
    let schema_response = state.handle_request(&schema_request);
    let schema_text = extract_text(&schema_response);

    if let Ok(schema) = serde_json::from_str::<serde_json::Value>(&schema_text) {
        if let Some(props) = schema["inputSchema"]["properties"].as_object() {
            if props.is_empty() {
                println!("  Params: (none)");
            } else {
                println!("  Params:");
                for (pname, pval) in props {
                    println!(
                        "    --{} <{}> {}",
                        pname,
                        pval["type"].as_str().unwrap_or("string"),
                        pval["description"].as_str().unwrap_or("")
                    );
                }
            }
        }
    } else {
        println!("  {}", schema_text);
    }
    println!();

    // Step 3: call_tool
    let arguments: serde_json::Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: invalid JSON arguments: {}", e);
            std::process::exit(1);
        }
    };

    println!(
        "Step 3: call_tool(name=\"{}\", arguments={})",
        tool_name, args_json
    );
    let call_request = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "call_tool",
            "arguments": {
                "name": tool_name,
                "arguments": arguments
            }
        }
    });
    let call_response = state.handle_request(&call_request);
    println!();
    print_result(&call_response);

    Ok(())
}

fn extract_text(response: &serde_json::Value) -> String {
    response["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .unwrap_or("")
        .to_string()
}

fn print_result(response: &serde_json::Value) {
    let result = &response["result"];
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let content_text = extract_text(response);

    if is_error {
        println!("--- Result: ERROR ---");
        eprintln!("{}", content_text);
        std::process::exit(1);
    } else {
        println!("--- Result: OK ---");
        print!("{}", content_text);
        if !content_text.ends_with('\n') {
            println!();
        }
    }
}
