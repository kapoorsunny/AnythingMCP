use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::error::{McpWrapError, Result};
use crate::registry::models::{ToolDefinition, ToolsFile};

pub trait ToolRegistry: Send + Sync {
    fn add(&self, tool: ToolDefinition) -> Result<()>;
    fn remove(&self, name: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<ToolDefinition>>;
    fn list(&self) -> Result<Vec<ToolDefinition>>;
}

pub struct JsonFileRegistry {
    path: PathBuf,
    tools: Arc<Mutex<Vec<ToolDefinition>>>,
}

impl JsonFileRegistry {
    pub fn new(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tools = if path.exists() {
            let content = fs::read_to_string(&path)?;
            let tools_file: ToolsFile = serde_json::from_str(&content)?;
            tools_file.tools
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            tools: Arc::new(Mutex::new(tools)),
        })
    }

    fn persist(&self, tools: &[ToolDefinition]) -> Result<()> {
        let tools_file = ToolsFile {
            version: 1,
            tools: tools.to_vec(),
        };
        let json = serde_json::to_string_pretty(&tools_file)?;

        // Atomic write: write to PID-unique temp file then rename
        let temp_path = self
            .path
            .with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temp_path, &json)?;
        fs::rename(&temp_path, &self.path)?;

        Ok(())
    }
}

impl ToolRegistry for JsonFileRegistry {
    fn add(&self, tool: ToolDefinition) -> Result<()> {
        let mut tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;

        // Upsert: remove existing tool with same name
        tools.retain(|t| t.name != tool.name);
        tools.push(tool);

        self.persist(&tools)?;
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<()> {
        let mut tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;

        let initial_len = tools.len();
        tools.retain(|t| t.name != name);

        if tools.len() == initial_len {
            return Err(McpWrapError::ToolNotFound(name.to_string()));
        }

        self.persist(&tools)?;
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<ToolDefinition>> {
        let tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;

        Ok(tools.iter().find(|t| t.name == name).cloned())
    }

    fn list(&self) -> Result<Vec<ToolDefinition>> {
        let tools = self
            .tools
            .lock()
            .map_err(|e| McpWrapError::RegistryError(format!("Failed to acquire lock: {}", e)))?;

        Ok(tools.clone())
    }
}

/// In-memory registry for testing
#[cfg(test)]
pub struct InMemoryRegistry {
    tools: Arc<Mutex<Vec<ToolDefinition>>>,
}

#[cfg(test)]
impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg(test)]
impl ToolRegistry for InMemoryRegistry {
    fn add(&self, tool: ToolDefinition) -> Result<()> {
        let mut tools = self.tools.lock().unwrap();
        tools.retain(|t| t.name != tool.name);
        tools.push(tool);
        Ok(())
    }

    fn remove(&self, name: &str) -> Result<()> {
        let mut tools = self.tools.lock().unwrap();
        let initial_len = tools.len();
        tools.retain(|t| t.name != name);
        if tools.len() == initial_len {
            return Err(McpWrapError::ToolNotFound(name.to_string()));
        }
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Option<ToolDefinition>> {
        let tools = self.tools.lock().unwrap();
        Ok(tools.iter().find(|t| t.name == name).cloned())
    }

    fn list(&self) -> Result<Vec<ToolDefinition>> {
        let tools = self.tools.lock().unwrap();
        Ok(tools.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_tool(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            command: format!("echo {}", name),
            description: format!("Test tool {}", name),
            params: vec![],
            transport: crate::registry::models::TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_json_registry_add_and_get() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        let tool = make_tool("test_tool");
        registry.add(tool.clone()).unwrap();

        let retrieved = registry.get("test_tool").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test_tool");
    }

    #[test]
    fn test_json_registry_add_and_list() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        registry.add(make_tool("a")).unwrap();
        registry.add(make_tool("b")).unwrap();

        let tools = registry.list().unwrap();
        assert_eq!(tools.len(), 2);
    }

    #[test]
    fn test_json_registry_upsert() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        registry.add(make_tool("a")).unwrap();
        let mut updated = make_tool("a");
        updated.description = "Updated".to_string();
        registry.add(updated).unwrap();

        let tools = registry.list().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].description, "Updated");
    }

    #[test]
    fn test_json_registry_remove() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        registry.add(make_tool("a")).unwrap();
        registry.remove("a").unwrap();

        let tools = registry.list().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_json_registry_remove_not_found() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        let result = registry.remove("nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            McpWrapError::ToolNotFound(name) => assert_eq!(name, "nonexistent"),
            e => panic!("Expected ToolNotFound, got: {:?}", e),
        }
    }

    #[test]
    fn test_json_registry_get_not_found() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        let result = registry.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_json_registry_persistence() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");

        // Add tool with first registry instance
        {
            let registry = JsonFileRegistry::new(path.clone()).unwrap();
            registry.add(make_tool("persistent")).unwrap();
        }

        // Load with second registry instance
        {
            let registry = JsonFileRegistry::new(path).unwrap();
            let tools = registry.list().unwrap();
            assert_eq!(tools.len(), 1);
            assert_eq!(tools[0].name, "persistent");
        }
    }

    #[test]
    fn test_json_registry_empty_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tools.json");
        let registry = JsonFileRegistry::new(path).unwrap();

        let tools = registry.list().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_in_memory_registry() {
        let registry = InMemoryRegistry::new();

        registry.add(make_tool("a")).unwrap();
        registry.add(make_tool("b")).unwrap();

        assert_eq!(registry.list().unwrap().len(), 2);
        assert!(registry.get("a").unwrap().is_some());

        registry.remove("a").unwrap();
        assert_eq!(registry.list().unwrap().len(), 1);
        assert!(registry.get("a").unwrap().is_none());

        assert!(registry.remove("nonexistent").is_err());
    }
}
