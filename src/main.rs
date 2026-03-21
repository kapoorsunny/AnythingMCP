mod cli;
mod commands;
mod error;
mod executor;
mod logger;
mod mcp;
mod openapi;
mod parser;
mod registry;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use cli::{Cli, Commands};
use parser::help_parser::HeuristicHelpParser;
use parser::help_runner::ProcessHelpRunner;
use registry::store::JsonFileRegistry;

fn get_tools_path() -> PathBuf {
    if let Ok(dir) = std::env::var("MCPW_TOOLS_DIR") {
        return PathBuf::from(dir).join("tools.json");
    }
    let home = dirs::home_dir().expect("Could not determine home directory");
    home.join(".mcpw").join("tools.json")
}

fn get_api_tools_path() -> PathBuf {
    if let Ok(dir) = std::env::var("MCPW_TOOLS_DIR") {
        return PathBuf::from(dir).join("api_tools.json");
    }
    let home = dirs::home_dir().expect("Could not determine home directory");
    home.join(".mcpw").join("api_tools.json")
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let tools_path = get_tools_path();

    // Initialize logger
    logger::McpwLogger::init(&tools_path);

    match cli.command {
        Commands::Register {
            name,
            cmd,
            desc,
            r#type,
            force,
            allow_unsafe,
        } => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let help_runner = ProcessHelpRunner::new();
            let help_parser = HeuristicHelpParser::new();

            if let Err(e) = commands::register::run(
                &registry,
                &help_runner,
                &help_parser,
                commands::register::RegisterOptions {
                    name: &name,
                    cmd: &cmd,
                    desc: desc.as_deref(),
                    transport_type: &r#type,
                    force,
                    allow_unsafe,
                    tools_path: &tools_path,
                },
            ) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::List => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let api_tools_path = get_api_tools_path();
            let api_registry = openapi::store::ApiToolRegistry::new(api_tools_path).ok();

            if let Err(e) = commands::list::run(&registry, api_registry.as_ref()) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Remove { name, all } => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            if let Err(e) = commands::remove::run(&registry, name.as_deref(), all, &tools_path) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Serve {
            port,
            host,
            progressive,
        } => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let api_tools_path = get_api_tools_path();
            if let Err(e) =
                commands::serve::run(Arc::new(registry), &host, port, api_tools_path, progressive)
                    .await
            {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Inspect { name } => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            if let Err(e) = commands::inspect::run(&registry, &name) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Test {
            name,
            args,
            progressive,
        } => {
            let registry = match JsonFileRegistry::new(tools_path.clone()) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            let api_tools_path = get_api_tools_path();
            let api_registry = openapi::store::ApiToolRegistry::new(api_tools_path)
                .ok()
                .map(Arc::new);

            if let Err(e) = commands::test_tool::run(
                Arc::new(registry),
                api_registry,
                &name,
                &args,
                progressive,
            ) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Import {
            source,
            r#type,
            auth_env,
            auth_type,
            auth_header,
            include,
            exclude,
            prefix,
            header,
        } => {
            let api_tools_path = get_api_tools_path();
            let api_registry = match openapi::store::ApiToolRegistry::new(api_tools_path) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };

            if let Err(e) = commands::import::run(
                &api_registry,
                commands::import::ImportOptions {
                    source: &source,
                    transport_type: &r#type,
                    auth_env: auth_env.as_deref(),
                    auth_type: Some(&auth_type),
                    auth_header: auth_header.as_deref(),
                    include: &include,
                    exclude: &exclude,
                    prefix: prefix.as_deref(),
                    headers: &header,
                },
            )
            .await
            {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Block {
            command,
            reason,
            list,
            reset,
        } => {
            if reset {
                if let Err(e) = commands::block::reset_blocklist(&tools_path) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            } else if list {
                if let Err(e) = commands::block::list_blocked(&tools_path) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            } else if let Some(cmd) = command {
                let registry = match JsonFileRegistry::new(tools_path.clone()) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                };
                if let Err(e) = commands::block::block(&tools_path, &cmd, &reason, &registry) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            } else {
                // No command given, show the list
                if let Err(e) = commands::block::list_blocked(&tools_path) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Unblock { command } => {
            if let Err(e) = commands::block::unblock(&tools_path, &command) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }

        Commands::Logs { tail, follow } => {
            if let Err(e) = commands::logs::run(&tools_path, follow, tail) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
