use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::{McpWrapError, Result};
use crate::openapi::models::{ApiToolDefinition, ApiToolsFile};

/// Registry for API tools (imported from OpenAPI specs)
pub struct ApiToolRegistry {
    path: PathBuf,
    tools: Arc<Mutex<Vec<ApiToolDefinition>>>,
}

impl ApiToolRegistry {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tools = if path.exists() {
            let content = fs::read_to_string(&path)?;
            let file: ApiToolsFile = serde_json::from_str(&content)?;
            file.tools
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            tools: Arc::new(Mutex::new(tools)),
        })
    }

    fn persist(&self, tools: &[ApiToolDefinition]) -> Result<()> {
        let file = ApiToolsFile {
            version: 1,
            tools: tools.to_vec(),
        };
        let json = serde_json::to_string_pretty(&file)?;
        let temp_path = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temp_path, &json)?;
        fs::rename(&temp_path, &self.path)?;
        Ok(())
    }

    /// Add multiple tools (from an import), replacing any with the same name
    pub fn add_many(&self, new_tools: Vec<ApiToolDefinition>) -> Result<()> {
        let mut tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;

        for new_tool in &new_tools {
            tools.retain(|t| t.name != new_tool.name);
        }
        tools.extend(new_tools);

        self.persist(&tools)?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<ApiToolDefinition>> {
        let tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;
        Ok(tools.clone())
    }

    pub fn get(&self, name: &str) -> Result<Option<ApiToolDefinition>> {
        let tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;
        Ok(tools.iter().find(|t| t.name == name).cloned())
    }
}
