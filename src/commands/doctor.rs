use std::path::Path;

use crate::error::Result;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::store::ToolRegistry;

/// A single diagnostic check result
#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

/// Run all diagnostic checks and return results
pub fn diagnose(
    tools_path: &Path,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();

    results.push(check_tools_dir(tools_path));
    results.push(check_tools_file(tools_path));
    results.extend(check_cli_tools(registry)?);
    results.push(check_api_tools_file(tools_path));
    results.extend(check_api_tool_env_vars(api_registry)?);
    results.push(check_blocklist_file(tools_path));

    Ok(results)
}

/// Print results to stdout and return exit code (0 = all pass, 1 = any fail)
pub fn run(
    tools_path: &Path,
    registry: &dyn ToolRegistry,
    api_registry: Option<&ApiToolRegistry>,
) -> Result<i32> {
    let results = diagnose(tools_path, registry, api_registry)?;

    println!("mcpw doctor");
    println!("{}", "\u{2500}".repeat(60));

    for result in &results {
        let icon = match result.status {
            CheckStatus::Pass => "PASS",
            CheckStatus::Warn => "WARN",
            CheckStatus::Fail => "FAIL",
        };
        println!("  [{}] {}: {}", icon, result.name, result.message);
    }

    println!();

    let fails = results
        .iter()
        .filter(|r| r.status == CheckStatus::Fail)
        .count();
    let warns = results
        .iter()
        .filter(|r| r.status == CheckStatus::Warn)
        .count();
    let passes = results
        .iter()
        .filter(|r| r.status == CheckStatus::Pass)
        .count();

    println!(
        "{} checks passed, {} warnings, {} failures",
        passes, warns, fails
    );

    if fails > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}

/// Check that the tools directory exists
fn check_tools_dir(tools_path: &Path) -> CheckResult {
    let dir = tools_path.parent().unwrap_or(tools_path);

    if dir.is_dir() {
        CheckResult {
            name: "Tools directory".to_string(),
            status: CheckStatus::Pass,
            message: format!("{} exists", dir.display()),
        }
    } else {
        CheckResult {
            name: "Tools directory".to_string(),
            status: CheckStatus::Fail,
            message: format!("{} does not exist", dir.display()),
        }
    }
}

/// Check that tools.json exists and is valid
fn check_tools_file(tools_path: &Path) -> CheckResult {
    if !tools_path.exists() {
        return CheckResult {
            name: "tools.json".to_string(),
            status: CheckStatus::Warn,
            message: "Not found (no CLI tools registered yet)".to_string(),
        };
    }

    match std::fs::read_to_string(tools_path) {
        Ok(content) => match serde_json::from_str::<crate::registry::models::ToolsFile>(&content) {
            Ok(_) => CheckResult {
                name: "tools.json".to_string(),
                status: CheckStatus::Pass,
                message: "Valid".to_string(),
            },
            Err(e) => CheckResult {
                name: "tools.json".to_string(),
                status: CheckStatus::Fail,
                message: format!("Invalid JSON: {}", e),
            },
        },
        Err(e) => CheckResult {
            name: "tools.json".to_string(),
            status: CheckStatus::Fail,
            message: format!("Cannot read: {}", e),
        },
    }
}

/// Check that each CLI tool's command executable exists
fn check_cli_tools(registry: &dyn ToolRegistry) -> Result<Vec<CheckResult>> {
    let tools = registry.list()?;
    let mut results = Vec::new();

    if tools.is_empty() {
        results.push(CheckResult {
            name: "CLI tools".to_string(),
            status: CheckStatus::Warn,
            message: "No CLI tools registered".to_string(),
        });
        return Ok(results);
    }

    for tool in &tools {
        let tokens = match shell_words::split(&tool.command) {
            Ok(t) if !t.is_empty() => t,
            _ => {
                results.push(CheckResult {
                    name: format!("Tool '{}'", tool.name),
                    status: CheckStatus::Fail,
                    message: format!("Cannot parse command: {}", tool.command),
                });
                continue;
            }
        };

        let executable = &tokens[0];
        let found = which_executable(executable);

        if found {
            results.push(CheckResult {
                name: format!("Tool '{}'", tool.name),
                status: CheckStatus::Pass,
                message: format!("Command '{}' found", executable),
            });
        } else {
            results.push(CheckResult {
                name: format!("Tool '{}'", tool.name),
                status: CheckStatus::Fail,
                message: format!("Command '{}' not found in PATH", executable),
            });
        }
    }

    Ok(results)
}

/// Check that api_tools.json exists and is valid
fn check_api_tools_file(tools_path: &Path) -> CheckResult {
    let api_path = tools_path
        .parent()
        .unwrap_or(tools_path)
        .join("api_tools.json");

    if !api_path.exists() {
        return CheckResult {
            name: "api_tools.json".to_string(),
            status: CheckStatus::Warn,
            message: "Not found (no API tools imported yet)".to_string(),
        };
    }

    match std::fs::read_to_string(&api_path) {
        Ok(content) => {
            match serde_json::from_str::<crate::openapi::models::ApiToolsFile>(&content) {
                Ok(_) => CheckResult {
                    name: "api_tools.json".to_string(),
                    status: CheckStatus::Pass,
                    message: "Valid".to_string(),
                },
                Err(e) => CheckResult {
                    name: "api_tools.json".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Invalid JSON: {}", e),
                },
            }
        }
        Err(e) => CheckResult {
            name: "api_tools.json".to_string(),
            status: CheckStatus::Fail,
            message: format!("Cannot read: {}", e),
        },
    }
}

/// Check that environment variables for API tool auth are set
fn check_api_tool_env_vars(api_registry: Option<&ApiToolRegistry>) -> Result<Vec<CheckResult>> {
    let mut results = Vec::new();

    let api_tools = match api_registry {
        Some(r) => r.list()?,
        None => return Ok(results),
    };

    if api_tools.is_empty() {
        return Ok(results);
    }

    for tool in &api_tools {
        if let Some(ref auth) = tool.auth {
            let env_var = &auth.auth_env;
            if std::env::var(env_var).is_ok() {
                results.push(CheckResult {
                    name: format!("API auth '{}'", tool.name),
                    status: CheckStatus::Pass,
                    message: format!("Env var '{}' is set", env_var),
                });
            } else {
                results.push(CheckResult {
                    name: format!("API auth '{}'", tool.name),
                    status: CheckStatus::Fail,
                    message: format!("Env var '{}' is NOT set", env_var),
                });
            }
        }

        for header in &tool.static_headers {
            if let Some(ref env_var) = header.env_var {
                if std::env::var(env_var).is_ok() {
                    results.push(CheckResult {
                        name: format!("API header '{}/{}'", tool.name, header.name),
                        status: CheckStatus::Pass,
                        message: format!("Env var '{}' is set", env_var),
                    });
                } else {
                    results.push(CheckResult {
                        name: format!("API header '{}/{}'", tool.name, header.name),
                        status: CheckStatus::Warn,
                        message: format!("Env var '{}' is NOT set", env_var),
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Check that blocklist.json exists and is valid
fn check_blocklist_file(tools_path: &Path) -> CheckResult {
    let blocklist_path = tools_path
        .parent()
        .unwrap_or(tools_path)
        .join("blocklist.json");

    if !blocklist_path.exists() {
        return CheckResult {
            name: "blocklist.json".to_string(),
            status: CheckStatus::Warn,
            message: "Not found (defaults will be used on first block)".to_string(),
        };
    }

    match std::fs::read_to_string(&blocklist_path) {
        Ok(content) => {
            match serde_json::from_str::<crate::commands::block::BlocklistFile>(&content) {
                Ok(bl) => CheckResult {
                    name: "blocklist.json".to_string(),
                    status: CheckStatus::Pass,
                    message: format!("Valid ({} commands blocked)", bl.commands.len()),
                },
                Err(e) => CheckResult {
                    name: "blocklist.json".to_string(),
                    status: CheckStatus::Fail,
                    message: format!("Invalid JSON: {}", e),
                },
            }
        }
        Err(e) => CheckResult {
            name: "blocklist.json".to_string(),
            status: CheckStatus::Fail,
            message: format!("Cannot read: {}", e),
        },
    }
}

/// Check if an executable can be found (absolute path or in PATH)
fn which_executable(executable: &str) -> bool {
    let path = Path::new(executable);

    // If it's an absolute or relative path, check directly
    if executable.contains('/') || executable.contains('\\') {
        return path.exists();
    }

    // Search in PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(executable);
            if candidate.exists() {
                return true;
            }
            // On Windows, also check with common extensions
            if cfg!(target_os = "windows") {
                for ext in &[".exe", ".cmd", ".bat", ".com"] {
                    let with_ext = dir.join(format!("{}{}", executable, ext));
                    if with_ext.exists() {
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
    use crate::registry::models::{ToolDefinition, TransportType};
    use crate::registry::store::InMemoryRegistry;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_tool(name: &str, command: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            command: command.to_string(),
            description: format!("Test tool {}", name),
            params: vec![],
            transport: TransportType::Stdio,
            registered_at: Utc::now(),
        }
    }

    #[test]
    fn test_check_tools_dir_exists() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let result = check_tools_dir(&tools_path);
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_tools_dir_missing() {
        let result = check_tools_dir(Path::new("/nonexistent/path/tools.json"));
        assert_eq!(result.status, CheckStatus::Fail);
    }

    #[test]
    fn test_check_tools_file_missing() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let result = check_tools_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_check_tools_file_valid() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(&tools_path, r#"{"version": 1, "tools": []}"#).unwrap();
        let result = check_tools_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_tools_file_invalid_json() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(&tools_path, "not json").unwrap();
        let result = check_tools_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Fail);
        assert!(result.message.contains("Invalid JSON"));
    }

    #[test]
    fn test_check_cli_tools_empty() {
        let registry = InMemoryRegistry::new();
        let results = check_cli_tools(&registry).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Warn);
    }

    #[test]
    fn test_check_cli_tools_valid_command() {
        let registry = InMemoryRegistry::new();
        registry.add(make_tool("echo_test", "echo")).unwrap();
        let results = check_cli_tools(&registry).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_cli_tools_missing_command() {
        let registry = InMemoryRegistry::new();
        registry
            .add(make_tool("bad_tool", "nonexistent_binary_xyz_999"))
            .unwrap();
        let results = check_cli_tools(&registry).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CheckStatus::Fail);
        assert!(results[0].message.contains("not found"));
    }

    #[test]
    fn test_check_api_tools_file_missing() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let result = check_api_tools_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_check_api_tools_file_valid() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(
            tmp.path().join("api_tools.json"),
            r#"{"version": 1, "tools": []}"#,
        )
        .unwrap();
        let result = check_api_tools_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Pass);
    }

    #[test]
    fn test_check_blocklist_file_missing() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let result = check_blocklist_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Warn);
    }

    #[test]
    fn test_check_blocklist_file_valid() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(
            tmp.path().join("blocklist.json"),
            r#"{"commands": [{"command": "rm", "reason": "Dangerous"}]}"#,
        )
        .unwrap();
        let result = check_blocklist_file(&tools_path);
        assert_eq!(result.status, CheckStatus::Pass);
        assert!(result.message.contains("1 commands blocked"));
    }

    #[test]
    fn test_which_executable_found() {
        assert!(which_executable("echo"));
    }

    #[test]
    fn test_which_executable_not_found() {
        assert!(!which_executable("nonexistent_binary_xyz_999"));
    }

    #[test]
    fn test_which_executable_absolute_path() {
        // /bin/echo exists on macOS/Linux
        if cfg!(not(target_os = "windows")) {
            assert!(which_executable("/bin/echo"));
            assert!(!which_executable("/nonexistent/path/to/binary"));
        }
    }

    #[test]
    fn test_diagnose_empty_setup() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        let registry = InMemoryRegistry::new();
        let results = diagnose(&tools_path, &registry, None).unwrap();
        // Should have: dir check, tools.json check, cli tools check, api_tools check, blocklist check
        assert!(results.len() >= 5);
    }

    #[test]
    fn test_diagnose_mixed_results() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(&tools_path, r#"{"version": 1, "tools": []}"#).unwrap();

        let registry = InMemoryRegistry::new();
        registry.add(make_tool("good_tool", "echo")).unwrap();
        registry
            .add(make_tool("bad_tool", "nonexistent_xyz_999"))
            .unwrap();

        let results = diagnose(&tools_path, &registry, None).unwrap();

        let has_pass = results.iter().any(|r| r.status == CheckStatus::Pass);
        let has_fail = results.iter().any(|r| r.status == CheckStatus::Fail);
        assert!(has_pass);
        assert!(has_fail);
    }

    #[test]
    fn test_run_returns_exit_code() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(&tools_path, r#"{"version": 1, "tools": []}"#).unwrap();

        let registry = InMemoryRegistry::new();
        // All checks pass or warn (no fails)
        let exit_code = run(&tools_path, &registry, None).unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_run_returns_failure_exit_code() {
        let tmp = TempDir::new().unwrap();
        let tools_path = tmp.path().join("tools.json");
        std::fs::write(&tools_path, "invalid json!!").unwrap();

        let registry = InMemoryRegistry::new();
        let exit_code = run(&tools_path, &registry, None).unwrap();
        assert_eq!(exit_code, 1);
    }

    #[test]
    fn test_check_api_tool_env_vars_no_registry() {
        let results = check_api_tool_env_vars(None).unwrap();
        assert!(results.is_empty());
    }
}
