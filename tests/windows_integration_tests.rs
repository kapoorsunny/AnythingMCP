// Windows-specific integration tests using .cmd fixture scripts.
#![cfg(target_os = "windows")]

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
    let echo_path = fixture_path("echo_tool.cmd");
    cmd_with_tools_dir(&tmp)
        .args([
            "register", "echo_test",
            "--cmd", echo_path.to_str().unwrap(),
            "--type", "sse",
            "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Registered tool"));
}

#[test]
fn test_register_and_list() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.cmd");

    cmd_with_tools_dir(&tmp)
        .args([
            "register", "echo_test",
            "--cmd", echo_path.to_str().unwrap(),
            "--type", "sse",
            "--force",
        ])
        .assert()
        .success();

    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("echo_test"));
}

#[test]
fn test_register_and_remove() {
    let tmp = TempDir::new().unwrap();
    let echo_path = fixture_path("echo_tool.cmd");

    cmd_with_tools_dir(&tmp)
        .args([
            "register", "echo_test",
            "--cmd", echo_path.to_str().unwrap(),
            "--type", "sse",
            "--force",
        ])
        .assert()
        .success();

    cmd_with_tools_dir(&tmp)
        .args(["remove", "echo_test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed"));
}

#[test]
fn test_remove_nonexistent() {
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
    let echo_path = fixture_path("echo_tool.cmd");

    cmd_with_tools_dir(&tmp)
        .args([
            "register", "tool1",
            "--cmd", echo_path.to_str().unwrap(),
            "--type", "sse", "--force",
        ])
        .assert()
        .success();

    cmd_with_tools_dir(&tmp)
        .args([
            "register", "tool2",
            "--cmd", echo_path.to_str().unwrap(),
            "--type", "stdio", "--force",
        ])
        .assert()
        .success();

    cmd_with_tools_dir(&tmp)
        .args(["remove", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed all"));

    cmd_with_tools_dir(&tmp)
        .args(["list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No tools registered"));
}

#[test]
fn test_register_no_help_tool() {
    let tmp = TempDir::new().unwrap();
    let tool_path = fixture_path("no_help_tool.cmd");
    cmd_with_tools_dir(&tmp)
        .args([
            "register", "no_help",
            "--cmd", tool_path.to_str().unwrap(),
            "--type", "sse", "--force",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("0 parameters"));
}

#[test]
fn test_register_blocked_command() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["register", "bad_tool", "--cmd", "rm", "--type", "sse", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Blocked"));
}

#[test]
fn test_block_custom_command() {
    let tmp = TempDir::new().unwrap();

    cmd_with_tools_dir(&tmp)
        .args(["block", "notepad", "--reason", "Not allowed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Blocked command"));

    cmd_with_tools_dir(&tmp)
        .args(["register", "editor", "--cmd", "notepad", "--type", "sse", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not allowed"));

    cmd_with_tools_dir(&tmp)
        .args(["unblock", "notepad"])
        .assert()
        .success();
}

#[test]
fn test_block_list() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["block", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rm"))
        .stdout(predicate::str::contains("shutdown"));
}

#[test]
fn test_inspect_not_found() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["inspect", "nonexistent"])
        .assert()
        .failure();
}

#[test]
fn test_test_tool_not_found() {
    let tmp = TempDir::new().unwrap();
    cmd_with_tools_dir(&tmp)
        .args(["test", "nonexistent"])
        .assert()
        .failure();
}
