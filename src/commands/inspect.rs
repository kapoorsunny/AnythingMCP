use crate::error::Result;
use crate::registry::store::ToolRegistry;

pub fn run(registry: &dyn ToolRegistry, name: &str) -> Result<()> {
    match registry.get(name)? {
        Some(tool) => {
            println!("Tool: {}", tool.name);
            println!("Command: {}", tool.command);
            println!("Description: {}", tool.description);
            println!("Registered: {}", tool.registered_at);
            println!("Parameters: ({})", tool.params.len());
            println!();

            if tool.params.is_empty() {
                println!("  (no parameters)");
            } else {
                for param in &tool.params {
                    let type_str = format!("{:?}", param.param_type);
                    let required_str = if param.required { " [required]" } else { "" };
                    let default_str = if let Some(ref d) = param.default_value {
                        format!(" [default: {}]", d)
                    } else {
                        String::new()
                    };

                    println!(
                        "  --{:<20} <{:<10}> {}{}{}",
                        param.name, type_str, param.description, required_str, default_str
                    );
                }
            }

            Ok(())
        }
        None => {
            eprintln!("Error: tool '{}' not found.", name);
            std::process::exit(1);
        }
    }
}
