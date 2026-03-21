use regex::Regex;
use serde_json::Value;

use crate::error::{McpWrapError, Result};
use crate::openapi::models::{ApiParam, ApiParamLocation, ApiToolDefinition, AuthConfig};
use crate::registry::models::{ParamType, TransportType};

/// Options for parsing an OpenAPI spec.
pub struct ParseOptions<'a> {
    pub spec_content: &'a str,
    pub spec_source: &'a str,
    pub transport: TransportType,
    pub auth: Option<AuthConfig>,
    pub static_headers: Vec<crate::openapi::models::StaticHeader>,
    pub include_patterns: &'a [String],
    pub exclude_patterns: &'a [String],
    pub prefix: Option<&'a str>,
}

/// Parse an OpenAPI spec (JSON or YAML string) into API tool definitions.
pub fn parse_openapi_spec(opts: ParseOptions<'_>) -> Result<Vec<ApiToolDefinition>> {
    let spec_source = opts.spec_source;
    let transport = opts.transport;
    let auth = opts.auth;
    let static_headers = opts.static_headers;
    let prefix = opts.prefix;
    // Try JSON first, then YAML
    let spec: Value = serde_json::from_str(opts.spec_content).or_else(|_| {
        serde_yaml::from_str::<Value>(opts.spec_content).map_err(|e| {
            McpWrapError::HelpParseFailed {
                cmd: spec_source.to_string(),
                reason: format!("Failed to parse spec as JSON or YAML: {}", e),
            }
        })
    })?;

    let base_url = extract_base_url(&spec, spec_source);
    let paths = spec
        .get("paths")
        .and_then(|p| p.as_object())
        .ok_or_else(|| McpWrapError::HelpParseFailed {
            cmd: spec_source.to_string(),
            reason: "No 'paths' found in OpenAPI spec".to_string(),
        })?;

    let mut tools = Vec::new();

    for (path, path_item) in paths {
        // Apply include/exclude filters
        if !should_include_path(path, opts.include_patterns, opts.exclude_patterns) {
            continue;
        }

        let path_obj = match path_item.as_object() {
            Some(o) => o,
            None => continue,
        };

        for (method, operation) in path_obj {
            // Skip non-HTTP methods (e.g., "parameters", "summary")
            let method_upper = method.to_uppercase();
            if !["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"]
                .contains(&method_upper.as_str())
            {
                continue;
            }

            let tool_name = generate_tool_name(path, &method_upper, operation, prefix);
            let description = extract_description(operation, path, &method_upper);
            let params = extract_parameters(operation, path_obj, path);
            let url_template = format!("{}{}", base_url, path);

            tools.push(ApiToolDefinition {
                name: tool_name,
                description,
                method: method_upper,
                url_template,
                params,
                transport: transport.clone(),
                auth: auth.clone(),
                static_headers: static_headers.clone(),
                source_spec: spec_source.to_string(),
            });
        }
    }

    Ok(tools)
}

/// Extract base URL from the spec. If the spec has a relative server URL,
/// derive the base from the spec source URL.
fn extract_base_url(spec: &Value, spec_source: &str) -> String {
    // OpenAPI 3.x: servers[0].url
    if let Some(url) = spec
        .get("servers")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
    {
        let url = url.trim_end_matches('/');
        // If URL is relative (starts with /), derive host from spec_source
        if url.starts_with('/') {
            if let Some(base) = extract_host_from_url(spec_source) {
                return format!("{}{}", base, url);
            }
        }
        return url.to_string();
    }

    // Swagger 2.x: host + basePath
    let host = spec
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("localhost");
    let base_path = spec.get("basePath").and_then(|b| b.as_str()).unwrap_or("");
    let scheme = spec
        .get("schemes")
        .and_then(|s| s.as_array())
        .and_then(|arr| arr.first())
        .and_then(|s| s.as_str())
        .unwrap_or("https");

    format!("{}://{}{}", scheme, host, base_path.trim_end_matches('/'))
}

/// Extract scheme + host from a URL (e.g., "https://api.example.com/path" -> "https://api.example.com")
fn extract_host_from_url(url: &str) -> Option<String> {
    // Find scheme://host
    let after_scheme = url.find("://").map(|i| i + 3)?;
    let host_end = url[after_scheme..]
        .find('/')
        .map(|i| after_scheme + i)
        .unwrap_or(url.len());
    Some(url[..host_end].to_string())
}

/// Generate a tool name from the operation
fn generate_tool_name(path: &str, method: &str, operation: &Value, prefix: Option<&str>) -> String {
    // Prefer operationId if present
    let base_name = if let Some(op_id) = operation.get("operationId").and_then(|o| o.as_str()) {
        to_snake_case(op_id)
    } else {
        // Auto-generate from method + path: GET /users/{id} -> get_users_by_id
        let clean_path = path
            .replace('/', "_")
            .replace('{', "by_")
            .replace('}', "")
            .trim_matches('_')
            .to_string();
        let clean_path = clean_path.replace("__", "_");
        format!("{}_{}", method.to_lowercase(), clean_path.trim_matches('_'))
    };

    match prefix {
        Some(p) => format!("{}_{}", p, base_name),
        None => base_name,
    }
}

/// Convert camelCase or PascalCase to snake_case
fn to_snake_case(s: &str) -> String {
    let re = Regex::new(r"([a-z0-9])([A-Z])").expect("Invalid regex");
    let result = re.replace_all(s, "${1}_${2}");
    result
        .to_lowercase()
        .chars()
        .map(|c| if c == '-' || c == ' ' { '_' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Extract description from the operation
fn extract_description(operation: &Value, path: &str, method: &str) -> String {
    operation
        .get("summary")
        .and_then(|s| s.as_str())
        .or_else(|| operation.get("description").and_then(|d| d.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{} {}", method, path))
}

/// Extract parameters from the operation and path item
fn extract_parameters(
    operation: &Value,
    path_item: &serde_json::Map<String, Value>,
    path: &str,
) -> Vec<ApiParam> {
    let mut params = Vec::new();

    // Collect params from path-level and operation-level
    let path_params = path_item
        .get("parameters")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    let op_params = operation
        .get("parameters")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();

    // Operation params override path params
    for param in path_params.iter().chain(op_params.iter()) {
        if let Some(api_param) = parse_parameter(param) {
            // Deduplicate by name
            if !params.iter().any(|p: &ApiParam| p.name == api_param.name) {
                params.push(api_param);
            }
        }
    }

    // Extract path parameters from URL template that weren't in the params list
    let path_param_re = Regex::new(r"\{(\w+)\}").expect("Invalid regex");
    for cap in path_param_re.captures_iter(path) {
        let name = cap[1].to_string();
        if !params.iter().any(|p| p.name == name) {
            params.push(ApiParam {
                name,
                description: "Path parameter".to_string(),
                param_type: ParamType::String,
                required: true,
                location: ApiParamLocation::Path,
            });
        }
    }

    // Extract request body params (for POST/PUT/PATCH)
    if let Some(body) = operation.get("requestBody") {
        extract_body_params(body, &mut params);
    }

    // Swagger 2.x: body params are in parameters with "in": "body"
    // Already handled by parse_parameter

    params
}

/// Parse a single parameter definition
fn parse_parameter(param: &Value) -> Option<ApiParam> {
    let name = param.get("name").and_then(|n| n.as_str())?.to_string();
    let location_str = param.get("in").and_then(|i| i.as_str()).unwrap_or("query");

    let location = match location_str {
        "path" => ApiParamLocation::Path,
        "query" => ApiParamLocation::Query,
        "header" => ApiParamLocation::Header,
        "body" => ApiParamLocation::Body,
        _ => ApiParamLocation::Query,
    };

    let description = param
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    let required = param
        .get("required")
        .and_then(|r| r.as_bool())
        .unwrap_or(location == ApiParamLocation::Path); // path params are always required

    let param_type = infer_type_from_schema(param.get("schema").unwrap_or(param));

    Some(ApiParam {
        name,
        description,
        param_type,
        required,
        location,
    })
}

/// Extract request body properties as params
fn extract_body_params(body: &Value, params: &mut Vec<ApiParam>) {
    let schema = body
        .get("content")
        .and_then(|c| c.get("application/json"))
        .and_then(|j| j.get("schema"))
        .or_else(|| body.get("schema"));

    if let Some(schema) = schema {
        let required_fields: Vec<String> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
            for (name, prop) in props {
                let description = prop
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();

                let param_type = infer_type_from_schema(prop);
                let required = required_fields.contains(name);

                params.push(ApiParam {
                    name: name.clone(),
                    description,
                    param_type,
                    required,
                    location: ApiParamLocation::Body,
                });
            }
        }
    }
}

/// Infer ParamType from a JSON Schema type field
fn infer_type_from_schema(schema: &Value) -> ParamType {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("integer") | Some("int") => ParamType::Integer,
        Some("number") | Some("float") | Some("double") => ParamType::Float,
        Some("boolean") | Some("bool") => ParamType::Boolean,
        _ => ParamType::String,
    }
}

/// Check if a path should be included based on filters
fn should_include_path(path: &str, includes: &[String], excludes: &[String]) -> bool {
    // If includes are specified, path must match at least one
    if !includes.is_empty() {
        let matches_include = includes.iter().any(|pattern| path_matches(path, pattern));
        if !matches_include {
            return false;
        }
    }

    // If excludes are specified, path must not match any
    if !excludes.is_empty() {
        let matches_exclude = excludes.iter().any(|pattern| path_matches(path, pattern));
        if matches_exclude {
            return false;
        }
    }

    true
}

/// Simple glob-like path matching (supports * wildcard)
fn path_matches(path: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        path.starts_with(prefix)
    } else {
        path == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("getUserById"), "get_user_by_id");
        assert_eq!(to_snake_case("GetUserById"), "get_user_by_id");
        assert_eq!(to_snake_case("listAllPets"), "list_all_pets");
        assert_eq!(to_snake_case("simple"), "simple");
        assert_eq!(to_snake_case("HTMLParser"), "htmlparser");
    }

    #[test]
    fn test_path_matches() {
        assert!(path_matches("/users/123", "/users/*"));
        assert!(path_matches("/users", "/users"));
        assert!(!path_matches("/admin/users", "/users/*"));
        assert!(path_matches("/repos/foo/bar", "/repos/*"));
    }

    #[test]
    fn test_should_include_path() {
        // No filters — include all
        assert!(should_include_path("/users", &[], &[]));

        // Include filter
        assert!(should_include_path("/users/1", &["/users/*".into()], &[]));
        assert!(!should_include_path("/admin", &["/users/*".into()], &[]));

        // Exclude filter
        assert!(!should_include_path("/admin/x", &[], &["/admin/*".into()]));
        assert!(should_include_path("/users", &[], &["/admin/*".into()]));

        // Both
        assert!(should_include_path(
            "/users/1",
            &["/users/*".into()],
            &["/users/admin*".into()]
        ));
        assert!(!should_include_path(
            "/users/admin",
            &["/users/*".into()],
            &["/users/admin*".into()]
        ));
    }

    #[test]
    fn test_generate_tool_name_with_operation_id() {
        let op = serde_json::json!({"operationId": "getUserById"});
        let name = generate_tool_name("/users/{id}", "GET", &op, None);
        assert_eq!(name, "get_user_by_id");
    }

    #[test]
    fn test_generate_tool_name_auto() {
        let op = serde_json::json!({});
        let name = generate_tool_name("/users/{id}", "GET", &op, None);
        assert_eq!(name, "get_users_by_id");
    }

    #[test]
    fn test_generate_tool_name_with_prefix() {
        let op = serde_json::json!({"operationId": "listPets"});
        let name = generate_tool_name("/pets", "GET", &op, Some("petstore"));
        assert_eq!(name, "petstore_list_pets");
    }

    #[test]
    fn test_parse_petstore_spec() {
        let spec = r#"{
            "openapi": "3.0.0",
            "info": {"title": "Petstore", "version": "1.0.0"},
            "servers": [{"url": "https://petstore.example.com/v1"}],
            "paths": {
                "/pets": {
                    "get": {
                        "operationId": "listPets",
                        "summary": "List all pets",
                        "parameters": [
                            {
                                "name": "limit",
                                "in": "query",
                                "description": "Max items to return",
                                "required": false,
                                "schema": {"type": "integer"}
                            }
                        ]
                    },
                    "post": {
                        "operationId": "createPet",
                        "summary": "Create a pet",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["name"],
                                        "properties": {
                                            "name": {
                                                "type": "string",
                                                "description": "Pet name"
                                            },
                                            "tag": {
                                                "type": "string",
                                                "description": "Pet tag"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "/pets/{petId}": {
                    "get": {
                        "operationId": "showPetById",
                        "summary": "Get a pet by ID",
                        "parameters": [
                            {
                                "name": "petId",
                                "in": "path",
                                "required": true,
                                "schema": {"type": "string"}
                            }
                        ]
                    }
                }
            }
        }"#;

        let tools = parse_openapi_spec(ParseOptions {
            spec_content: spec,
            spec_source: "petstore.json",
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            include_patterns: &[],
            exclude_patterns: &[],
            prefix: None,
        })
        .unwrap();

        assert_eq!(tools.len(), 3);

        let list = tools.iter().find(|t| t.name == "list_pets").unwrap();
        assert_eq!(list.method, "GET");
        assert_eq!(list.url_template, "https://petstore.example.com/v1/pets");
        assert_eq!(list.params.len(), 1);
        assert_eq!(list.params[0].name, "limit");
        assert_eq!(list.params[0].param_type, ParamType::Integer);
        assert!(!list.params[0].required);

        let create = tools.iter().find(|t| t.name == "create_pet").unwrap();
        assert_eq!(create.method, "POST");
        assert_eq!(create.params.len(), 2); // name + tag from body
        let name_param = create.params.iter().find(|p| p.name == "name").unwrap();
        assert!(name_param.required);
        assert_eq!(name_param.location, ApiParamLocation::Body);

        let show = tools.iter().find(|t| t.name == "show_pet_by_id").unwrap();
        assert_eq!(show.params.len(), 1);
        assert_eq!(show.params[0].name, "petId");
        assert!(show.params[0].required);
        assert_eq!(show.params[0].location, ApiParamLocation::Path);
    }

    #[test]
    fn test_parse_with_filters() {
        let spec = r#"{
            "openapi": "3.0.0",
            "info": {"title": "API", "version": "1.0.0"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/users": {"get": {"summary": "List users"}},
                "/users/{id}": {"get": {"summary": "Get user"}},
                "/admin/settings": {"get": {"summary": "Admin settings"}}
            }
        }"#;

        // Include only /users/*
        let tools = parse_openapi_spec(ParseOptions {
            spec_content: spec,
            spec_source: "api.json",
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            include_patterns: &["/users*".into()],
            exclude_patterns: &[],
            prefix: None,
        })
        .unwrap();
        assert_eq!(tools.len(), 2);

        // Exclude /admin/*
        let tools = parse_openapi_spec(ParseOptions {
            spec_content: spec,
            spec_source: "api.json",
            transport: TransportType::Sse,
            auth: None,
            static_headers: vec![],
            include_patterns: &[],
            exclude_patterns: &["/admin/*".into()],
            prefix: None,
        })
        .unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().all(|t| !t.name.contains("admin")));
    }
}
