use chrono::Utc;

use crate::error::Result;
use crate::parser::help_parser::HelpParser;
use crate::parser::help_runner::HelpRunner;
use crate::registry::models::ToolDefinition;
use crate::registry::store::ToolRegistry;

/// Result of updating a single tool
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateOutcome {
    Updated,
    Unchanged,
    Error,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct UpdateResult {
    pub tool_name: String,
    pub outcome: UpdateOutcome,
    pub message: String,
    pub old_param_count: usize,
    pub new_param_count: usize,
}

/// Update a single tool by re-running help discovery
pub fn update_tool(
    tool: &ToolDefinition,
    registry: &dyn ToolRegistry,
    help_runner: &dyn HelpRunner,
    help_parser: &dyn HelpParser,
    dry_run: bool,
) -> UpdateResult {
    let help_output = match help_runner.run_help(&tool.command) {
        Ok(output) => output,
        Err(e) => {
            return UpdateResult {
                tool_name: tool.name.clone(),
                outcome: UpdateOutcome::Error,
                message: format!("Help discovery failed: {}", e),
                old_param_count: tool.params.len(),
                new_param_count: 0,
            };
        }
    };

    if help_output.is_empty() {
        return UpdateResult {
            tool_name: tool.name.clone(),
            outcome: UpdateOutcome::Unchanged,
            message: "No help output available".to_string(),
            old_param_count: tool.params.len(),
            new_param_count: tool.params.len(),
        };
    }

    let new_params = match help_parser.parse(&help_output) {
        Ok(params) => params,
        Err(e) => {
            return UpdateResult {
                tool_name: tool.name.clone(),
                outcome: UpdateOutcome::Error,
                message: format!("Parse failed: {}", e),
                old_param_count: tool.params.len(),
                new_param_count: 0,
            };
        }
    };

    // Compare param names
    let old_names: std::collections::HashSet<&str> =
        tool.params.iter().map(|p| p.name.as_str()).collect();
    let new_names: std::collections::HashSet<&str> =
        new_params.iter().map(|p| p.name.as_str()).collect();

    if old_names == new_names {
        return UpdateResult {
            tool_name: tool.name.clone(),
            outcome: UpdateOutcome::Unchanged,
            message: format!("No changes ({} params)", tool.params.len()),
            old_param_count: tool.params.len(),
            new_param_count: new_params.len(),
        };
    }

    let old_count = tool.params.len();
    let new_count = new_params.len();

    if dry_run {
        let added: Vec<&str> = new_names.difference(&old_names).copied().collect();
        let removed: Vec<&str> = old_names.difference(&new_names).copied().collect();
        let mut parts = Vec::new();
        if !added.is_empty() {
            parts.push(format!("+{}", added.join(", +")));
        }
        if !removed.is_empty() {
            parts.push(format!("-{}", removed.join(", -")));
        }
        return UpdateResult {
            tool_name: tool.name.clone(),
            outcome: UpdateOutcome::Updated,
            message: format!(
                "Would update: {} -> {} params ({})",
                old_count,
                new_count,
                parts.join(", ")
            ),
            old_param_count: old_count,
            new_param_count: new_count,
        };
    }

    // Apply update
    let updated_tool = ToolDefinition {
        name: tool.name.clone(),
        command: tool.command.clone(),
        description: tool.description.clone(),
        params: new_params,
        transport: tool.transport.clone(),
        registered_at: Utc::now(),
    };

    if let Err(e) = registry.add(updated_tool) {
        return UpdateResult {
            tool_name: tool.name.clone(),
            outcome: UpdateOutcome::Error,
            message: format!("Failed to save: {}", e),
            old_param_count: old_count,
            new_param_count: new_count,
        };
    }

    UpdateResult {
        tool_name: tool.name.clone(),
        outcome: UpdateOutcome::Updated,
        message: format!("Updated: {} -> {} params", old_count, new_count),
        old_param_count: old_count,
        new_param_count: new_count,
    }
}

/// Run the update command
pub fn run(
    registry: &dyn ToolRegistry,
    help_runner: &dyn HelpRunner,
    help_parser: &dyn HelpParser,
    tool_name: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let tools = if let Some(name) = tool_name {
        match registry.get(name)? {
            Some(t) => vec![t],
            None => {
                eprintln!("Error: tool '{}' not found.", name);
                std::process::exit(1);
            }
        }
    } else {
        registry.list()?
    };

    if tools.is_empty() {
        println!("No CLI tools to update.");
        return Ok(());
    }

    let mode = if dry_run { " (dry run)" } else { "" };
    println!("Updating {} tool(s){}...", tools.len(), mode);
    println!("{}", "\u{2500}".repeat(60));

    let mut updated = 0;
    let mut unchanged = 0;
    let mut errors = 0;

    for tool in &tools {
        let result = update_tool(tool, registry, help_runner, help_parser, dry_run);

        let icon = match result.outcome {
            UpdateOutcome::Updated => {
                updated += 1;
                "UPD"
            }
            UpdateOutcome::Unchanged => {
                unchanged += 1;
                "OK"
            }
            UpdateOutcome::Error => {
                errors += 1;
                "ERR"
            }
        };

        println!("  [{}] {}: {}", icon, result.tool_name, result.message);
    }

    println!();
    println!(
        "{} updated, {} unchanged, {} errors",
        updated, unchanged, errors
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{ParamType, ToolParam, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;

    struct MockHelpRunner {
        output: String,
    }

    impl MockHelpRunner {
        fn new(output: &str) -> Self {
            Self {
                output: output.to_string(),
            }
        }
    }

    impl crate::parser::help_runner::HelpRunner for MockHelpRunner {
        fn run_help(&self, _command: &str) -> crate::error::Result<String> {
            Ok(self.output.clone())
        }
    }

    struct MockHelpParser {
        params: Vec<ToolParam>,
    }

    impl MockHelpParser {
        fn new(params: Vec<ToolParam>) -> Self {
            Self { params }
        }
    }

    impl HelpParser for MockHelpParser {
        fn parse(&self, _help_text: &str) -> crate::error::Result<Vec<ToolParam>> {
            Ok(self.params.clone())
        }
    }

    fn make_param(name: &str) -> ToolParam {
        ToolParam {
            name: name.to_string(),
            description: String::new(),
            param_type: ParamType::String,
            required: false,
            default_value: None,
        }
    }

    fn make_tool_with_params(name: &str, params: Vec<ToolParam>) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            command: "echo".to_string(),
            description: "Test tool".to_string(),
            params,
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_update_tool_no_changes() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let registry = InMemoryRegistry::new();
        registry.add(tool.clone()).unwrap();

        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input")]);

        let result = update_tool(&tool, &registry, &runner, &parser, false);
        assert_eq!(result.outcome, UpdateOutcome::Unchanged);
    }

    #[test]
    fn test_update_tool_with_changes() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let registry = InMemoryRegistry::new();
        registry.add(tool.clone()).unwrap();

        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input"), make_param("output")]);

        let result = update_tool(&tool, &registry, &runner, &parser, false);
        assert_eq!(result.outcome, UpdateOutcome::Updated);
        assert_eq!(result.old_param_count, 1);
        assert_eq!(result.new_param_count, 2);

        // Verify the registry was updated
        let updated = registry.get("my_tool").unwrap().unwrap();
        assert_eq!(updated.params.len(), 2);
    }

    #[test]
    fn test_update_tool_dry_run_no_save() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let registry = InMemoryRegistry::new();
        registry.add(tool.clone()).unwrap();

        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input"), make_param("new_flag")]);

        let result = update_tool(&tool, &registry, &runner, &parser, true);
        assert_eq!(result.outcome, UpdateOutcome::Updated);
        assert!(result.message.contains("Would update"));

        // Verify the registry was NOT changed
        let stored = registry.get("my_tool").unwrap().unwrap();
        assert_eq!(stored.params.len(), 1);
    }

    #[test]
    fn test_update_tool_empty_help() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let registry = InMemoryRegistry::new();

        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let result = update_tool(&tool, &registry, &runner, &parser, false);
        assert_eq!(result.outcome, UpdateOutcome::Unchanged);
    }

    #[test]
    fn test_update_tool_params_removed() {
        let tool =
            make_tool_with_params("my_tool", vec![make_param("input"), make_param("old_flag")]);
        let registry = InMemoryRegistry::new();
        registry.add(tool.clone()).unwrap();

        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input")]);

        let result = update_tool(&tool, &registry, &runner, &parser, false);
        assert_eq!(result.outcome, UpdateOutcome::Updated);
        assert_eq!(result.new_param_count, 1);
    }

    #[test]
    fn test_run_no_tools() {
        let registry = InMemoryRegistry::new();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        run(&registry, &runner, &parser, None, false).unwrap();
        // Should print "No CLI tools to update" and return Ok
    }

    #[test]
    fn test_run_tool_not_found() {
        let registry = InMemoryRegistry::new();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        // This calls process::exit, so we can't test it directly in-process
        // Instead, test the branch before it
        let tool = registry.get("nonexistent").unwrap();
        assert!(tool.is_none());
    }

    #[test]
    fn test_run_all_tools() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool_with_params("tool_a", vec![make_param("x")]))
            .unwrap();
        registry
            .add(make_tool_with_params("tool_b", vec![]))
            .unwrap();

        let runner = MockHelpRunner::new("some help");
        let parser = MockHelpParser::new(vec![make_param("x")]);

        run(&registry, &runner, &parser, None, false).unwrap();
    }

    #[test]
    fn test_run_specific_tool() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool_with_params("tool_a", vec![make_param("x")]))
            .unwrap();

        let runner = MockHelpRunner::new("some help");
        let parser = MockHelpParser::new(vec![make_param("x"), make_param("y")]);

        run(&registry, &runner, &parser, Some("tool_a"), false).unwrap();

        // Verify it was updated
        let updated = registry.get("tool_a").unwrap().unwrap();
        assert_eq!(updated.params.len(), 2);
    }

    #[test]
    fn test_run_dry_run() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool_with_params("tool_a", vec![make_param("x")]))
            .unwrap();

        let runner = MockHelpRunner::new("some help");
        let parser = MockHelpParser::new(vec![make_param("x"), make_param("y")]);

        run(&registry, &runner, &parser, None, true).unwrap();

        // Verify nothing was changed
        let stored = registry.get("tool_a").unwrap().unwrap();
        assert_eq!(stored.params.len(), 1);
    }
}
