use crate::error::{McpWrapError, Result};

/// Supported MCP clients
#[derive(Debug, Clone, PartialEq)]
pub enum ClientType {
    ClaudeDesktop,
    ClaudeCode,
    Cursor,
    Vscode,
}

impl ClientType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "claude-desktop" | "claude_desktop" | "desktop" => Ok(ClientType::ClaudeDesktop),
            "claude-code" | "claude_code" | "code" => Ok(ClientType::ClaudeCode),
            "cursor" => Ok(ClientType::Cursor),
            "vscode" | "vs-code" | "vs_code" => Ok(ClientType::Vscode),
            _ => Err(McpWrapError::RegistryError(format!(
                "Unknown client '{}'. Supported: claude-desktop, claude-code, cursor, vscode",
                s
            ))),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            ClientType::ClaudeDesktop => "Claude Desktop",
            ClientType::ClaudeCode => "Claude Code",
            ClientType::Cursor => "Cursor",
            ClientType::Vscode => "VS Code",
        }
    }
}

/// Generate the JSON config snippet for a given client
pub fn generate_config(
    client: &ClientType,
    mcpw_path: &str,
    port: u16,
    progressive: bool,
) -> String {
    let mut args = vec!["serve".to_string()];
    if port != 3000 {
        args.push("--port".to_string());
        args.push(port.to_string());
    }
    if progressive {
        args.push("--progressive".to_string());
    }

    let args_json: Vec<String> = args.iter().map(|a| format!("\"{}\"", a)).collect();
    let args_str = args_json.join(", ");

    match client {
        ClientType::ClaudeDesktop | ClientType::Cursor => {
            format!(
                r#"{{
  "mcpServers": {{
    "mcpw": {{
      "command": "{}",
      "args": [{}]
    }}
  }}
}}"#,
                mcpw_path, args_str
            )
        }
        ClientType::ClaudeCode => {
            // Claude Code uses `claude mcp add` CLI command
            let args_flat: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            format!(
                "claude mcp add mcpw -- {} {}",
                mcpw_path,
                args_flat.join(" ")
            )
        }
        ClientType::Vscode => {
            format!(
                r#"{{
  "mcp": {{
    "servers": {{
      "mcpw": {{
        "command": "{}",
        "args": [{}]
      }}
    }}
  }}
}}"#,
                mcpw_path, args_str
            )
        }
    }
}

/// Get the config file location hint for a client
pub fn config_location(client: &ClientType) -> &str {
    match client {
        ClientType::ClaudeDesktop => {
            if cfg!(target_os = "macos") {
                "~/Library/Application Support/Claude/claude_desktop_config.json"
            } else if cfg!(target_os = "windows") {
                "%APPDATA%\\Claude\\claude_desktop_config.json"
            } else {
                "~/.config/Claude/claude_desktop_config.json"
            }
        }
        ClientType::ClaudeCode => "Run the command below to add mcpw to Claude Code",
        ClientType::Cursor => {
            if cfg!(target_os = "macos") {
                "~/.cursor/mcp.json"
            } else if cfg!(target_os = "windows") {
                "%USERPROFILE%\\.cursor\\mcp.json"
            } else {
                "~/.cursor/mcp.json"
            }
        }
        ClientType::Vscode => ".vscode/mcp.json (workspace) or User settings",
    }
}

/// Run the config command
pub fn run(client_name: &str, mcpw_path: Option<&str>, port: u16, progressive: bool) -> Result<()> {
    let client = ClientType::from_str(client_name)?;

    let path = mcpw_path
        .map(|s| s.to_string())
        .unwrap_or_else(find_mcpw_path);

    let config = generate_config(&client, &path, port, progressive);
    let location = config_location(&client);

    println!("Configuration for {}:", client.display_name());
    println!("Location: {}", location);
    println!();
    println!("{}", config);

    Ok(())
}

/// Try to find the mcpw binary path
fn find_mcpw_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "mcpw".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_type_from_str_claude_desktop() {
        assert_eq!(
            ClientType::from_str("claude-desktop").unwrap(),
            ClientType::ClaudeDesktop
        );
        assert_eq!(
            ClientType::from_str("desktop").unwrap(),
            ClientType::ClaudeDesktop
        );
    }

    #[test]
    fn test_client_type_from_str_claude_code() {
        assert_eq!(
            ClientType::from_str("claude-code").unwrap(),
            ClientType::ClaudeCode
        );
        assert_eq!(
            ClientType::from_str("code").unwrap(),
            ClientType::ClaudeCode
        );
    }

    #[test]
    fn test_client_type_from_str_cursor() {
        assert_eq!(ClientType::from_str("cursor").unwrap(), ClientType::Cursor);
    }

    #[test]
    fn test_client_type_from_str_vscode() {
        assert_eq!(ClientType::from_str("vscode").unwrap(), ClientType::Vscode);
        assert_eq!(ClientType::from_str("vs-code").unwrap(), ClientType::Vscode);
    }

    #[test]
    fn test_client_type_from_str_unknown() {
        assert!(ClientType::from_str("unknown_client").is_err());
    }

    #[test]
    fn test_client_type_case_insensitive() {
        assert_eq!(
            ClientType::from_str("CLAUDE-DESKTOP").unwrap(),
            ClientType::ClaudeDesktop
        );
        assert_eq!(ClientType::from_str("Cursor").unwrap(), ClientType::Cursor);
    }

    #[test]
    fn test_generate_config_claude_desktop_defaults() {
        let config = generate_config(&ClientType::ClaudeDesktop, "mcpw", 3000, false);
        assert!(config.contains("\"mcpServers\""));
        assert!(config.contains("\"mcpw\""));
        assert!(config.contains("\"command\": \"mcpw\""));
        assert!(config.contains("\"serve\""));
        assert!(!config.contains("--port"));
        assert!(!config.contains("--progressive"));
    }

    #[test]
    fn test_generate_config_claude_desktop_custom_port() {
        let config = generate_config(&ClientType::ClaudeDesktop, "mcpw", 8080, false);
        assert!(config.contains("\"--port\""));
        assert!(config.contains("\"8080\""));
    }

    #[test]
    fn test_generate_config_claude_desktop_progressive() {
        let config = generate_config(&ClientType::ClaudeDesktop, "mcpw", 3000, true);
        assert!(config.contains("\"--progressive\""));
    }

    #[test]
    fn test_generate_config_claude_code() {
        let config = generate_config(&ClientType::ClaudeCode, "mcpw", 3000, false);
        assert!(config.contains("claude mcp add mcpw"));
        assert!(config.contains("mcpw serve"));
    }

    #[test]
    fn test_generate_config_cursor() {
        let config = generate_config(&ClientType::Cursor, "mcpw", 3000, false);
        assert!(config.contains("\"mcpServers\""));
        assert!(config.contains("\"command\": \"mcpw\""));
    }

    #[test]
    fn test_generate_config_vscode() {
        let config = generate_config(&ClientType::Vscode, "mcpw", 3000, false);
        assert!(config.contains("\"mcp\""));
        assert!(config.contains("\"servers\""));
        assert!(config.contains("\"command\": \"mcpw\""));
        // VS Code uses different format than Claude Desktop
        assert!(!config.contains("\"mcpServers\""));
    }

    #[test]
    fn test_generate_config_custom_path() {
        let config = generate_config(
            &ClientType::ClaudeDesktop,
            "/usr/local/bin/mcpw",
            3000,
            false,
        );
        assert!(config.contains("\"command\": \"/usr/local/bin/mcpw\""));
    }

    #[test]
    fn test_config_location_not_empty() {
        let clients = [
            ClientType::ClaudeDesktop,
            ClientType::ClaudeCode,
            ClientType::Cursor,
            ClientType::Vscode,
        ];
        for client in &clients {
            let loc = config_location(client);
            assert!(
                !loc.is_empty(),
                "Location for {:?} should not be empty",
                client
            );
        }
    }

    #[test]
    fn test_display_name() {
        assert_eq!(ClientType::ClaudeDesktop.display_name(), "Claude Desktop");
        assert_eq!(ClientType::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(ClientType::Cursor.display_name(), "Cursor");
        assert_eq!(ClientType::Vscode.display_name(), "VS Code");
    }
}
