use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::executor::command_executor::ProcessCommandExecutor;
use crate::mcp::server::McpServerState;
use crate::mcp::sse;
use crate::mcp::stdio;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::models::TransportType;
use crate::registry::store::ToolRegistry;

pub async fn run(
    registry: Arc<dyn ToolRegistry>,
    host: &str,
    port: u16,
    api_tools_path: PathBuf,
    progressive: bool,
) -> Result<()> {
    let all_tools = registry.list()?;
    let stdio_count = all_tools
        .iter()
        .filter(|t| t.transport == TransportType::Stdio)
        .count();
    let sse_count = all_tools
        .iter()
        .filter(|t| t.transport == TransportType::Sse)
        .count();

    // Load API tools
    let api_registry = Arc::new(ApiToolRegistry::new(api_tools_path)?);
    let api_tools = api_registry.list()?;
    let api_stdio_count = api_tools
        .iter()
        .filter(|t| t.transport == TransportType::Stdio)
        .count();
    let api_sse_count = api_tools
        .iter()
        .filter(|t| t.transport == TransportType::Sse)
        .count();

    let executor: Arc<dyn crate::executor::command_executor::CommandExecutor> =
        Arc::new(ProcessCommandExecutor::new());

    eprintln!("MCP server starting (PID {})", std::process::id());
    eprintln!(
        "  STDIO: {} tools ({} CLI + {} API) on stdin/stdout",
        stdio_count + api_stdio_count,
        stdio_count,
        api_stdio_count
    );
    eprintln!(
        "  SSE:   {} tools ({} CLI + {} API) on http://{}:{}",
        sse_count + api_sse_count,
        sse_count,
        api_sse_count,
        host,
        port
    );

    if progressive {
        eprintln!("  Mode: progressive disclosure (3 meta-tools)");
    }

    crate::logger::server_start(
        std::process::id(),
        stdio_count + api_stdio_count,
        sse_count + api_sse_count,
        progressive,
    );

    // Create separate server states for each transport
    let mut stdio_state = McpServerState::with_transport(
        Arc::clone(&registry),
        Arc::clone(&executor),
        TransportType::Stdio,
    );
    stdio_state.set_api_registry(Arc::clone(&api_registry));
    stdio_state.set_progressive(progressive);
    let stdio_state = Arc::new(stdio_state);

    let mut sse_state = McpServerState::with_transport(
        Arc::clone(&registry),
        Arc::clone(&executor),
        TransportType::Sse,
    );
    sse_state.set_api_registry(Arc::clone(&api_registry));
    sse_state.set_progressive(progressive);
    let sse_state = Arc::new(sse_state);

    let sse_host = host.to_string();

    let sse_handle = tokio::spawn(async move {
        if let Err(e) = sse::run_sse(sse_state, &sse_host, port).await {
            eprintln!("SSE transport error: {}", e);
        }
    });

    let stdio_handle = tokio::task::spawn_blocking(move || {
        if let Err(e) = stdio::run_stdio(stdio_state) {
            eprintln!("STDIO transport error: {}", e);
        }
        eprintln!("STDIO transport closed (stdin ended)");
    });

    let _ = tokio::join!(sse_handle, stdio_handle);

    Ok(())
}
