use std::collections::HashMap;


use crate::error::{McpWrapError, Result};
use crate::registry::models::ToolArgValue;

pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub trait CommandExecutor: Send + Sync {
    fn execute(
        &self,
        command: &str,
        args: &HashMap<String, ToolArgValue>,
    ) -> Result<ExecutionResult>;
}

#[derive(Default)]
pub struct ProcessCommandExecutor;

impl ProcessCommandExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Maximum execution time for a tool call (30 seconds)
    const TIMEOUT_SECS: u64 = 30;

    /// Maximum output size to capture (1 MB)
    const MAX_OUTPUT_BYTES: usize = 1_048_576;
}

impl CommandExecutor for ProcessCommandExecutor {
    fn execute(
        &self,
        command: &str,
        args: &HashMap<String, ToolArgValue>,
    ) -> Result<ExecutionResult> {
        let tokens = shell_words::split(command).map_err(|e| McpWrapError::ExecutionFailed {
            exit_code: -1,
            stderr: format!("Failed to parse command: {}", e),
        })?;

        if tokens.is_empty() {
            return Err(McpWrapError::ExecutionFailed {
                exit_code: -1,
                stderr: "Empty command".to_string(),
            });
        }

        let executable = &tokens[0];
        let static_args = &tokens[1..];

        let mut cmd = crate::parser::help_runner::build_command(executable, static_args);

        // Add dynamic arguments - each as discrete .arg() tokens, never shell-interpolated
        for (key, value) in args {
            match value {
                ToolArgValue::Boolean(true) => {
                    cmd.arg(format!("--{}", key));
                }
                ToolArgValue::Boolean(false) => {
                    // Skip - do not append flag
                }
                _ => {
                    cmd.arg(format!("--{}", key));
                    cmd.arg(value.to_string());
                }
            }
        }

        let mut child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    McpWrapError::CommandNotFound(executable.clone())
                } else {
                    e.into()
                }
            })?;

        // Wait with timeout
        let timeout = std::time::Duration::from_secs(Self::TIMEOUT_SECS);
        let start = std::time::Instant::now();

        loop {
            match child.try_wait() {
                Ok(Some(_)) => break, // Process exited
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(McpWrapError::ExecutionFailed {
                            exit_code: -1,
                            stderr: format!(
                                "Command timed out after {} seconds",
                                Self::TIMEOUT_SECS
                            ),
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(e) => return Err(e.into()),
            }
        }

        let output = child.wait_with_output()?;

        // Cap output size to prevent memory exhaustion
        let stdout = String::from_utf8_lossy(
            &output.stdout[..output.stdout.len().min(Self::MAX_OUTPUT_BYTES)],
        )
        .to_string();
        let stderr = String::from_utf8_lossy(
            &output.stderr[..output.stderr.len().min(Self::MAX_OUTPUT_BYTES)],
        )
        .to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(ExecutionResult {
            stdout,
            stderr,
            exit_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_simple_command() {
        let executor = ProcessCommandExecutor::new();
        let args = HashMap::new();
        let result = executor.execute("echo hello", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[test]
    fn test_execute_with_string_args() {
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert(
            "message".to_string(),
            ToolArgValue::String("world".to_string()),
        );
        let result = executor.execute("echo", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        // echo will print --message world
        assert!(result.stdout.contains("--message"));
        assert!(result.stdout.contains("world"));
    }

    #[test]
    fn test_execute_with_boolean_true() {
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert("verbose".to_string(), ToolArgValue::Boolean(true));
        let result = executor.execute("echo", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("--verbose"));
    }

    #[test]
    fn test_execute_with_boolean_false_skipped() {
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert("verbose".to_string(), ToolArgValue::Boolean(false));
        let result = executor.execute("echo test", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(!result.stdout.contains("--verbose"));
    }

    #[test]
    fn test_execute_command_not_found() {
        let executor = ProcessCommandExecutor::new();
        let args = HashMap::new();
        let result = executor.execute("nonexistent_command_xyz_123", &args);
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_nonzero_exit() {
        let executor = ProcessCommandExecutor::new();
        let args = HashMap::new();
        let result = executor.execute("false", &args).unwrap();
        assert_ne!(result.exit_code, 0);
    }

    #[test]
    fn test_shell_injection_prevention() {
        // The value "; rm -rf /" should be passed as a literal string argument,
        // not interpreted as shell syntax.
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert(
            "message".to_string(),
            ToolArgValue::String("; rm -rf /".to_string()),
        );
        // Using echo, it should echo the literal string
        let result = executor.execute("echo", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        // The value should appear literally in the output
        assert!(result.stdout.contains("; rm -rf /"));
    }

    #[test]
    fn test_execute_with_integer_arg() {
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert("width".to_string(), ToolArgValue::Integer(800));
        let result = executor.execute("echo", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("--width"));
        assert!(result.stdout.contains("800"));
    }

    #[test]
    fn test_execute_with_float_arg() {
        let executor = ProcessCommandExecutor::new();
        let mut args = HashMap::new();
        args.insert("rate".to_string(), ToolArgValue::Float(3.14));
        let result = executor.execute("echo", &args).unwrap();
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("--rate"));
        assert!(result.stdout.contains("3.14"));
    }
}
