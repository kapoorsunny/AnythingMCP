use crate::error::{McpWrapError, Result};
use crate::openapi::models::{AuthConfig, StaticHeader};
use crate::openapi::parser::parse_openapi_spec;
use crate::openapi::store::ApiToolRegistry;
use crate::registry::models::TransportType;

pub struct ImportOptions<'a> {
    pub source: &'a str,
    pub transport_type: &'a str,
    pub auth_env: Option<&'a str>,
    pub auth_type: Option<&'a str>,
    pub auth_header: Option<&'a str>,
    pub include: &'a [String],
    pub exclude: &'a [String],
    pub prefix: Option<&'a str>,
    pub headers: &'a [String],
}

pub async fn run(registry: &ApiToolRegistry, opts: ImportOptions<'_>) -> Result<()> {
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

    // Build auth config if provided
    let auth = opts.auth_env.map(|env| AuthConfig {
        auth_type: opts.auth_type.unwrap_or("bearer").to_string(),
        auth_env: env.to_string(),
        auth_header: opts.auth_header.map(|h| h.to_string()),
    });

    // Parse static headers: "header-name=ENV_VAR_NAME"
    let static_headers: Vec<StaticHeader> = opts
        .headers
        .iter()
        .filter_map(|h| {
            let parts: Vec<&str> = h.splitn(2, '=').collect();
            if parts.len() == 2 {
                Some(StaticHeader {
                    name: parts[0].to_string(),
                    env_var: Some(parts[1].to_string()),
                    value: None,
                })
            } else {
                eprintln!(
                    "Warning: ignoring malformed header '{}'. Use format: name=ENV_VAR",
                    h
                );
                None
            }
        })
        .collect();

    // Fetch the spec — from URL or local file
    eprintln!("Fetching OpenAPI spec from {}...", opts.source);
    let spec_content = if opts.source.starts_with("http://") || opts.source.starts_with("https://")
    {
        let response =
            reqwest::get(opts.source)
                .await
                .map_err(|e| McpWrapError::HelpParseFailed {
                    cmd: opts.source.to_string(),
                    reason: format!("Failed to fetch spec: {}", e),
                })?;

        if !response.status().is_success() {
            return Err(McpWrapError::HelpParseFailed {
                cmd: opts.source.to_string(),
                reason: format!("HTTP {}", response.status()),
            });
        }

        response
            .text()
            .await
            .map_err(|e| McpWrapError::HelpParseFailed {
                cmd: opts.source.to_string(),
                reason: format!("Failed to read response: {}", e),
            })?
    } else {
        std::fs::read_to_string(opts.source).map_err(|e| McpWrapError::HelpParseFailed {
            cmd: opts.source.to_string(),
            reason: format!("Failed to read file: {}", e),
        })?
    };

    // Parse the spec
    let tools = parse_openapi_spec(crate::openapi::parser::ParseOptions {
        spec_content: &spec_content,
        spec_source: opts.source,
        transport,
        auth,
        static_headers,
        include_patterns: opts.include,
        exclude_patterns: opts.exclude,
        prefix: opts.prefix,
    })?;

    if tools.is_empty() {
        eprintln!("No endpoints found in the spec.");
        return Ok(());
    }

    // Show what was discovered
    println!("Discovered {} endpoints:", tools.len());
    for tool in &tools {
        println!(
            "  {} {:<8} {:<30} ({})",
            tool.transport,
            tool.method,
            tool.name,
            tool.params.len()
        );
    }

    // Save to registry
    registry.add_many(tools.clone())?;

    println!(
        "\n\u{2714} Imported {} API tools from {}",
        tools.len(),
        opts.source
    );

    Ok(())
}
