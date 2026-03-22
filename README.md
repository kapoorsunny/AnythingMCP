# mcpw

Turn any command-line tool or web API into an MCP server — without writing any code.

`mcpw` takes any CLI tool, script, binary, or OpenAPI-compatible web service and exposes it as an [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) tool that AI assistants like Claude, Cursor, and others can call directly.

## How It Works

**CLI tools:** Register a command — `mcpw` auto-discovers parameters from `--help` output.

**Web APIs:** Import an OpenAPI/Swagger spec — `mcpw` creates MCP tools for each endpoint.

Both types are served simultaneously over STDIO (for local AI clients) and SSE (for remote clients).

No schemas to write. No code to maintain. Just point at a command or API and go.

## Installation

### Homebrew (macOS / Linux)

```bash
brew tap kapoorsunny/tap
brew install mcpw
```

### Pre-built binaries

Download from the [releases page](https://github.com/kapoorsunny/AnythingMCP/releases) — available for macOS (ARM/Intel), Linux (ARM/x64), and Windows (x64).

### From source

```bash
git clone https://github.com/kapoorsunny/AnythingMCP
cd mcpw
cargo install --path .
```

## Quick Start

```bash
# Register a CLI tool
mcpw register resize_image --cmd "python resize.py"

# Import an entire API from OpenAPI spec
mcpw import https://petstore3.swagger.io/api/v3/openapi.json --type sse

# See what's registered
mcpw list

# Test a tool locally
mcpw test resize_image --args '{"input": "photo.jpg", "width": 640}'

# Start the MCP server (both STDIO and SSE)
mcpw serve
```

## CLI Reference

### `register` — Register a tool

```bash
mcpw register <name> --cmd <command> [--type stdio|sse] [--desc <description>] [--force]
```

Registers a command as an MCP tool. Parameters are auto-discovered from `--help` output.

- `--type stdio` (default) — tool is served via STDIO (for local clients like Claude Desktop)
- `--type sse` — tool is served via SSE HTTP (for remote/network clients)

```bash
# Python script (default: stdio)
mcpw register resize --cmd "python /scripts/resize.py" --desc "Resize images"

# System utility for remote access
mcpw register http --cmd "curl" --type sse

# Bash script
mcpw register deploy --cmd "/scripts/deploy.sh"

# Overwrite without prompting
mcpw register deploy --cmd "/scripts/deploy_v2.sh" --force
```

**Parameter discovery chain:**
1. `--help` (all platforms)
2. `-h` (all platforms)
3. `man <command>` (macOS/Linux)
4. `/?` (Windows)

Tools with no `--flag` style parameters (e.g., `cp`, `mv`) are registered with zero parameters — they can still be called, but the AI won't auto-discover their flags.

### `list` — List all registered tools

```bash
mcpw list
```

Shows both CLI tools and API tools in one table:

```
NAME                          KIND      PARAMS  TYPE    DESCRIPTION
────────────────────────────────────────────────────────────────────────────────
resize_image                  CLI       3       STDIO   Resize an image to given dim...
http                          CLI       9       SSE     Make HTTP requests
get_pet_by_id                 API GET   1       SSE     Find pet by ID
create_user                   API POST  0       SSE     Create user
```

### `inspect` — Show full tool details

```bash
mcpw inspect <name>
```

```
Tool: resize_image
Command: python resize.py
Description: Resize an image to given dimensions
Parameters: (3)
  --input              <String    > Path to input image [required]
  --width              <Integer   > Target width in pixels [default: 800]
  --verbose            <Boolean   > Enable verbose output
```

### `test` — Test a tool locally

```bash
mcpw test <name> [--args <json>]
```

Built-in MCP client that executes the tool and shows the result — no external client needed.

```bash
# Test with arguments
mcpw test resize_image --args '{"input": "photo.jpg", "width": 640}'

# Test with no arguments
mcpw test list_files

# Test boolean flags
mcpw test my_tool --args '{"verbose": true}'
```

Output shows three sections:
1. **Tool Schema** — what AI clients see (JSON Schema)
2. **Request** — the arguments being sent
3. **Result** — OK with stdout, or ERROR with stderr

### `import` — Import API tools from OpenAPI spec

```bash
mcpw import <source> [--type stdio|sse] [--auth-env <VAR>] [--auth-type bearer|header|basic]
                      [--auth-header <name>] [--include <pattern>] [--exclude <pattern>]
                      [--prefix <prefix>]
```

Imports API endpoints from an OpenAPI v3 or Swagger v2 spec (JSON or YAML, from URL or local file). Each endpoint becomes an MCP tool that makes HTTP requests.

```bash
# Import all endpoints from a public API
mcpw import https://petstore3.swagger.io/api/v3/openapi.json --type sse

# Import from a local file
mcpw import ./openapi.yaml

# Import with bearer token auth (secret stored in env var, never on disk)
mcpw import https://api.example.com/openapi.json \
  --auth-env API_TOKEN --auth-type bearer

# Import with API key header
mcpw import https://api.example.com/openapi.json \
  --auth-env MY_KEY --auth-type header --auth-header "X-API-Key"

# Import only specific endpoints
mcpw import https://api.github.com/openapi.json \
  --include "/repos/*" --exclude "/repos/*/actions/*"

# Import with a name prefix
mcpw import https://api.stripe.com/openapi.json --prefix stripe
```

**Tool naming:** Uses `operationId` from the spec if present (e.g., `getUserById` → `get_user_by_id`). Falls back to auto-generated from method + path (e.g., `GET /users/{id}` → `get_users_by_id`).

**Authentication:** Secrets are never stored on disk. You specify the env var name at import time; at serve time, `mcpw` reads the env var and adds the auth header to every request.

### `serve` — Start the MCP server

```bash
mcpw serve [--port <port>] [--host <host>]
```

Starts a single server that serves **both STDIO and SSE simultaneously**. CLI tools and API tools are both available. STDIO-typed tools are served over STDIO; SSE-typed tools are served over SSE.

- **STDIO** — reads JSON-RPC from stdin/stdout (for Claude Desktop, Cursor, etc.)
- **SSE** — HTTP server on the specified port (for remote/network clients)

For API tools with auth, set the env var before starting:
```bash
API_TOKEN=tok_123 mcpw serve --port 8080
```

```bash
# Start with default SSE port (3000)
mcpw serve

# Custom port
mcpw serve --port 8080

# Progressive mode (recommended for 20+ tools)
mcpw serve --progressive

# Bind to all interfaces (for remote access)
mcpw serve --host 0.0.0.0 --port 3000
```

**Progressive disclosure** (`--progressive`): Instead of sending all tool schemas upfront (which eats context window), serves 3 meta-tools (`search_tools`, `get_tool_schema`, `call_tool`) that let the LLM discover tools on-demand. Reduces token usage by 80-160x for large tool sets.

### `remove` — Remove a tool

```bash
mcpw remove <name>           # remove one tool
mcpw remove --all            # remove all CLI and API tools
```

### `block` / `unblock` — Manage command blocklist

```bash
mcpw block --list                                    # see all blocked commands
mcpw block curl --reason "Company policy: no HTTP"   # block a command
mcpw unblock curl                                    # unblock a command
mcpw block --reset                                   # reset to defaults
```

Dangerous commands (rm, shutdown, dd, bash, etc.) are blocked by default. Blocking a command also removes any already-registered tools using it. Use `--allow-unsafe` on `register` to override.

### `doctor` — Diagnose configuration issues

```bash
mcpw doctor
```

Runs diagnostic checks on your setup: tools directory, JSON file validity, command reachability, env vars for API auth, and blocklist health. Each check reports PASS, WARN, or FAIL. Exit code 0 if no failures, 1 otherwise.

### `config` — Generate client configuration

```bash
mcpw config --client claude-desktop
mcpw config --client cursor
mcpw config --client vscode
mcpw config --client claude-code
mcpw config --client claude-desktop --port 8080 --progressive
```

Generates the exact JSON snippet needed for each MCP client, with the correct format and file location. No more manual JSON editing.

### `export` / `import-config` — Portable tool bundles

```bash
# Export all tools
mcpw export > my-tools.json

# Export a single tool
mcpw export --tool resize_image > resize.json

# Import on another machine or share with teammates
mcpw import-config my-tools.json
```

Exports CLI and API tool registrations as portable JSON. Use this to share setups, back up configs, or replicate across machines.

### `validate` — Detect schema drift

```bash
mcpw validate                     # validate all CLI tools
mcpw validate --tool resize       # validate a specific tool
```

Re-runs help discovery on registered tools and compares parameters against the stored schema. Reports added/removed parameters. Exit code 0 if no drift, 1 if drift detected.

### `update` — Refresh tool parameters

```bash
mcpw update --tool resize_image   # re-discover params for one tool
mcpw update                       # refresh all tools
mcpw update --dry-run             # preview changes without applying
```

When your underlying CLI tool changes (new flags, removed flags), `update` re-runs help discovery and saves the updated schema. Use `--dry-run` to preview what would change.

### `status` — Server health check

```bash
mcpw status                       # human-readable output
mcpw status --json                # machine-readable (for monitoring)
mcpw status --port 8080           # check specific SSE port
```

Shows whether the server is running, PID, tool counts, SSE port status, and recent log activity. Exit code 0 if running, 1 if not.

### `dry-run` — Preview tool execution

```bash
mcpw dry-run my_tool --args '{"input": "data.csv"}'
mcpw dry-run get_user --args '{"id": "123"}'
```

Shows the exact command (CLI) or HTTP request (API) that would be executed — without actually running it. For CLI tools, displays the full subprocess invocation with all argument tokens. For API tools, shows method, URL, headers, auth status, and body.

## Connecting to AI Clients

> **Tip:** Run `mcpw config --client <name>` to auto-generate the config snippet for any client below.

### Claude Desktop

Add to `~/.claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "mcpw": {
      "command": "mcpw",
      "args": ["serve"]
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json` in your project:

```json
{
  "mcpServers": {
    "mcpw": {
      "command": "mcpw",
      "args": ["serve"]
    }
  }
}
```

### Claude Code

Add to `.mcp.json` in your project:

```json
{
  "mcpServers": {
    "mcpw": {
      "command": "mcpw",
      "args": ["serve"]
    }
  }
}
```

### Remote clients (SSE)

The SSE endpoint is available at the same time as STDIO:

```bash
mcpw serve --port 8080
```

- `POST /mcp` — send JSON-RPC requests
- `GET /sse` — stream responses via Server-Sent Events

## Examples

### Wrap a Python script

```python
# resize.py
import argparse

parser = argparse.ArgumentParser(description="Resize an image")
parser.add_argument("--input", required=True, help="Input image path [required]")
parser.add_argument("--width", type=int, default=800, help="Target width [default: 800]")
parser.add_argument("--height", type=int, default=600, help="Target height [default: 600]")
parser.add_argument("--format", default="png", help="Output format [default: png]")
args = parser.parse_args()

print(f"Resized {args.input} to {args.width}x{args.height} as {args.format}")
```

```bash
mcpw register resize --cmd "python resize.py"
# => Registered tool 'resize' with 4 parameters

mcpw test resize --args '{"input": "photo.jpg", "width": 1024}'
# => Resized photo.jpg to 1024x600 as png
```

### Wrap curl for HTTP requests

```bash
mcpw register http --cmd "curl"
# => Registered tool 'http' with 9 parameters (auto-discovered from curl --help)

mcpw test http --args '{"silent": true}'
```

### Wrap a deployment script

```bash
#!/bin/bash
# deploy.sh
echo "Deploying to $1..."
```

```bash
mcpw register deploy --cmd "/scripts/deploy.sh" --desc "Deploy application"
mcpw test deploy
```

### Wrap ffmpeg

```bash
mcpw register transcode --cmd "ffmpeg"
mcpw inspect transcode
# Shows all discovered ffmpeg flags
```

## How Parameter Discovery Works

When you register a tool, `mcpw` runs the command with `--help` and parses the output using heuristic rules:

| Help output pattern | Discovered as |
|---|---|
| `--name <FILE>` | String parameter |
| `--count <INT>` | Integer parameter |
| `--rate <FLOAT>` | Float parameter |
| `--verbose` (no type) | Boolean flag |
| `--name VALUE` (bare metavar) | String parameter |
| `[default: X]` in description | Optional with default |
| `[required]` in description | Required parameter |

The parser handles both angle-bracket types (`<FILE>`) and bare metavars (`MESSAGE`) as used by Python's argparse.

`--help` and `--version` flags are automatically excluded.

## Security

- **Command blocklist** — dangerous commands (rm, shutdown, dd, bash, etc.) blocked by default. Customizable via `mcpw block` / `mcpw unblock`. Blocking also removes already-registered tools
- **No shell injection** — CLI tool arguments are passed as discrete `Command::arg()` tokens, never through a shell interpreter
- **No shell evaluation** — command strings are tokenized with `shell-words`, not executed via `sh -c`
- **Type validation** — MCP arguments are type-checked against the schema; unknown args are rejected
- **No secrets on disk** — API auth tokens are referenced by env var name, never stored in config files
- **CORS protection** — SSE server has restrictive CORS headers to prevent browser-based CSRF
- **Execution timeout** — CLI tools timeout after 30 seconds; API calls timeout after 30 seconds
- **Output cap** — CLI tool stdout/stderr capped at 1MB to prevent memory exhaustion

## Data Storage

```
~/.mcpw/
  tools.json       CLI tool registrations (from 'mcpw register')
  api_tools.json   API tool registrations (from 'mcpw import')
  blocklist.json   Blocked commands (from 'mcpw block', auto-created with defaults)
```

Both files use atomic writes (temp file + rename) to prevent corruption.

Set `MCPW_TOOLS_DIR` environment variable to use a custom directory.

## Platform Support

| Platform | Help Discovery | Status |
|---|---|---|
| macOS | `--help` → `-h` → `man` | Supported |
| Linux | `--help` → `-h` → `man` | Supported |
| Windows | `--help` → `-h` → `/?` | Supported |

## Development

```bash
cargo build              # Build
cargo test               # Run all tests (192 tests)
cargo clippy -- -D warnings  # Lint
cargo fmt --check        # Format check
```

## License

MIT
