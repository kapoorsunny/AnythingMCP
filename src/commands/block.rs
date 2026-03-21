use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{McpWrapError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedCommand {
    pub command: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlocklistFile {
    pub commands: Vec<BlockedCommand>,
}

impl Default for BlocklistFile {
    fn default() -> Self {
        Self {
            commands: vec![
                BlockedCommand {
                    command: "rm".into(),
                    reason: "Destructive: removes files/directories".into(),
                },
                BlockedCommand {
                    command: "rmdir".into(),
                    reason: "Destructive: removes directories".into(),
                },
                BlockedCommand {
                    command: "shred".into(),
                    reason: "Destructive: securely deletes files".into(),
                },
                BlockedCommand {
                    command: "shutdown".into(),
                    reason: "System: shuts down the machine".into(),
                },
                BlockedCommand {
                    command: "reboot".into(),
                    reason: "System: reboots the machine".into(),
                },
                BlockedCommand {
                    command: "halt".into(),
                    reason: "System: halts the machine".into(),
                },
                BlockedCommand {
                    command: "poweroff".into(),
                    reason: "System: powers off the machine".into(),
                },
                BlockedCommand {
                    command: "init".into(),
                    reason: "System: changes run level".into(),
                },
                BlockedCommand {
                    command: "mkfs".into(),
                    reason: "Destructive: formats a filesystem".into(),
                },
                BlockedCommand {
                    command: "dd".into(),
                    reason: "Destructive: raw disk write, can overwrite data".into(),
                },
                BlockedCommand {
                    command: "fdisk".into(),
                    reason: "Destructive: modifies disk partitions".into(),
                },
                BlockedCommand {
                    command: "parted".into(),
                    reason: "Destructive: modifies disk partitions".into(),
                },
                BlockedCommand {
                    command: "chmod".into(),
                    reason: "Security: changes file permissions".into(),
                },
                BlockedCommand {
                    command: "chown".into(),
                    reason: "Security: changes file ownership".into(),
                },
                BlockedCommand {
                    command: "iptables".into(),
                    reason: "Security: modifies firewall rules".into(),
                },
                BlockedCommand {
                    command: "kill".into(),
                    reason: "Destructive: terminates processes".into(),
                },
                BlockedCommand {
                    command: "killall".into(),
                    reason: "Destructive: terminates processes by name".into(),
                },
                BlockedCommand {
                    command: "pkill".into(),
                    reason: "Destructive: terminates processes by pattern".into(),
                },
                BlockedCommand {
                    command: "bash".into(),
                    reason: "Unsafe: arbitrary shell execution".into(),
                },
                BlockedCommand {
                    command: "sh".into(),
                    reason: "Unsafe: arbitrary shell execution".into(),
                },
                BlockedCommand {
                    command: "zsh".into(),
                    reason: "Unsafe: arbitrary shell execution".into(),
                },
            ],
        }
    }
}

/// Load the blocklist from disk, or create default if missing.
pub fn load_blocklist(tools_path: &Path) -> Result<BlocklistFile> {
    let blocklist_path = blocklist_path(tools_path);

    if blocklist_path.exists() {
        let content = fs::read_to_string(&blocklist_path)?;
        let file: BlocklistFile = serde_json::from_str(&content)?;
        Ok(file)
    } else {
        // Create default blocklist
        let default = BlocklistFile::default();
        save_blocklist(tools_path, &default)?;
        Ok(default)
    }
}

fn save_blocklist(tools_path: &Path, blocklist: &BlocklistFile) -> Result<()> {
    let path = blocklist_path(tools_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(blocklist)?;
    fs::write(path, json)?;
    Ok(())
}

fn blocklist_path(tools_path: &Path) -> std::path::PathBuf {
    tools_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| tools_path.to_path_buf())
        .join("blocklist.json")
}

/// Check if a command is blocked. Returns the reason if blocked.
pub fn is_blocked(tools_path: &Path, cmd: &str) -> Option<String> {
    let blocklist = load_blocklist(tools_path).unwrap_or_default();
    let executable = cmd
        .split_whitespace()
        .next()
        .map(|t| t.rsplit('/').next().unwrap_or(t))
        .unwrap_or("");

    blocklist
        .commands
        .iter()
        .find(|b| b.command == executable)
        .map(|b| b.reason.clone())
}

/// Add a command to the blocklist and remove any already-registered tools using it.
pub fn block(
    tools_path: &Path,
    command: &str,
    reason: &str,
    registry: &dyn crate::registry::store::ToolRegistry,
) -> Result<()> {
    let mut blocklist = load_blocklist(tools_path)?;

    // Check if already blocked
    if blocklist.commands.iter().any(|b| b.command == command) {
        println!("'{}' is already blocked.", command);
        return Ok(());
    }

    blocklist.commands.push(BlockedCommand {
        command: command.to_string(),
        reason: reason.to_string(),
    });

    save_blocklist(tools_path, &blocklist)?;

    // Remove any already-registered CLI tools using this command
    let tools = registry.list()?;
    let mut removed = 0;
    for tool in &tools {
        let exe = tool
            .command
            .split_whitespace()
            .next()
            .map(|t| t.rsplit('/').next().unwrap_or(t))
            .unwrap_or("");
        if exe == command {
            registry.remove(&tool.name)?;
            removed += 1;
        }
    }

    if removed > 0 {
        println!(
            "\u{2714} Blocked command '{}' (removed {} registered tool{})",
            command,
            removed,
            if removed == 1 { "" } else { "s" }
        );
    } else {
        println!("\u{2714} Blocked command '{}'", command);
    }
    Ok(())
}

/// Remove a command from the blocklist.
pub fn unblock(tools_path: &Path, command: &str) -> Result<()> {
    let mut blocklist = load_blocklist(tools_path)?;

    let before = blocklist.commands.len();
    blocklist.commands.retain(|b| b.command != command);

    if blocklist.commands.len() == before {
        return Err(McpWrapError::RegistryError(format!(
            "'{}' is not in the blocklist.",
            command
        )));
    }

    save_blocklist(tools_path, &blocklist)?;
    println!("\u{2714} Unblocked command '{}'", command);
    Ok(())
}

/// List all blocked commands.
pub fn list_blocked(tools_path: &Path) -> Result<()> {
    let blocklist = load_blocklist(tools_path)?;

    if blocklist.commands.is_empty() {
        println!("No commands blocked. Use 'mcpw block <command>' to add one.");
        return Ok(());
    }

    println!("{:<16}REASON", "COMMAND");
    println!("{}", "\u{2500}".repeat(60));

    for entry in &blocklist.commands {
        println!("{:<16}{}", entry.command, entry.reason);
    }

    Ok(())
}

/// Reset blocklist to defaults.
pub fn reset_blocklist(tools_path: &Path) -> Result<()> {
    let default = BlocklistFile::default();
    save_blocklist(tools_path, &default)?;
    println!(
        "\u{2714} Reset blocklist to defaults ({} commands)",
        default.commands.len()
    );
    Ok(())
}
