# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in mcpw, please report it responsibly.

**Do NOT open a public issue for security vulnerabilities.**

Instead, email

Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

You will receive a response within 48 hours.

## Security Model

mcpw executes commands as subprocesses. The security model is designed to prevent misuse:

### Protections

- **No shell injection** — arguments are passed via `Command::arg()`, never through a shell interpreter
- **Command blocklist** — dangerous commands (rm, shutdown, dd, bash, etc.) blocked by default
- **Unknown args rejected** — only parameters defined in the tool schema are accepted
- **Execution timeout** — 30-second timeout kills hung processes
- **Output cap** — stdout/stderr capped at 1MB
- **CORS protection** — SSE server rejects cross-origin browser requests
- **No secrets on disk** — API auth tokens referenced by env var name, never stored

### Known Limitations

- The SSE server does not have authentication. Do not expose it to untrusted networks without a reverse proxy.
- Tool execution runs with the same permissions as the mcpw process.
- The blocklist can be bypassed with `--allow-unsafe`.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | Yes       |
