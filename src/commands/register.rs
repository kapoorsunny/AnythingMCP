use std::io::{self, Write};

use chrono::Utc;

use crate::error::{McpWrapError, Result};
use crate::parser::help_parser::HelpParser;
use crate::parser::help_runner::HelpRunner;
use crate::registry::models::{ToolDefinition, TransportType};
use crate::registry::store::ToolRegistry;

pub struct RegisterOptions<'a> {
    pub name: &'a str,
    pub cmd: &'a str,
    pub desc: Option<&'a str>,
    pub transport_type: &'a str,
    pub force: bool,
    pub allow_unsafe: bool,
    pub tools_path: &'a std::path::Path,
}

pub fn run(
    registry: &dyn ToolRegistry,
    help_runner: &dyn HelpRunner,
    help_parser: &dyn HelpParser,
    opts: RegisterOptions<'_>,
) -> Result<()> {
    // Validate tool name: must be snake_case (lowercase alphanumeric + underscores)
    let name_re = regex::Regex::new(r"^[a-z][a-z0-9_]{0,63}$").expect("Invalid regex");
    if !name_re.is_match(opts.name) {
        return Err(McpWrapError::RegistryError(format!(
            "Invalid tool name '{}'. Must be snake_case (lowercase letters, numbers, underscores), \
             start with a letter, and be at most 64 characters.",
            opts.name
        )));
    }

    // Safety: check blocklist (unless --allow-unsafe)
    if !opts.allow_unsafe {
        if let Some(reason) = crate::commands::block::is_blocked(opts.tools_path, opts.cmd) {
            return Err(McpWrapError::RegistryError(format!(
                "Blocked: '{}'. {}. Use --allow-unsafe to override.",
                opts.cmd, reason
            )));
        }
    }

    // Parse transport type
    let transport = match opts.transport_type {
        "stdio" => TransportType::Stdio,
        "sse" => TransportType::Sse,
        other => {
            return Err(McpWrapError::RegistryError(format!(
                "Unknown type '{}'. Use 'stdio' or 'sse'.",
                other
            )));
        }
    };

    // Check if tool already exists
    if let Some(_existing) = registry.get(opts.name)? {
        if !opts.force {
            // If stdin is not a TTY (e.g., LLM, CI/CD), abort rather than hang
            if !atty::is(atty::Stream::Stdin) {
                return Err(McpWrapError::RegistryError(format!(
                    "Tool '{}' already exists. Use --force to overwrite.",
                    opts.name
                )));
            }

            print!("Tool '{}' already exists. Overwrite? [y/N] ", opts.name);
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();

            if input != "y" && input != "yes" {
                println!("Aborted.");
                return Ok(());
            }
        }
    }

    // Run --help to discover parameters
    let help_output = match help_runner.run_help(opts.cmd) {
        Ok(output) => output,
        Err(McpWrapError::CommandNotFound(cmd)) => {
            return Err(McpWrapError::CommandNotFound(cmd));
        }
        Err(e) => return Err(e),
    };

    // Parse help output to extract parameters
    let params = help_parser.parse(&help_output)?;

    if params.is_empty() {
        eprintln!(
            "\u{26a0}  No --flag-style parameters detected. Tool registered with no parameters."
        );
    }

    // Extract description from help if not provided
    let description = if let Some(d) = opts.desc {
        d.to_string()
    } else {
        let parser = crate::parser::help_parser::HeuristicHelpParser::new();
        parser
            .extract_description(&help_output)
            .unwrap_or_else(|| format!("Registered tool: {}", opts.name))
    };

    let tool = ToolDefinition {
        name: opts.name.to_string(),
        command: opts.cmd.to_string(),
        description,
        params: params.clone(),
        transport: transport.clone(),
        registered_at: Utc::now(),
    };

    registry.add(tool)?;

    crate::logger::register(opts.name, &transport.to_string(), params.len());

    println!(
        "\u{2714} Registered tool '{}' ({}) with {} parameters",
        opts.name,
        transport,
        params.len()
    );

    Ok(())
}
