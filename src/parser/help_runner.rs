use std::process::Command;

use crate::error::{McpWrapError, Result};

pub trait HelpRunner: Send + Sync {
    /// Runs help discovery for a command using platform-aware fallback chain:
    ///
    /// 1. `--help` (all platforms)
    /// 2. `-h` (all platforms)
    /// 3. `man <cmd>` (macOS/Linux only)
    /// 4. `/?` (Windows only)
    ///
    /// Returns empty string if all attempts produce no output.
    fn run_help(&self, command: &str) -> Result<String>;
}

#[derive(Default)]
pub struct ProcessHelpRunner;

impl ProcessHelpRunner {
    pub fn new() -> Self {
        Self
    }

    const MAX_OUTPUT_BYTES: usize = 65_536; // 64 KB

    fn run_command_with_flag(&self, tokens: &[String], flag: &str) -> Result<Option<String>> {
        let executable = &tokens[0];
        let static_args = &tokens[1..];

        let mut cmd = Command::new(executable);
        cmd.args(static_args);
        cmd.arg(flag);

        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let child = match child {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(McpWrapError::CommandNotFound(executable.clone()));
            }
            Err(e) => return Err(e.into()),
        };

        let handle = std::thread::spawn(move || child.wait_with_output());

        match handle.join() {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(
                    &output.stdout[..output.stdout.len().min(Self::MAX_OUTPUT_BYTES)],
                )
                .to_string();
                let stderr = String::from_utf8_lossy(
                    &output.stderr[..output.stderr.len().min(Self::MAX_OUTPUT_BYTES)],
                )
                .to_string();

                // Prefer stdout if non-empty
                if !stdout.trim().is_empty() {
                    Ok(Some(stdout))
                } else if !stderr.trim().is_empty() {
                    // Only use stderr as help if:
                    // - Command exited 0 (some tools print help to stderr), OR
                    // - stderr looks like actual help (contains options/flags patterns)
                    let is_success = output.status.success();
                    let looks_like_help = stderr.contains("--")
                        || stderr.to_lowercase().contains("usage")
                        || stderr.to_lowercase().contains("options:");
                    if is_success || looks_like_help {
                        Ok(Some(stderr))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Ok(None),
        }
    }

    /// Run `man <executable>` and capture the output.
    /// Only available on macOS/Linux. Uses `col -bx` to strip formatting.
    #[cfg(not(target_os = "windows"))]
    fn run_man(&self, executable: &str) -> Result<Option<String>> {
        // man outputs formatted text; pipe through col -bx to strip control chars
        let child = Command::new("man")
            .arg(executable)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("COLUMNS", "200") // wide output to avoid wrapping
            .env("MAN_KEEP_FORMATTING", "0")
            .spawn();

        let child = match child {
            Ok(c) => c,
            Err(_) => return Ok(None), // man not available
        };

        let handle = std::thread::spawn(move || child.wait_with_output());

        match handle.join() {
            Ok(Ok(output)) => {
                if !output.status.success() {
                    return Ok(None); // no man page for this command
                }

                let raw = &output.stdout[..output.stdout.len().min(Self::MAX_OUTPUT_BYTES)];

                // Strip man page formatting using col -bx
                let col_result = Command::new("col")
                    .args(["-bx"])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .and_then(|mut col| {
                        use std::io::Write;
                        if let Some(ref mut stdin) = col.stdin {
                            let _ = stdin.write_all(raw);
                        }
                        col.wait_with_output()
                    });

                let text = match col_result {
                    Ok(col_output) => String::from_utf8_lossy(&col_output.stdout).to_string(),
                    Err(_) => {
                        // col not available, use raw output with basic cleanup
                        let raw_str = String::from_utf8_lossy(raw).to_string();
                        // Strip common backspace-based formatting (char + backspace + char)
                        strip_backspace_formatting(&raw_str)
                    }
                };

                if text.trim().is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(text))
                }
            }
            Ok(Err(_)) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    /// Run `<executable> /?` for Windows help.
    #[cfg(target_os = "windows")]
    fn run_windows_help(&self, tokens: &[String]) -> Result<Option<String>> {
        let executable = &tokens[0];
        let static_args = &tokens[1..];

        let mut cmd = Command::new(executable);
        cmd.args(static_args);
        cmd.arg("/?");

        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn();

        let child = match child {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let handle = std::thread::spawn(move || child.wait_with_output());

        match handle.join() {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(
                    &output.stdout[..output.stdout.len().min(Self::MAX_OUTPUT_BYTES)],
                )
                .to_string();
                let stderr = String::from_utf8_lossy(
                    &output.stderr[..output.stderr.len().min(Self::MAX_OUTPUT_BYTES)],
                )
                .to_string();

                if !stdout.trim().is_empty() {
                    Ok(Some(stdout))
                } else if !stderr.trim().is_empty() {
                    Ok(Some(stderr))
                } else {
                    Ok(None)
                }
            }
            Ok(Err(_)) => Ok(None),
            Err(_) => Ok(None),
        }
    }
}

/// Strip backspace-based man page formatting (e.g., "c\x08c" for bold).
#[cfg(not(target_os = "windows"))]
fn strip_backspace_formatting(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());

    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i + 1] == 0x08 {
            // char + backspace + char: skip the first char and backspace
            i += 2;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }

    String::from_utf8_lossy(&result).to_string()
}

impl HelpRunner for ProcessHelpRunner {
    fn run_help(&self, command: &str) -> Result<String> {
        let tokens = shell_words::split(command).map_err(|e| McpWrapError::HelpParseFailed {
            cmd: command.to_string(),
            reason: format!("Failed to parse command: {}", e),
        })?;

        if tokens.is_empty() {
            return Err(McpWrapError::HelpParseFailed {
                cmd: command.to_string(),
                reason: "Empty command".to_string(),
            });
        }

        // 1. Try --help (all platforms)
        if let Some(output) = self.run_command_with_flag(&tokens, "--help")? {
            return Ok(output);
        }

        // 2. Try -h (all platforms)
        if let Some(output) = self.run_command_with_flag(&tokens, "-h")? {
            return Ok(output);
        }

        // 3. Platform-specific fallbacks
        #[cfg(not(target_os = "windows"))]
        {
            // Try man page (macOS/Linux)
            let executable = &tokens[0];
            // Extract just the command name from the path for man lookup
            let cmd_name = std::path::Path::new(executable)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(executable);

            if let Ok(Some(output)) = self.run_man(cmd_name) {
                return Ok(output);
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Try /? (Windows)
            if let Ok(Some(output)) = self.run_windows_help(&tokens) {
                return Ok(output);
            }
        }

        // All attempts returned empty
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_help_runner_with_echo() {
        let runner = ProcessHelpRunner::new();
        let result = runner.run_help("echo");
        assert!(result.is_ok());
    }

    #[test]
    fn test_process_help_runner_command_not_found() {
        let runner = ProcessHelpRunner::new();
        let result = runner.run_help("nonexistent_command_xyz_123");
        assert!(result.is_err());
        match result.unwrap_err() {
            McpWrapError::CommandNotFound(cmd) => {
                assert_eq!(cmd, "nonexistent_command_xyz_123");
            }
            e => panic!("Expected CommandNotFound, got: {:?}", e),
        }
    }

    #[test]
    fn test_process_help_runner_empty_command() {
        let runner = ProcessHelpRunner::new();
        let result = runner.run_help("");
        assert!(result.is_err());
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_man_page_fallback() {
        // `ls` typically has a man page but may not have --help on all systems
        let runner = ProcessHelpRunner::new();
        let result = runner.run_man("ls");
        assert!(result.is_ok());
        // On macOS/Linux, ls should have a man page
        if let Ok(Some(text)) = result {
            assert!(!text.is_empty());
            // Man page should mention something about listing
            let lower = text.to_lowercase();
            assert!(
                lower.contains("list") || lower.contains("directory") || lower.contains("ls"),
                "Man page output should mention ls/list/directory"
            );
        }
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_man_page_not_found() {
        let runner = ProcessHelpRunner::new();
        let result = runner.run_man("definitely_not_a_real_command_xyz");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_strip_backspace_formatting() {
        // "H\x08He\x08el\x08l" represents bold "Hel"
        let input = "H\x08He\x08el\x08lp";
        let result = strip_backspace_formatting(input);
        assert_eq!(result, "Help");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_man_fallback_discovers_flags() {
        // Full integration: run_help on a command that has a man page
        // `grep` is available on all Unix systems and has --flags in its man page
        let runner = ProcessHelpRunner::new();
        let help = runner.run_help("grep").unwrap();
        // grep's --help or man page should produce some flag output
        assert!(!help.is_empty(), "grep should produce help output");
    }
}
