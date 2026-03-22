use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mcpw")]
#[command(version)]
#[command(about = "Turn any CLI tool into an MCP tool without writing code")]
#[command(long_about = "\
mcpw turns any command-line tool, script, or binary into an MCP (Model \
Context Protocol) server — so AI assistants like Claude can call your tools directly.

QUICK START:
  # Register a CLI tool
  mcpw register my_tool --cmd \"python script.py\"

  # Import an entire API from OpenAPI spec
  mcpw import https://api.example.com/openapi.json --type sse

  # Test it
  mcpw test my_tool --args '{\"input\": \"data.csv\"}'

  # Serve via MCP (both STDIO and SSE simultaneously)
  mcpw serve

TWO WAYS TO ADD TOOLS:
  1. CLI tools (register): Wrap any command-line tool, script, or binary. \
Parameters are auto-discovered from --help output.
  2. API tools (import): Import endpoints from an OpenAPI/Swagger spec. \
Each endpoint becomes an MCP tool that makes HTTP requests.

FILES AND DIRECTORIES:
  ~/.mcpw/                Directory created on first use
  ~/.mcpw/tools.json      CLI tool registrations (from 'register')
  ~/.mcpw/api_tools.json  API tool registrations (from 'import')
  ~/.mcpw/blocklist.json  Blocked commands (from 'block', auto-created with defaults)

  Both files use atomic writes (temp file + rename) to prevent corruption. \
Override the storage location with the MCPW_TOOLS_DIR environment variable.

PARAMETER DISCOVERY:
  mcpw auto-detects --flag style parameters from help output. The discovery \
chain is platform-aware:
    1. Runs <command> --help (all platforms)
    2. Falls back to <command> -h (all platforms)
    3. Falls back to man <command> on macOS/Linux
    4. Falls back to <command> /? on Windows
  Captures stdout first; if empty, uses stderr (only if it looks like help output, \
not error messages). Output is truncated at 64KB.

  Recognized parameter patterns:
    --name <FILE>        -> String parameter
    --name <PATH>        -> String parameter
    --count <INT>        -> Integer parameter
    --count <NUM>        -> Integer parameter
    --rate <FLOAT>       -> Float parameter
    --verbose            -> Boolean flag (no value, just presence)
    --name VALUE         -> String parameter (bare metavar, e.g. argparse)
    [default: X]         -> Optional with default value
    [required]           -> Required parameter
  --help and --version flags are automatically excluded from discovery.

  Tools with only short flags (-r, -f) or positional arguments are registered \
with zero parameters. They can still be called but the AI won't know about \
their flags.

SECURITY:
  Command blocklist: Dangerous commands (rm, shutdown, dd, bash, etc.) are blocked \
by default. Manage with 'mcpw block' and 'mcpw unblock'. Use --allow-unsafe to override.
  No shell injection: All arguments passed as discrete process arguments, never \
through a shell interpreter.
  Unknown args rejected: Only parameters defined in the tool schema are accepted.

MCP PROTOCOL:
  mcpw implements MCP (Model Context Protocol) as a JSON-RPC 2.0 server. \
It advertises 'tools' capability only (no resources, no prompts). Server info: \
name=\"mcpw\", version=\"1.0.0\", protocolVersion=\"2024-11-05\".

  Supported JSON-RPC methods:
    initialize      Returns server capabilities and info
    tools/list      Returns all registered tools with JSON Schema
    tools/call      Executes a tool and returns stdout (or stderr on error)
    ping            Health check

TOOL EXECUTION:
  CLI tools: Arguments are type-checked, then the command is invoked as a \
subprocess (no shell). Exit code 0 = success (stdout), non-zero = error (stderr). \
30-second timeout, 1MB output cap.
  API tools: Arguments are mapped to path params, query params, headers, or \
JSON body per the OpenAPI spec. HTTP request is made with configured auth. \
2xx = success (response body), other = error. 30-second timeout.

EXAMPLES:
  # Register a CLI tool
  mcpw register resize --cmd \"python resize.py\" --desc \"Resize images\"

  # Import an API
  mcpw import https://petstore.swagger.io/v2/swagger.json --type sse

  # Start the server
  mcpw serve --port 8080

  # Connect to Claude Desktop:
  {
    \"mcpServers\": {
      \"mcpw\": {
        \"command\": \"mcpw\",
        \"args\": [\"serve\"]
      }
    }
  }")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Register a command as an MCP tool (auto-discovers parameters)
    #[command(long_about = "\
Register a command-line tool as an MCP tool. mcpw will automatically \
discover parameters by running the command's help.

COMMAND BLOCKLIST:
  Dangerous commands (rm, shutdown, dd, bash, etc.) are blocked by default. \
Use 'mcpw block --list' to see blocked commands, 'mcpw block <cmd>' to add, \
'mcpw unblock <cmd>' to remove. Use --allow-unsafe to register a blocked command.

PARAMETER DISCOVERY CHAIN:
  1. <command> --help (all platforms)
  2. <command> -h (all platforms)
  3. man <command> (macOS/Linux only)
  4. <command> /? (Windows only)
  If all return empty, the tool is registered with zero parameters and a \
warning is printed. The tool is still callable — it just won't have typed parameters.

WHAT GETS STORED:
  The tool definition is saved to ~/.mcpw/tools.json (or $MCPW_TOOLS_DIR/tools.json). \
Each entry contains: name, command string, description, parameters (with name, type, \
required flag, default value), and registration timestamp.

COMMAND TOKENIZATION:
  The --cmd value is tokenized using shell-word splitting rules. Quoted paths \
with spaces are handled correctly:
    --cmd \"python /path/to/my script.py\"
  The first token is the executable; remaining tokens are static arguments \
prepended before any dynamic call-time arguments.

DESCRIPTION:
  If --desc is not provided, the description is auto-extracted from the first \
non-empty, non-usage line of the help output. If no description can be found, \
a default \"Registered tool: <name>\" is used.

EXAMPLES:
  # Register a Python script with auto-discovered params
  mcpw register resize_image --cmd \"python resize.py\"

  # Provide a custom description
  mcpw register deploy --cmd \"/scripts/deploy.sh\" --desc \"Deploy to prod\"

  # Overwrite an existing registration
  mcpw register deploy --cmd \"/scripts/deploy_v2.sh\" --force

  # Register a tool that has no --help (params will be empty)
  mcpw register list_files --cmd \"ls\"

  # Register for remote access via SSE
  mcpw register my_api --cmd \"python api.py\" --type sse

  # Command with spaces in path
  mcpw register my_tool --cmd \"python '/path/with spaces/script.py'\"")]
    Register {
        /// Unique tool name (snake_case, used as MCP tool identifier)
        name: String,

        /// The full command string to execute (e.g. "python script.py", "curl", "/path/to/tool")
        #[arg(long)]
        cmd: String,

        /// Human-readable description (auto-extracted from --help if omitted)
        #[arg(long)]
        desc: Option<String>,

        /// Transport type: stdio (local, for Claude Desktop/Cursor) or sse (remote, HTTP)
        #[arg(long, default_value = "stdio")]
        r#type: String,

        /// Overwrite an existing tool with the same name without prompting
        #[arg(long)]
        force: bool,

        /// Allow registering blocked commands (rm, shutdown, dd, etc.)
        #[arg(long)]
        allow_unsafe: bool,
    },

    /// List all registered tools (CLI and API)
    #[command(long_about = "\
Display a table of all registered tools — both CLI tools (from 'register') and \
API tools (from 'import'). Shows name, kind (CLI or API method), parameter count, \
transport type, and description.

COLUMNS:
  NAME        Tool name
  KIND        CLI (command-line tool) or API GET/POST/PUT/DELETE (imported endpoint)
  PARAMS      Number of parameters
  TYPE        Transport: STDIO (local) or SSE (remote HTTP)
  DESCRIPTION Tool description

EXAMPLE:
  mcpw list

  NAME                          KIND      PARAMS  TYPE    DESCRIPTION
  ─────────────────────────────────────────────────────────────────────
  ps_tool                       CLI       0       STDIO   List processes
  get_pet_by_id                 API GET   1       SSE     Find pet by ID
  create_user                   API POST  0       SSE     Create user")]
    List,

    /// Remove a registered tool
    #[command(long_about = "\
Remove a tool from the registry (~/.mcpw/tools.json). The tool will no \
longer appear in 'list' or be available when the MCP server is started. \
Returns exit code 1 if the tool is not found.

EXAMPLES:
  mcpw remove resize_image
  mcpw remove deploy
  mcpw remove --all              Remove all CLI and API tools")]
    Remove {
        /// Name of the tool to remove (not needed with --all)
        name: Option<String>,

        /// Remove ALL registered tools (CLI and API)
        #[arg(long)]
        all: bool,
    },

    /// Start the MCP server (serves both STDIO and SSE simultaneously)
    #[command(long_about = "\
Start the MCP server. Loads both CLI tools (from ~/.mcpw/tools.json) and \
API tools (from ~/.mcpw/api_tools.json) and serves them over BOTH transports \
simultaneously in a single process:

  STDIO   Reads JSON-RPC from stdin, writes to stdout. Used by Claude Desktop, \
Cursor, and most local MCP clients. They spawn mcpw as a child process \
and communicate via pipes.

  SSE     HTTP server on the specified port with two endpoints:
            POST /mcp  — send JSON-RPC requests, get JSON-RPC responses
            GET  /sse  — subscribe to server-to-client event stream
          Used by remote/network clients.

STDIO-typed tools are served on STDIO only. SSE-typed tools are served on SSE only. \
Both transports run concurrently. CLI tool calls use a thread pool; API tool calls \
use async HTTP.

For API tools that require authentication, set the auth environment variable \
before starting the server (e.g. API_TOKEN=tok_123 mcpw serve).

MCP PROTOCOL DETAILS:
  Server name: mcpw, version: 1.0.0
  Protocol version: 2024-11-05
  Capabilities: tools only (no resources, no prompts)
  Methods: initialize, tools/list, tools/call, ping

EXAMPLES:
  # Start with default SSE port (3000)
  mcpw serve

  # Start with custom port
  mcpw serve --port 8080

  # Progressive mode (recommended for 20+ tools — reduces token usage by 80-160x)
  mcpw serve --progressive

  # Bind to all interfaces (for remote access)
  mcpw serve --host 0.0.0.0 --port 3000

CLAUDE DESKTOP / CURSOR / CLAUDE CODE CONFIGURATION:
  {
    \"mcpServers\": {
      \"mcpw\": {
        \"command\": \"mcpw\",
        \"args\": [\"serve\"]
      }
    }
  }")]
    Serve {
        /// Port for SSE transport
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Bind host for SSE transport
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Enable progressive tool disclosure (recommended for 20+ tools).
        /// Instead of exposing all tool schemas upfront, serves 3 meta-tools
        /// (search_tools, get_tool_schema, call_tool) that let the LLM discover
        /// tools on-demand. Reduces token usage by 80-160x for large tool sets.
        #[arg(long)]
        progressive: bool,
    },

    /// Import API tools from an OpenAPI/Swagger spec
    #[command(long_about = "\
Import API endpoints from an OpenAPI (v3) or Swagger (v2) spec and register \
them as MCP tools. Each endpoint becomes a tool that makes HTTP requests.

The spec can be a URL or a local file (JSON or YAML).

TOOL NAMING:
  If the endpoint has an operationId (e.g., \"getUserById\"), it is used as \
the tool name in snake_case (\"get_user_by_id\"). Otherwise, the name is \
auto-generated from the HTTP method and path (GET /users/{id} -> get_users_by_id).

AUTHENTICATION:
  Secrets are never stored on disk. Instead, specify the name of an environment \
variable that holds the secret. At runtime, mcpw reads the env var and adds the \
auth header to every request.
  --auth-env MY_TOKEN --auth-type bearer  -> Authorization: Bearer <value>
  --auth-env MY_KEY --auth-type header --auth-header X-API-Key  -> X-API-Key: <value>
  --auth-env MY_CREDS --auth-type basic  -> Authorization: Basic <value>

FILTERING:
  Use --include and --exclude to control which endpoints are imported. \
Patterns support trailing wildcards (e.g., /users/*).

EXAMPLES:
  # Import all endpoints from a public API
  mcpw import https://petstore.swagger.io/v2/swagger.json

  # Import from a local file
  mcpw import ./openapi.yaml --type sse

  # Import with bearer auth
  mcpw import https://api.example.com/openapi.json \\
    --auth-env API_TOKEN --auth-type bearer

  # Import only specific endpoints
  mcpw import https://api.github.com/openapi.json \\
    --include \"/repos/*\" --exclude \"/repos/*/actions/*\"

  # Import with a prefix
  mcpw import https://api.stripe.com/openapi.json --prefix stripe

FILES:
  Imported API tools are stored in ~/.mcpw/api_tools.json, separate from \
CLI tools in tools.json. Both are served simultaneously by mcpw serve.")]
    Import {
        /// OpenAPI spec URL or local file path
        source: String,

        /// Transport type: stdio or sse
        #[arg(long, default_value = "sse")]
        r#type: String,

        /// Environment variable name holding the auth secret
        #[arg(long)]
        auth_env: Option<String>,

        /// Auth type: bearer, header, or basic
        #[arg(long, default_value = "bearer")]
        auth_type: String,

        /// Custom header name for auth-type=header (default: X-API-Key)
        #[arg(long)]
        auth_header: Option<String>,

        /// Include only paths matching this pattern (supports trailing *)
        #[arg(long)]
        include: Vec<String>,

        /// Exclude paths matching this pattern (supports trailing *)
        #[arg(long)]
        exclude: Vec<String>,

        /// Prefix for generated tool names (e.g., "github" -> "github_list_repos")
        #[arg(long)]
        prefix: Option<String>,

        /// Static header as "name=ENV_VAR" (value read from env var at runtime).
        /// Can be specified multiple times. Example: --header "x-client-id=MY_CLIENT_ID"
        #[arg(long)]
        header: Vec<String>,
    },

    /// Show full details of a registered tool (params, types, defaults)
    #[command(long_about = "\
Display the complete definition of a registered tool including all discovered \
parameters, their types, whether they're required, and default values. Reads \
from ~/.mcpw/tools.json (or $MCPW_TOOLS_DIR/tools.json). Returns exit \
code 1 if the tool is not found.

DISPLAYED FIELDS:
  Tool          The registered tool name
  Command       The full command string (as passed to --cmd)
  Description   Human-readable description
  Registered    UTC timestamp of registration
  Parameters    Each parameter with:
                  --name    The flag name
                  <Type>    String, Integer, Float, or Boolean
                  desc      Description text
                  [required]      If the parameter is required
                  [default: X]    If the parameter has a default value

EXAMPLE:
  mcpw inspect resize_image

  Tool: resize_image
  Command: python resize.py
  Description: Resize an image to given dimensions
  Registered: 2026-03-19T10:00:00Z
  Parameters: (3)
    --input              <String    > Path to input image [required]
    --width              <Integer   > Target width in pixels [default: 800]
    --verbose            <Boolean   > Enable verbose output")]
    Inspect {
        /// Name of the tool to inspect
        name: String,
    },

    /// Test a registered tool by executing it with arguments (built-in MCP client)
    #[command(long_about = "\
Test a registered tool without needing an external MCP client. This command \
acts as a built-in MCP client — it shows the tool schema (what AI clients see), \
sends the request, executes the command, and displays the result.

Use this to verify your tools work correctly before connecting them to Claude \
or other AI assistants.

EXECUTION PATH:
  This uses the exact same code path as a real MCP tools/call request:
  1. Looks up the tool definition from the registry
  2. Converts JSON arguments to typed values using the parameter schema
  3. Invokes the command as a subprocess (no shell interpreter)
  4. Returns stdout on success (exit 0) or stderr on failure (non-zero exit)

ARGUMENT FORMAT:
  Arguments are passed as a JSON object where keys are parameter names:
    --args '{\"name\": \"value\", \"flag\": true, \"count\": 42}'
  Types must match the tool's parameter schema:
    String parameters:   \"value\"
    Integer parameters:  42
    Float parameters:    3.14
    Boolean parameters:  true or false
  Boolean true adds the --flag; Boolean false omits it entirely.

OUTPUT SECTIONS:
  1. Tool Schema  — the full MCP JSON Schema (what AI clients see)
  2. Request      — the tool name and arguments being sent
  3. Result       — OK with stdout content, or ERROR with stderr content

EXIT CODES:
  0    Tool executed successfully (exit code 0)
  1    Tool failed (non-zero exit), tool not found, or invalid arguments

EXAMPLES:
  # Test with arguments
  mcpw test resize_image --args '{\"input\": \"photo.jpg\", \"width\": 640}'

  # Test a tool that takes no arguments
  mcpw test list_files

  # Test with boolean flags
  mcpw test my_tool --args '{\"verbose\": true, \"output\": \"result.txt\"}'

  # Test error handling
  mcpw test failing_tool")]
    Test {
        /// Name of the tool to test
        name: String,

        /// JSON arguments to pass (e.g. '{"key": "value", "flag": true}')
        #[arg(long, default_value = "{}")]
        args: String,

        /// Simulate progressive disclosure (shows the 3-step flow an LLM would follow:
        /// search_tools → get_tool_schema → call_tool)
        #[arg(long)]
        progressive: bool,
    },

    /// Block a command from being registered as an MCP tool
    #[command(long_about = "\
Manage the command blocklist. Blocked commands cannot be registered unless \
--allow-unsafe is used. A default blocklist ships with dangerous commands \
(rm, shutdown, dd, bash, etc.). You can add or remove entries.

EXAMPLES:
  mcpw block curl --reason \"Company policy: no HTTP from MCP\"
  mcpw block --list
  mcpw block --reset")]
    Block {
        /// Command to block (e.g. "curl", "wget")
        command: Option<String>,

        /// Reason for blocking
        #[arg(long, default_value = "Blocked by user")]
        reason: String,

        /// List all blocked commands
        #[arg(long)]
        list: bool,

        /// Reset blocklist to defaults
        #[arg(long)]
        reset: bool,
    },

    /// Unblock a previously blocked command
    #[command(long_about = "\
Remove a command from the blocklist, allowing it to be registered.

EXAMPLES:
  mcpw unblock chmod
  mcpw unblock curl")]
    Unblock {
        /// Command to unblock
        command: String,
    },

    /// Generate MCP client configuration snippet
    #[command(long_about = "\
Generate the JSON configuration snippet needed to connect an MCP client to mcpw. \
Supports Claude Desktop, Claude Code, Cursor, and VS Code.

Each client uses a slightly different config format and file location. This command \
outputs the exact snippet and tells you where to paste it.

SUPPORTED CLIENTS:
  claude-desktop    Claude Desktop app (macOS/Windows)
  claude-code       Claude Code CLI
  cursor            Cursor IDE
  vscode            VS Code with Copilot

EXAMPLES:
  mcpw config --client claude-desktop
  mcpw config --client vscode
  mcpw config --client cursor --port 8080
  mcpw config --client claude-code --progressive")]
    Config {
        /// Target client: claude-desktop, claude-code, cursor, vscode
        #[arg(long)]
        client: String,

        /// SSE port (included in config if not 3000)
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Include --progressive flag in config
        #[arg(long)]
        progressive: bool,
    },

    /// Export tool registrations as portable JSON
    #[command(long_about = "\
Export all registered tools (CLI and API) as a portable JSON bundle. \
Use this to share your tool setup with teammates, back up your config, \
or replicate across machines.

OUTPUT:
  JSON to stdout containing CLI tools, API tools, and version info.

EXAMPLES:
  mcpw export > my-tools.json
  mcpw export --tool resize_image > single-tool.json")]
    Export {
        /// Export only a specific tool by name
        #[arg(long)]
        tool: Option<String>,
    },

    /// Import tools from an exported JSON bundle
    #[command(
        name = "import-config",
        long_about = "\
Import tool registrations from a previously exported JSON bundle file. \
This adds all CLI and API tools from the bundle into your local registry, \
overwriting any existing tools with the same name.

EXAMPLES:
  mcpw import-config my-tools.json
  mcpw import-config /path/to/team-tools.json"
    )]
    ImportConfig {
        /// Path to the exported JSON file
        file: String,
    },

    /// Preview a tool call without executing it
    #[command(
        name = "dry-run",
        long_about = "\
Preview what a tool call would do without actually executing it. Shows the \
exact command (CLI) or HTTP request (API) that would be sent.

For CLI tools: displays the full command with all arguments expanded as \
discrete tokens — exactly what the subprocess would receive.

For API tools: displays the HTTP method, URL with path params substituted, \
query parameters, headers, auth status, and request body.

EXAMPLES:
  mcpw dry-run my_tool --args '{\"input\": \"data.csv\"}'
  mcpw dry-run get_user --args '{\"id\": \"123\"}'
  mcpw dry-run deploy"
    )]
    DryRun {
        /// Name of the tool to preview
        name: String,

        /// JSON arguments (same format as 'mcpw test')
        #[arg(long, default_value = "{}")]
        args: String,
    },

    /// Diagnose common configuration issues
    #[command(long_about = "\
Run diagnostic checks on your mcpw installation and registered tools. \
Verifies that the tools directory exists, configuration files are valid JSON, \
all registered CLI tool commands are reachable, API tool auth environment \
variables are set, and the blocklist is well-formed.

Each check reports PASS, WARN, or FAIL. Exit code 0 if no failures, 1 otherwise.

CHECKS PERFORMED:
  Tools directory      ~/.mcpw/ exists
  tools.json           Valid JSON with correct schema
  api_tools.json       Valid JSON with correct schema (if present)
  blocklist.json       Valid JSON (if present)
  CLI tool commands    Each registered tool's executable is in PATH
  API auth env vars    Environment variables for auth are set

EXAMPLES:
  mcpw doctor")]
    Doctor,

    /// Show server and tool status
    #[command(long_about = "\
Show the current status of mcpw — whether the server is running, how many \
tools are registered, SSE port status, and recent log activity.

Useful for monitoring and debugging. Exit code 0 if server is running, 1 if not.

OUTPUT:
  Server status (running/not running, PID)
  CLI and API tool counts
  SSE port and connectivity
  Log file status and last entry

EXAMPLES:
  mcpw status
  mcpw status --json
  mcpw status --port 8080")]
    Status {
        /// Check SSE on this port (default 3000)
        #[arg(long, default_value = "3000")]
        port: u16,

        /// Output as JSON (for scripting/monitoring)
        #[arg(long)]
        json: bool,
    },

    /// Re-discover parameters for registered tools (schema refresh)
    #[command(long_about = "\
Re-run help discovery for registered CLI tools and update their parameter \
schemas. Use this when the underlying tool has changed (new flags added, \
old flags removed) and you want the MCP schema to reflect the current state.

For each tool, re-runs the help parser, compares parameters, and updates \
the registration if changes are found.

OPTIONS:
  --tool NAME    Update only a specific tool
  --dry-run      Show what would change without saving
  --all          Update all CLI tools (default)

EXAMPLES:
  mcpw update --tool resize_image     Re-discover params for one tool
  mcpw update --all                   Refresh all tools
  mcpw update --dry-run               Preview changes without applying")]
    Update {
        /// Update only a specific tool by name
        #[arg(long)]
        tool: Option<String>,

        /// Preview changes without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Validate registered tools for schema drift
    #[command(long_about = "\
Validate registered CLI tools by re-running help discovery and comparing the \
current parameters against the stored schema. Detects schema drift — when a \
tool's flags change but the MCP registration hasn't been updated.

CHECKS:
  - Command executable exists and is reachable
  - Help output can be obtained
  - Current parameters match stored schema
  - Reports added/removed parameters

EXIT CODES:
  0    All tools valid (no drift)
  1    Drift or errors detected

EXAMPLES:
  mcpw validate                    Validate all CLI tools
  mcpw validate --tool resize      Validate a specific tool")]
    Validate {
        /// Validate only a specific tool by name
        #[arg(long)]
        tool: Option<String>,
    },

    /// View mcpw logs
    #[command(long_about = "\
View the mcpw log file (~/.mcpw/mcpw.log). Logs are written automatically \
for server events, tool calls, registrations, blocks, and errors.

Log format (human-readable):
  2026-03-21 10:00:01 [INFO]  Server started (PID 1234, STDIO: 2, SSE: 5)
  2026-03-21 10:00:05 [CALL]  ps_tool -> OK (45ms)
  2026-03-21 10:00:07 [CALL]  deploy -> ERROR (30000ms) timeout
  2026-03-21 10:00:10 [BLOCK] Rejected 'rm' — Destructive

EXAMPLES:
  mcpw logs                  Show last 50 lines
  mcpw logs --tail 100       Show last 100 lines
  mcpw logs --follow         Live stream (like tail -f)")]
    Logs {
        /// Number of lines to show
        #[arg(long, default_value = "50")]
        tail: usize,

        /// Follow the log in real-time (like tail -f)
        #[arg(long)]
        follow: bool,
    },
}
