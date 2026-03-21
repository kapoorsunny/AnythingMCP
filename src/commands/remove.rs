use std::path::Path;

use crate::error::{McpWrapError, Result};
use crate::registry::store::ToolRegistry;

pub fn run(
    registry: &dyn ToolRegistry,
    name: Option<&str>,
    all: bool,
    tools_path: &Path,
) -> Result<()> {
    if all {
        return remove_all(registry, tools_path);
    }

    let name = match name {
        Some(n) => n,
        None => {
            eprintln!("Error: provide a tool name or use --all to remove everything.");
            std::process::exit(1);
        }
    };

    match registry.remove(name) {
        Ok(()) => {
            println!("\u{2714} Removed tool '{}'", name);
            Ok(())
        }
        Err(McpWrapError::ToolNotFound(_)) => {
            eprintln!("Error: tool '{}' not found.", name);
            std::process::exit(1);
        }
        Err(e) => Err(e),
    }
}

fn remove_all(registry: &dyn ToolRegistry, tools_path: &Path) -> Result<()> {
    // Remove all CLI tools
    let cli_tools = registry.list()?;
    let cli_count = cli_tools.len();
    for tool in &cli_tools {
        registry.remove(&tool.name)?;
    }

    // Remove API tools file
    let api_path = tools_path
        .parent()
        .map(|p| p.join("api_tools.json"))
        .unwrap_or_else(|| tools_path.with_file_name("api_tools.json"));

    let api_count = if api_path.exists() {
        let content = std::fs::read_to_string(&api_path)?;
        let file: serde_json::Value = serde_json::from_str(&content)?;
        let count = file["tools"].as_array().map(|a| a.len()).unwrap_or(0);
        std::fs::remove_file(&api_path)?;
        count
    } else {
        0
    };

    println!(
        "\u{2714} Removed all tools ({} CLI, {} API)",
        cli_count, api_count
    );
    Ok(())
}
