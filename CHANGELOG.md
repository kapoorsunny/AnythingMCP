# Changelog

All notable changes to this project will be documented in this file.

## [1.0.0] - 2026-03-21

### Added
- `register` — Register CLI tools with auto-discovered parameters (--help, -h, man, /?)
- `import` — Import OpenAPI/Swagger specs as MCP tools (JSON and YAML, URL or file)
- `list` — List all registered tools (CLI + API) with KIND and TYPE columns
- `inspect` — Show full tool definition with parameters
- `test` — Built-in MCP client to test tools locally
- `test --progressive` — Simulate the 3-step LLM discovery flow
- `serve` — Start MCP server (both STDIO and SSE transports simultaneously)
- `serve --progressive` — Progressive tool disclosure for large tool sets (80-160x token savings)
- `remove` — Remove a tool or all tools (--all)
- `block` / `unblock` — Customizable command blocklist (dangerous commands blocked by default)
- `logs` — View mcpw logs (--follow for live stream)
- Per-tool transport type (--type stdio|sse)
- OpenAPI auth support (bearer, header, basic via env vars)
- Static headers for imported APIs (--header)
- Tool name validation (snake_case enforced)
- 30-second execution timeout
- 1MB output cap
- CORS protection on SSE server
- Platform-aware help discovery (macOS, Linux, Windows)
- 99 tests (unit + integration + load)
