use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn cmd_with_tools_dir(tmp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("mcpw").unwrap();
    cmd.env("MCPW_TOOLS_DIR", tmp.path());
    cmd
}

// Note: These integration tests use MCPW_TOOLS_DIR env var to override
// the default ~/.mcpw path. We need to add support for this in main.rs.

#[test]
fn test_list_empty() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tools registered"));
}

#[test]
fn test_register_echo_tool() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "echo_test",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Test echo tool",
            "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Registered tool 'echo_test'"));
}

#[test]
fn test_register_and_list() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "my_echo",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Echo tool",
            "--force",
        ])
        .assert()
        .success();

    // List
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my_echo"));
}

#[test]
fn test_register_force_overwrite() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register first time
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "overwrite_test",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Original description",
            "--force",
        ])
        .assert()
        .success();

    // Register again with --force
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "overwrite_test",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Updated description",
            "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Registered tool 'overwrite_test'"));
}

#[test]
fn test_remove_tool() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "to_remove",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Will be removed",
            "--force",
        ])
        .assert()
        .success();

    // Remove
    cmd_with_tools_dir(&tmp)
        .args(["remove", "to_remove"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed tool 'to_remove'"));

    // Verify gone
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tools registered"));
}

#[test]
fn test_remove_not_found() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["remove", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_remove_all() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register two tools
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "tool_a",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Tool A",
            "--force",
        ])
        .assert()
        .success();

    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "tool_b",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Tool B",
            "--force",
        ])
        .assert()
        .success();

    // Verify 2 tools exist
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tool_a"))
        .stdout(predicate::str::contains("tool_b"));

    // Remove all
    cmd_with_tools_dir(&tmp)
        .args(["remove", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed all tools"));

    // Verify empty
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tools registered"));
}

#[test]
fn test_remove_no_name_no_all() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp).args(["remove"]).assert().failure();
}

#[test]
fn test_register_blocked_command() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args([
            "register", "bad_tool", "--cmd", "rm", "--type", "sse", "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Blocked"));
}

#[test]
fn test_register_blocked_with_allow_unsafe() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "safe_echo",
            "--cmd",
            "echo test",
            "--type",
            "sse",
            "--force",
            "--allow-unsafe",
        ])
        .assert()
        .success();
}

#[test]
fn test_block_custom_command() {
    let tmp = TempDir::new().unwrap();

    // Block curl
    cmd_with_tools_dir(&tmp)
        .args(["block", "curl", "--reason", "No HTTP"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blocked command 'curl'"));

    // Try registering — should fail
    cmd_with_tools_dir(&tmp)
        .args([
            "register", "http", "--cmd", "curl", "--type", "sse", "--force",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No HTTP"));

    // Unblock
    cmd_with_tools_dir(&tmp)
        .args(["unblock", "curl"])
        .assert()
        .success();

    // Now register works
    cmd_with_tools_dir(&tmp)
        .args([
            "register", "http", "--cmd", "curl", "--type", "sse", "--force",
        ])
        .assert()
        .success();
}

#[test]
fn test_block_removes_registered_tool() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register a tool using python3
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "my_script",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--force",
        ])
        .assert()
        .success();

    // Verify it's registered
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my_script"));

    // Block python3
    cmd_with_tools_dir(&tmp)
        .args(["block", "python3", "--reason", "No Python"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed 1 registered tool"));

    // Verify it's gone
    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tools registered"));
}

#[test]
fn test_block_list_and_reset() {
    let tmp = TempDir::new().unwrap();

    // List defaults
    cmd_with_tools_dir(&tmp)
        .args(["block", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rm"))
        .stdout(predicate::str::contains("shutdown"));

    // Reset
    cmd_with_tools_dir(&tmp)
        .args(["block", "--reset"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Reset blocklist"));
}

#[test]
fn test_register_no_help_tool() {
    let tmp = TempDir::new().unwrap();
    let tool_path = fixture_path("no_help_tool.sh");

    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "no_help",
            "--cmd",
            &tool_path.to_str().unwrap(),
            "--desc",
            "No help tool",
            "--force",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "No --flag-style parameters detected",
        ));
}

#[test]
fn test_register_help_on_stderr() {
    let tmp = TempDir::new().unwrap();
    let tool_path = fixture_path("help_on_stderr_tool.sh");

    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "stderr_help",
            "--cmd",
            &tool_path.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    // Inspect to verify params were parsed from stderr
    cmd_with_tools_dir(&tmp)
        .args(["inspect", "stderr_help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("input"))
        .stdout(predicate::str::contains("output"));
}

#[test]
fn test_inspect_tool() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "inspect_test",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Inspect test tool",
            "--force",
        ])
        .assert()
        .success();

    // Inspect
    cmd_with_tools_dir(&tmp)
        .args(["inspect", "inspect_test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tool: inspect_test"))
        .stdout(predicate::str::contains("Inspect test tool"));
}

#[test]
fn test_inspect_not_found() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["inspect", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_test_command_success() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "echo_for_test",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--force",
        ])
        .assert()
        .success();

    // Test with args
    cmd_with_tools_dir(&tmp)
        .args([
            "test",
            "echo_for_test",
            "--args",
            r#"{"message": "hello test"}"#,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tool Schema"))
        .stdout(predicate::str::contains("Request"))
        .stdout(predicate::str::contains("Result: OK"))
        .stdout(predicate::str::contains("hello test"));
}

#[test]
fn test_test_command_error() {
    let tmp = TempDir::new().unwrap();
    let fail_path = fixture_path("fail_tool.sh");

    // Register
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "fail_for_test",
            "--cmd",
            &fail_path.to_str().unwrap(),
            "--desc",
            "Always fails",
            "--force",
        ])
        .assert()
        .success();

    // Test - should fail
    cmd_with_tools_dir(&tmp)
        .args(["test", "fail_for_test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("fatal error"));
}

#[test]
fn test_test_command_not_found() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["test", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_serve_stdio_tool_call() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register tool first
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "echo_stdio",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Echo for stdio test",
            "--force",
        ])
        .assert()
        .success();

    // Start stdio server and send requests
    let bin_path = assert_cmd::cargo::cargo_bin("mcpw");
    let mut child = Command::new(bin_path)
        .args(["serve"])
        .env("MCPW_TOOLS_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start serve");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");

        // Send initialize
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        writeln!(stdin, "{}", serde_json::to_string(&init_req).unwrap()).unwrap();

        // Send tools/list
        let list_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });
        writeln!(stdin, "{}", serde_json::to_string(&list_req).unwrap()).unwrap();

        // Send tools/call
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "echo_stdio",
                "arguments": {
                    "message": "hello from test"
                }
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&call_req).unwrap()).unwrap();
    }
    // Close stdin to end the server
    child.stdin.take();

    let output = child.wait_with_output().expect("Failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse responses
    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    assert!(
        responses.len() >= 3,
        "Expected at least 3 responses, got {}: {}",
        responses.len(),
        stdout
    );

    // Check initialize response
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "mcpw");

    // Check tools/list response
    let tools = responses[1]["result"]["tools"].as_array().unwrap();
    assert!(!tools.is_empty());
    assert_eq!(tools[0]["name"], "echo_stdio");

    // Check tools/call response
    assert!(responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("hello from test"));
}

#[test]
fn test_tool_failure_returns_mcp_error() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tmp = TempDir::new().unwrap();
    let fail_path = fixture_path("fail_tool.sh");

    // Register fail tool
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "fail_tool",
            "--cmd",
            &fail_path.to_str().unwrap(),
            "--desc",
            "Always fails",
            "--force",
        ])
        .assert()
        .success();

    // Start stdio server
    let bin_path = assert_cmd::cargo::cargo_bin("mcpw");
    let mut child = Command::new(bin_path)
        .args(["serve"])
        .env("MCPW_TOOLS_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start serve");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");

        // Initialize
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        writeln!(stdin, "{}", serde_json::to_string(&init_req).unwrap()).unwrap();

        // Call fail tool
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "fail_tool",
                "arguments": {}
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&call_req).unwrap()).unwrap();
    }
    child.stdin.take();

    let output = child.wait_with_output().expect("Failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    assert!(responses.len() >= 2);

    // The call should return isError: true with stderr content
    assert_eq!(responses[1]["result"]["isError"], true);
    assert!(responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("fatal error"));
}

#[test]
fn test_shell_injection_integration() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.py");

    // Register echo tool
    cmd_with_tools_dir(&tmp)
        .args([
            "register",
            "echo_inject",
            "--cmd",
            &format!("python3 {}", echo_path.display()),
            "--desc",
            "Echo for injection test",
            "--force",
        ])
        .assert()
        .success();

    // Start stdio server
    let bin_path = assert_cmd::cargo::cargo_bin("mcpw");
    let mut child = Command::new(bin_path)
        .args(["serve"])
        .env("MCPW_TOOLS_DIR", tmp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start serve");

    {
        let stdin = child.stdin.as_mut().expect("Failed to open stdin");

        // Initialize
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });
        writeln!(stdin, "{}", serde_json::to_string(&init_req).unwrap()).unwrap();

        // Call with injection attempt
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "echo_inject",
                "arguments": {
                    "message": "; rm -rf /"
                }
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&call_req).unwrap()).unwrap();
    }
    child.stdin.take();

    let output = child.wait_with_output().expect("Failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let responses: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    assert!(responses.len() >= 2);

    // The injection string should be echoed literally, not interpreted
    let text = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("");
    assert!(
        text.contains("; rm -rf /"),
        "Expected literal injection string in output, got: {}",
        text
    );
}
