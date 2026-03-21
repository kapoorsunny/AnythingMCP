use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;

/// Global logger instance
static LOGGER: std::sync::OnceLock<Mutex<McpwLogger>> = std::sync::OnceLock::new();

pub struct McpwLogger {
    path: PathBuf,
}

impl McpwLogger {
    /// Initialize the global logger. Call once at startup.
    pub fn init(tools_path: &Path) {
        let log_path = tools_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| tools_path.to_path_buf())
            .join("mcpw.log");

        if let Some(parent) = log_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let _ = LOGGER.set(Mutex::new(McpwLogger { path: log_path }));
    }

    fn write_line(&self, line: &str) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{}", line);
        }
    }
}

/// Log an event. Safe to call from anywhere — no-ops if logger not initialized.
pub fn log(level: &str, event: &str, details: &str) {
    if let Some(logger) = LOGGER.get() {
        if let Ok(logger) = logger.lock() {
            let ts = Utc::now().format("%Y-%m-%d %H:%M:%S");
            let line = if details.is_empty() {
                format!("{} [{:<5}] {}", ts, level, event)
            } else {
                format!("{} [{:<5}] {} {}", ts, level, event, details)
            };
            logger.write_line(&line);
        }
    }
}

// Convenience functions — some used by serve/register, others reserved for future commands

#[allow(dead_code)]
pub fn info(message: &str) {
    log("INFO", message, "");
}

pub fn call_ok(tool: &str, duration_ms: u128) {
    log(
        "CALL",
        &format!("{} -> OK", tool),
        &format!("({}ms)", duration_ms),
    );
}

pub fn call_err(tool: &str, duration_ms: u128, error: &str) {
    log(
        "CALL",
        &format!("{} -> ERROR", tool),
        &format!("({}ms) {}", duration_ms, error),
    );
}

#[allow(dead_code)]
pub fn block(cmd: &str, reason: &str) {
    log(
        "BLOCK",
        &format!("Rejected '{}'", cmd),
        &format!("— {}", reason),
    );
}

pub fn register(name: &str, transport: &str, params: usize) {
    log(
        "REG",
        &format!("Registered '{}'", name),
        &format!("({}, {} params)", transport, params),
    );
}

#[allow(dead_code)]
pub fn remove(name: &str) {
    log("REG", &format!("Removed '{}'", name), "");
}

#[allow(dead_code)]
pub fn import(source: &str, count: usize) {
    log(
        "REG",
        &format!("Imported {} tools", count),
        &format!("from {}", source),
    );
}

pub fn server_start(pid: u32, stdio_count: usize, sse_count: usize, progressive: bool) {
    let mode = if progressive { ", progressive" } else { "" };
    log(
        "INFO",
        "Server started",
        &format!(
            "(PID {}, STDIO: {} tools, SSE: {} tools{})",
            pid, stdio_count, sse_count, mode
        ),
    );
}

#[allow(dead_code)]
pub fn server_stop() {
    log("INFO", "Server stopped", "");
}

/// Get the log file path (for the `logs` command)
pub fn log_path(tools_path: &Path) -> PathBuf {
    tools_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| tools_path.to_path_buf())
        .join("mcpw.log")
}
