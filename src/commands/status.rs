use std::path::Path;

use crate::error::Result;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::store::ToolRegistry;

/// Server status information
#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub cli_tool_count: usize,
    pub api_tool_count: usize,
    pub sse_port: u16,
    pub log_file_exists: bool,
    pub last_log_line: Option<String>,
}

/// Check if mcpw serve is running by looking for the process
fn find_mcpw_serve_pid() -> Option<u32> {
    // Use `pgrep` on Unix or check process list
    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("pgrep")
            .args(["-f", "mcpw serve"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let current_pid = std::process::id();
            // Return the first PID that isn't our own process
            for line in stdout.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != current_pid {
                        return Some(pid);
                    }
                }
            }
        }
        None
    }

    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq mcpw.exe", "/FO", "CSV", "/NH"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("mcpw.exe") {
                // Parse CSV: "mcpw.exe","PID",...
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let pid_str = parts[1].trim().trim_matches('"');
                        if let Ok(pid) = pid_str.parse::<u32>() {
                            return Some(pid);
                        }
                    }
                }
            }
        }
        None
    }
}

/// Read the last line of the log file
fn last_log_line(tools_path: &Path) -> Option<String> {
    let log_path = crate::logger::log_path(tools_path);
    let content = std::fs::read_to_string(log_path).ok()?;
    content.lines().last().map(|s| s.to_string())
}

/// Check if the SSE port is responding
fn check_sse_port(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}

/// Gather server status
pub fn gather_status(
    tools_path: &Path,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
    port: u16,
) -> Result<ServerStatus> {
    let cli_tool_count = registry.list()?.len();
    let api_tool_count = api_registry
        .map(|r| r.list())
        .transpose()?
        .map(|t| t.len())
        .unwrap_or(0);

    let pid = find_mcpw_serve_pid();
    let log_path = crate::logger::log_path(tools_path);
    let log_exists = log_path.exists();
    let last_line = last_log_line(tools_path);

    Ok(ServerStatus {
        running: pid.is_some(),
        pid,
        cli_tool_count,
        api_tool_count,
        sse_port: port,
        log_file_exists: log_exists,
        last_log_line: last_line,
    })
}

/// Run the status command
pub fn run(
    tools_path: &Path,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
    port: u16,
    json_output: bool,
) -> Result<i32> {
    let status = gather_status(tools_path, registry, api_registry, port)?;

    if json_output {
        print_json(&status);
    } else {
        print_human(&status);
    }

    if status.running {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn print_human(status: &ServerStatus) {
    println!("mcpw status");
    println!("{}", "\u{2500}".repeat(40));

    if status.running {
        println!("  Server:     RUNNING (PID {})", status.pid.unwrap_or(0));
    } else {
        println!("  Server:     NOT RUNNING");
    }

    println!("  CLI tools:  {}", status.cli_tool_count);
    println!("  API tools:  {}", status.api_tool_count);
    println!("  SSE port:   {}", status.sse_port);

    let sse_status = if status.running && check_sse_port(status.sse_port) {
        "responding"
    } else if status.running {
        "not responding"
    } else {
        "n/a"
    };
    println!("  SSE status: {}", sse_status);

    println!(
        "  Log file:   {}",
        if status.log_file_exists {
            "exists"
        } else {
            "not found"
        }
    );

    if let Some(ref line) = status.last_log_line {
        let display = if line.len() > 70 {
            format!("{}...", &line[..67])
        } else {
            line.clone()
        };
        println!("  Last log:   {}", display);
    }
}

fn print_json(status: &ServerStatus) {
    let json = serde_json::json!({
        "running": status.running,
        "pid": status.pid,
        "cli_tool_count": status.cli_tool_count,
        "api_tool_count": status.api_tool_count,
        "sse_port": status.sse_port,
        "log_file_exists": status.log_file_exists,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{ToolDefinition, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            command: format!("echo {}", name),
            description: format!("Test tool {}", name),
            params: vec![],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_gather_status_empty() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();

        let status = gather_status(&tools_path, &registry, None, 3000).unwrap();
        assert_eq!(status.cli_tool_count, 0);
        assert_eq!(status.api_tool_count, 0);
        assert_eq!(status.sse_port, 3000);
    }

    #[test]
    fn test_gather_status_with_tools() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("tool_a")).unwrap();
        registry.add(make_tool("tool_b")).unwrap();

        let status = gather_status(&tools_path, &registry, None, 8080).unwrap();
        assert_eq!(status.cli_tool_count, 2);
        assert_eq!(status.sse_port, 8080);
    }

    #[test]
    fn test_gather_status_log_exists() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(
            tmp.path().join("mcpw.log"),
            "2026-03-22 10:00:00 [INFO] Server started\n",
        )
        .unwrap();

        let registry = InMemoryRegistry::new();
        let status = gather_status(&tools_path, &registry, None, 3000).unwrap();
        assert!(status.log_file_exists);
        assert!(status.last_log_line.is_some());
        assert!(status.last_log_line.unwrap().contains("Server started"));
    }

    #[test]
    fn test_gather_status_no_log() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();

        let status = gather_status(&tools_path, &registry, None, 3000).unwrap();
        assert!(!status.log_file_exists);
        assert!(status.last_log_line.is_none());
    }

    #[test]
    fn test_run_not_running_returns_1() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();

        // Server is not running, so should return exit code 1
        let exit_code = run(&tools_path, &registry, None, 3000, false).unwrap();
        assert_eq!(exit_code, 1);
    }

    #[test]
    fn test_run_json_output() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();

        // Should not panic with JSON output
        let exit_code = run(&tools_path, &registry, None, 3000, true).unwrap();
        assert_eq!(exit_code, 1); // not running
    }

    #[test]
    fn test_check_sse_port_not_listening() {
        // Port 19999 is very unlikely to be in use
        assert!(!check_sse_port(19999));
    }

    #[test]
    fn test_server_status_fields() {
        let status = ServerStatus {
            running: true,
            pid: Some(12345),
            cli_tool_count: 3,
            api_tool_count: 5,
            sse_port: 3000,
            log_file_exists: true,
            last_log_line: Some("test line".to_string()),
        };

        assert!(status.running);
        assert_eq!(status.pid, Some(12345));
        assert_eq!(status.cli_tool_count, 3);
        assert_eq!(status.api_tool_count, 5);
    }

    #[test]
    fn test_print_json_format() {
        let status = ServerStatus {
            running: false,
            pid: None,
            cli_tool_count: 2,
            api_tool_count: 1,
            sse_port: 3000,
            log_file_exists: false,
            last_log_line: None,
        };

        // Just verify it doesn't panic
        print_json(&status);
    }
}
