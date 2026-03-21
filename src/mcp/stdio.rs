use std::io::{self, BufRead, Write};
use std::sync::Arc;

use crate::mcp::server::McpServerState;

/// Run the MCP server over stdio (JSON-RPC over stdin/stdout).
/// This is a blocking function — each line on stdin is a JSON-RPC request;
/// each response is written as a line on stdout.
pub fn run_stdio(state: Arc<McpServerState>) -> crate::error::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = stdin.lock();
    let mut writer = stdout.lock();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim().to_string();

        if line.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let error_response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {}", e)
                    }
                });
                writeln!(writer, "{}", serde_json::to_string(&error_response)?)?;
                writer.flush()?;
                continue;
            }
        };

        let response = state.handle_request(&request);

        // Don't send response for notifications (no id)
        if response.is_null() {
            continue;
        }

        writeln!(writer, "{}", serde_json::to_string(&response)?)?;
        writer.flush()?;
    }

    Ok(())
}
