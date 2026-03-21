use crate::error::Result;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::store::ToolRegistry;

pub fn run(registry: &dyn ToolRegistry, api_registry: Option<&ApiToolRegistry>) -> Result<()> {
    let cli_tools = registry.list()?;
    let api_tools = api_registry
        .map(|r| r.list())
        .transpose()?
        .unwrap_or_default();

    if cli_tools.is_empty() && api_tools.is_empty() {
        println!("No tools registered. Use 'mcpw register' or 'mcpw import' to add tools.");
        return Ok(());
    }

    println!(
        "{:<30}{:<10}{:<8}{:<8}DESCRIPTION",
        "NAME", "KIND", "PARAMS", "TYPE"
    );
    println!("{}", "\u{2500}".repeat(96));

    for tool in &cli_tools {
        println!(
            "{:<30}{:<10}{:<8}{:<8}{}",
            truncate(&tool.name, 28),
            "CLI",
            tool.params.len(),
            tool.transport.to_string(),
            truncate(&tool.description, 30),
        );
    }

    for tool in &api_tools {
        println!(
            "{:<30}{:<10}{:<8}{:<8}{}",
            truncate(&tool.name, 28),
            format!("API {}", tool.method),
            tool.params.len(),
            tool.transport.to_string(),
            truncate(&tool.description, 30),
        );
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max {
        let truncated: String = s.chars().take(max - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
