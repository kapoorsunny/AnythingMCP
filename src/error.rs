use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpWrapError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Help parsing failed for command '{cmd}': {reason}")]
    HelpParseFailed { cmd: String, reason: String },

    #[error("Command execution failed (exit {exit_code}): {stderr}")]
    ExecutionFailed { exit_code: i32, stderr: String },

    #[error("Invalid argument type for param '{param}': expected {expected}")]
    InvalidArgType { param: String, expected: String },

    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, McpWrapError>;
