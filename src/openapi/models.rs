use serde::{Deserialize, Serialize};

use crate::registry::models::{ParamType, TransportType};

/// How a parameter is passed in the HTTP request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ApiParamLocation {
    #[serde(rename = "path")]
    Path,
    #[serde(rename = "query")]
    Query,
    #[serde(rename = "header")]
    Header,
    #[serde(rename = "body")]
    Body,
}

/// A parameter for an API tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParam {
    pub name: String,
    pub description: String,
    pub param_type: ParamType,
    pub required: bool,
    pub location: ApiParamLocation,
}

/// Auth configuration — stores how to authenticate, never the secret itself
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub auth_type: String,           // "bearer", "header", "basic"
    pub auth_env: String,            // env var name holding the secret
    pub auth_header: Option<String>, // custom header name for "header" type
}

/// A static header sent with every request. Value is an env var name or literal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticHeader {
    pub name: String,            // header name, e.g. "x-client-id"
    pub env_var: Option<String>, // env var name, e.g. "X_CLIENT_ID"
    pub value: Option<String>,   // literal value (used if env_var is not set)
}

/// An API tool definition (imported from OpenAPI)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiToolDefinition {
    pub name: String,
    pub description: String,
    pub method: String,       // GET, POST, PUT, DELETE, PATCH
    pub url_template: String, // "https://api.example.com/users/{id}"
    pub params: Vec<ApiParam>,
    pub transport: TransportType,
    pub auth: Option<AuthConfig>,
    pub static_headers: Vec<StaticHeader>,
    pub source_spec: String, // where the spec was imported from
}

/// File format for persisted API tools
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiToolsFile {
    pub version: u32,
    pub tools: Vec<ApiToolDefinition>,
}

impl Default for ApiToolsFile {
    fn default() -> Self {
        Self {
            version: 1,
            tools: Vec::new(),
        }
    }
}
