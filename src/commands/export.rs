use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::openapi::models::ApiToolDefinition;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::models::ToolDefinition;
use crate::registry::store::ToolRegistry;

/// Portable export format containing both CLI and API tools
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportBundle {
    pub version: u32,
    pub cli_tools: Vec<ToolDefinition>,
    pub api_tools: Vec<ApiToolDefinition>,
}

/// Build an export bundle from registries, optionally filtering by tool name
pub fn build_export(
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
    tool_name: Option<&str>,
) -> Result<ExportBundle> {
    let cli_tools = if let Some(name) = tool_name {
        registry.get(name)?.map(|t| vec![t]).unwrap_or_default()
    } else {
        registry.list()?
    };

    let api_tools = if let Some(name) = tool_name {
        match api_registry {
            Some(r) => r.get(name)?.map(|t| vec![t]).unwrap_or_default(),
            None => Vec::new(),
        }
    } else {
        api_registry
            .map(|r| r.list())
            .transpose()?
            .unwrap_or_default()
    };

    Ok(ExportBundle {
        version: 1,
        cli_tools,
        api_tools,
    })
}

/// Run the export command — outputs JSON to stdout
pub fn run(
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
    tool_name: Option<&str>,
) -> Result<()> {
    let bundle = build_export(registry, api_registry, tool_name)?;

    if bundle.cli_tools.is_empty() && bundle.api_tools.is_empty() {
        if let Some(name) = tool_name {
            eprintln!("No tool found with name '{}'", name);
            std::process::exit(1);
        } else {
            eprintln!("No tools to export. Use 'mcpw register' or 'mcpw import' first.");
            std::process::exit(1);
        }
    }

    let json = serde_json::to_string_pretty(&bundle)?;
    println!("{}", json);

    Ok(())
}

/// Import tools from an export bundle JSON string
pub fn import_from_bundle(
    json: &str,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
) -> Result<(usize, usize)> {
    let bundle: ExportBundle = serde_json::from_str(json)?;

    let cli_count = bundle.cli_tools.len();
    for tool in bundle.cli_tools {
        registry.add(tool)?;
    }

    let api_count = bundle.api_tools.len();
    if let Some(api_reg) = api_registry {
        if !bundle.api_tools.is_empty() {
            api_reg.add_many(bundle.api_tools)?;
        }
    }

    Ok((cli_count, api_count))
}

/// Run the import-config command — reads JSON from a file
pub fn run_import(
    file_path: &str,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
) -> Result<()> {
    let json = std::fs::read_to_string(file_path).map_err(|e| {
        crate::error::McpWrapError::RegistryError(format!("Cannot read '{}': {}", file_path, e))
    })?;

    let (cli_count, api_count) = import_from_bundle(&json, registry, api_registry)?;
    println!(
        "Imported {} CLI tool(s) and {} API tool(s)",
        cli_count, api_count
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{ToolDefinition, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;

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
    fn test_build_export_empty() {
        let registry = InMemoryRegistry::new();
        let bundle = build_export(&registry, None, None).unwrap();
        assert!(bundle.cli_tools.is_empty());
        assert!(bundle.api_tools.is_empty());
        assert_eq!(bundle.version, 1);
    }

    #[test]
    fn test_build_export_with_cli_tools() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("tool_a")).unwrap();
        registry.add(make_tool("tool_b")).unwrap();

        let bundle = build_export(&registry, None, None).unwrap();
        assert_eq!(bundle.cli_tools.len(), 2);
    }

    #[test]
    fn test_build_export_filter_by_name() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("tool_a")).unwrap();
        registry.add(make_tool("tool_b")).unwrap();

        let bundle = build_export(&registry, None, Some("tool_a")).unwrap();
        assert_eq!(bundle.cli_tools.len(), 1);
        assert_eq!(bundle.cli_tools[0].name, "tool_a");
    }

    #[test]
    fn test_build_export_filter_nonexistent() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("tool_a")).unwrap();

        let bundle = build_export(&registry, None, Some("nonexistent")).unwrap();
        assert!(bundle.cli_tools.is_empty());
    }

    #[test]
    fn test_export_bundle_serialization_roundtrip() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("roundtrip_tool")).unwrap();

        let bundle = build_export(&registry, None, None).unwrap();
        let json = serde_json::to_string_pretty(&bundle).unwrap();
        let deserialized: ExportBundle = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.cli_tools.len(), 1);
        assert_eq!(deserialized.cli_tools[0].name, "roundtrip_tool");
    }

    #[test]
    fn test_import_from_bundle_cli_tools() {
        let source = InMemoryRegistry::new();
        source.add(make_tool("imported_tool")).unwrap();

        let bundle = build_export(&source, None, None).unwrap();
        let json = serde_json::to_string(&bundle).unwrap();

        let target = InMemoryRegistry::new();
        let (cli_count, api_count) = import_from_bundle(&json, &target, None).unwrap();

        assert_eq!(cli_count, 1);
        assert_eq!(api_count, 0);
        assert!(target.get("imported_tool").unwrap().is_some());
    }

    #[test]
    fn test_import_from_bundle_overwrites_existing() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("existing")).unwrap();

        let mut tool = make_tool("existing");
        tool.description = "Updated description".to_string();
        let bundle = ExportBundle {
            version: 1,
            cli_tools: vec![tool],
            api_tools: vec![],
        };
        let json = serde_json::to_string(&bundle).unwrap();

        let (cli_count, _) = import_from_bundle(&json, &registry, None).unwrap();
        assert_eq!(cli_count, 1);

        let imported = registry.get("existing").unwrap().unwrap();
        assert_eq!(imported.description, "Updated description");
    }

    #[test]
    fn test_import_from_bundle_invalid_json() {
        let registry = InMemoryRegistry::new();
        let result = import_from_bundle("not json", &registry, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_from_bundle_multiple_tools() {
        let bundle = ExportBundle {
            version: 1,
            cli_tools: vec![make_tool("a"), make_tool("b"), make_tool("c")],
            api_tools: vec![],
        };
        let json = serde_json::to_string(&bundle).unwrap();

        let registry = InMemoryRegistry::new();
        let (cli_count, _) = import_from_bundle(&json, &registry, None).unwrap();

        assert_eq!(cli_count, 3);
        assert_eq!(registry.list().unwrap().len(), 3);
    }
}
