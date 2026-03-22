use crate::error::Result;
use crate::parser::help_parser::HelpParser;
use crate::parser::help_runner::HelpRunner;
use crate::registry::models::ToolDefinition;
use crate::registry::store::ToolRegistry;

/// Validation result for a single tool
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationStatus {
    Ok,
    Drift,
    Error,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ValidationResult {
    pub tool_name: String,
    pub status: ValidationStatus,
    pub message: String,
    pub added_params: Vec<String>,
    pub removed_params: Vec<String>,
}

/// Validate a single CLI tool by re-running help discovery and comparing params
pub fn validate_tool(
    tool: &ToolDefinition,
    help_runner: &dyn HelpRunner,
    help_parser: &dyn HelpParser,
) -> ValidationResult {
    // Check command is parseable
    let tokens = match shell_words::split(&tool.command) {
        Ok(t) if !t.is_empty() => t,
        _ => {
            return ValidationResult {
                tool_name: tool.name.clone(),
                status: ValidationStatus::Error,
                message: format!("Cannot parse command: {}", tool.command),
                added_params: vec![],
                removed_params: vec![],
            };
        }
    };

    // Check executable exists
    let executable = &tokens[0];
    if !command_exists(executable) {
        return ValidationResult {
            tool_name: tool.name.clone(),
            status: ValidationStatus::Error,
            message: format!("Command '{}' not found", executable),
            added_params: vec![],
            removed_params: vec![],
        };
    }

    // Run help and parse params
    let help_output = match help_runner.run_help(&tool.command) {
        Ok(output) => output,
        Err(e) => {
            return ValidationResult {
                tool_name: tool.name.clone(),
                status: ValidationStatus::Error,
                message: format!("Help discovery failed: {}", e),
                added_params: vec![],
                removed_params: vec![],
            };
        }
    };

    if help_output.is_empty() {
        return ValidationResult {
            tool_name: tool.name.clone(),
            status: ValidationStatus::Ok,
            message: "No help output (cannot detect drift)".to_string(),
            added_params: vec![],
            removed_params: vec![],
        };
    }

    let current_params = match help_parser.parse(&help_output) {
        Ok(params) => params,
        Err(e) => {
            return ValidationResult {
                tool_name: tool.name.clone(),
                status: ValidationStatus::Error,
                message: format!("Parse failed: {}", e),
                added_params: vec![],
                removed_params: vec![],
            };
        }
    };
    let current_names: std::collections::HashSet<&str> =
        current_params.iter().map(|p| p.name.as_str()).collect();
    let stored_names: std::collections::HashSet<&str> =
        tool.params.iter().map(|p| p.name.as_str()).collect();

    let added: Vec<String> = current_names
        .difference(&stored_names)
        .map(|s| s.to_string())
        .collect();
    let removed: Vec<String> = stored_names
        .difference(&current_names)
        .map(|s| s.to_string())
        .collect();

    if added.is_empty() && removed.is_empty() {
        ValidationResult {
            tool_name: tool.name.clone(),
            status: ValidationStatus::Ok,
            message: format!("Schema matches ({} params)", tool.params.len()),
            added_params: vec![],
            removed_params: vec![],
        }
    } else {
        let mut drift_parts = Vec::new();
        if !added.is_empty() {
            drift_parts.push(format!("new: {}", added.join(", ")));
        }
        if !removed.is_empty() {
            drift_parts.push(format!("removed: {}", removed.join(", ")));
        }
        ValidationResult {
            tool_name: tool.name.clone(),
            status: ValidationStatus::Drift,
            message: format!("Schema drift detected ({})", drift_parts.join("; ")),
            added_params: added,
            removed_params: removed,
        }
    }
}

/// Run validation for all tools or a specific tool
pub fn run(
    registry: &dyn ToolRegistry,
    help_runner: &dyn HelpRunner,
    help_parser: &dyn HelpParser,
    tool_name: Option<&str>,
) -> Result<i32> {
    let tools = if let Some(name) = tool_name {
        match registry.get(name)? {
            Some(t) => vec![t],
            None => {
                eprintln!("Error: tool '{}' not found.", name);
                return Ok(1);
            }
        }
    } else {
        registry.list()?
    };

    if tools.is_empty() {
        println!("No CLI tools to validate.");
        return Ok(0);
    }

    println!("Validating {} tool(s)...", tools.len());
    println!("{}", "\u{2500}".repeat(60));

    let mut has_drift = false;
    let mut has_error = false;

    for tool in &tools {
        let result = validate_tool(tool, help_runner, help_parser);

        let icon = match result.status {
            ValidationStatus::Ok => "OK",
            ValidationStatus::Drift => {
                has_drift = true;
                "DRIFT"
            }
            ValidationStatus::Error => {
                has_error = true;
                "ERROR"
            }
        };

        println!("  [{}] {}: {}", icon, result.tool_name, result.message);
    }

    println!();

    if has_error {
        println!("Validation completed with errors.");
        Ok(1)
    } else if has_drift {
        println!("Schema drift detected. Run 'mcpw update' to refresh.");
        Ok(1)
    } else {
        println!("All tools validated successfully.");
        Ok(0)
    }
}

/// Check if a command exists (absolute path or in PATH)
fn command_exists(executable: &str) -> bool {
    use std::path::Path;

    if executable.contains('/') || executable.contains('\\') {
        return Path::new(executable).exists();
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join(executable).exists() {
                return true;
            }
            if cfg!(target_os = "windows") {
                for ext in &[".exe", ".cmd", ".bat", ".com"] {
                    if dir.join(format!("{}{}", executable, ext)).exists() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{ParamType, ToolParam, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;

    /// Mock help runner that returns configurable output
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

    impl HelpRunner for MockHelpRunner {
        fn run_help(&self, _command: &str) -> crate::error::Result<String> {
            Ok(self.output.clone())
        }
    }

    /// Mock help parser that returns configurable params
    struct MockHelpParser {
        params: Vec<crate::registry::models::ToolParam>,
    }

    impl MockHelpParser {
        fn new(params: Vec<crate::registry::models::ToolParam>) -> Self {
            Self { params }
        }
    }

    impl HelpParser for MockHelpParser {
        fn parse(
            &self,
            _help_text: &str,
        ) -> crate::error::Result<Vec<crate::registry::models::ToolParam>> {
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
            command: "echo".to_string(), // echo exists on all platforms
            description: "Test tool".to_string(),
            params,
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_validate_tool_no_drift() {
        let tool =
            make_tool_with_params("my_tool", vec![make_param("input"), make_param("output")]);
        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input"), make_param("output")]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Ok);
        assert!(result.added_params.is_empty());
        assert!(result.removed_params.is_empty());
    }

    #[test]
    fn test_validate_tool_new_params_detected() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input"), make_param("new_flag")]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Drift);
        assert!(result.added_params.contains(&"new_flag".to_string()));
    }

    #[test]
    fn test_validate_tool_removed_params_detected() {
        let tool =
            make_tool_with_params("my_tool", vec![make_param("input"), make_param("old_flag")]);
        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("input")]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Drift);
        assert!(result.removed_params.contains(&"old_flag".to_string()));
    }

    #[test]
    fn test_validate_tool_added_and_removed() {
        let tool = make_tool_with_params("my_tool", vec![make_param("old")]);
        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![make_param("new")]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Drift);
        assert!(result.added_params.contains(&"new".to_string()));
        assert!(result.removed_params.contains(&"old".to_string()));
    }

    #[test]
    fn test_validate_tool_empty_help_output() {
        let tool = make_tool_with_params("my_tool", vec![make_param("input")]);
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Ok);
        assert!(result.message.contains("cannot detect drift"));
    }

    #[test]
    fn test_validate_tool_command_not_found() {
        let mut tool = make_tool_with_params("bad_tool", vec![]);
        tool.command = "nonexistent_binary_xyz_999".to_string();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Error);
        assert!(result.message.contains("not found"));
    }

    #[test]
    fn test_validate_tool_unparseable_command() {
        let mut tool = make_tool_with_params("bad_tool", vec![]);
        tool.command = "\"unclosed quote".to_string();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Error);
        assert!(result.message.contains("Cannot parse"));
    }

    #[test]
    fn test_validate_tool_zero_params_no_drift() {
        let tool = make_tool_with_params("simple", vec![]);
        let runner = MockHelpRunner::new("some help text");
        let parser = MockHelpParser::new(vec![]);

        let result = validate_tool(&tool, &runner, &parser);
        assert_eq!(result.status, ValidationStatus::Ok);
    }

    #[test]
    fn test_run_no_tools() {
        let registry = InMemoryRegistry::new();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let exit_code = run(&registry, &runner, &parser, None).unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_run_tool_not_found() {
        let registry = InMemoryRegistry::new();
        let runner = MockHelpRunner::new("");
        let parser = MockHelpParser::new(vec![]);

        let exit_code = run(&registry, &runner, &parser, Some("nonexistent")).unwrap();
        assert_eq!(exit_code, 1);
    }

    #[test]
    fn test_run_all_ok() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool_with_params("tool_a", vec![make_param("x")]))
            .unwrap();

        let runner = MockHelpRunner::new("some help");
        let parser = MockHelpParser::new(vec![make_param("x")]);

        let exit_code = run(&registry, &runner, &parser, None).unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_run_with_drift() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool_with_params("tool_a", vec![make_param("old")]))
            .unwrap();

        let runner = MockHelpRunner::new("some help");
        let parser = MockHelpParser::new(vec![make_param("new")]);

        let exit_code = run(&registry, &runner, &parser, None).unwrap();
        assert_eq!(exit_code, 1);
    }

    #[test]
    fn test_command_exists_echo() {
        assert!(command_exists("echo"));
    }

    #[test]
    fn test_command_exists_nonexistent() {
        assert!(!command_exists("nonexistent_binary_xyz_999"));
    }
}
