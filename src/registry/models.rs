use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ToolArgValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

impl fmt::Display for ToolArgValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolArgValue::String(s) => write!(f, "{}", s),
            ToolArgValue::Integer(i) => write!(f, "{}", i),
            ToolArgValue::Float(v) => write!(f, "{}", v),
            ToolArgValue::Boolean(b) => write!(f, "{}", b),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParamType {
    String,
    Integer,
    Float,
    Boolean,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransportType {
    #[serde(rename = "stdio")]
    Stdio,
    #[serde(rename = "sse")]
    Sse,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportType::Stdio => write!(f, "STDIO"),
            TransportType::Sse => write!(f, "SSE"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub command: String,
    pub description: String,
    pub params: Vec<ToolParam>,
    pub transport: TransportType,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolsFile {
    pub version: u32,
    pub tools: Vec<ToolDefinition>,
}

impl Default for ToolsFile {
    fn default() -> Self {
        Self {
            version: 1,
            tools: Vec::new(),
        }
    }
}
