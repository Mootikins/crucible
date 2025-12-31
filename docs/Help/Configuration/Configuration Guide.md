---
title: Configuration Guide
description: Complete guide to configuring Crucible
tags:
  - configuration
  - reference
---

# Configuration Guide

Crucible uses TOML configuration files stored in `~/.config/crucible/`.

## Quick Start

```bash
mkdir -p ~/.config/crucible
# Create config.toml with your settings
```

## File Structure

```
~/.config/crucible/
├── config.toml       # Main configuration file
├── mcps.toml         # MCP server configurations (included)
├── embedding.toml    # Embedding/API config (optional)
├── discovery.toml    # Discovery paths (optional)
├── hooks.toml        # Hook configs (optional)
└── profiles.toml     # Environment profiles (optional)
```

## Dynamic Values

Crucible supports dynamic value references that are resolved at config load time.

### `{env:VAR}` Environment Variables

Use `{env:VAR}` to read values from environment variables:

```toml
[providers.openai]
backend = "openai"
api_key = "{env:OPENAI_API_KEY}"

[providers.anthropic]
backend = "anthropic"
api_key = "{env:ANTHROPIC_API_KEY}"
```

This is the recommended way to handle API keys and secrets:
- Store secrets in your shell profile, `.env` file, or secret manager
- Reference them in config with `{env:VAR}` syntax
- The config file itself contains no secrets

### `{file:path}` File References

Use `{file:path}` **anywhere** in your config to pull in external content:

```toml
# Include a whole section from a TOML file
gateway = "{file:mcps.toml}"

# Include just a secret value (plain text file)
[embedding]
provider = "openai"
api_key = "{file:~/.secrets/openai.key}"

# Works in arrays too
extra_paths = ["{file:paths.txt}", "/static/path"]

# Works at any nesting level
[deep.nested.config]
secret = "{file:secret.txt}"
```

**How it works:**
- If the file has a `.toml` extension → parsed as structured TOML data
- Otherwise → file content is used as a string (whitespace trimmed)

### `{dir:path}` Directory References (config.d style)

Use `{dir:path}` to merge all `.toml` files from a directory:

```toml
# Include all provider configs from a directory
providers = "{dir:~/.config/crucible/providers.d/}"

# Drop-in MCP server configs
gateway = "{dir:mcps.d/}"
```

Files are processed in sorted order (alphabetically), allowing predictable overrides:

```
~/.config/crucible/providers.d/
├── 00-base.toml      # Processed first (defaults)
├── 10-local.toml     # Local Ollama config
├── 20-cloud.toml     # Cloud providers
└── 99-override.toml  # Processed last (overrides)
```

- Only `.toml` files are processed
- Hidden files (starting with `.`) are ignored
- Later files override earlier ones for conflicting keys
- Tables are deep-merged, arrays are appended

### Combining All Three

You can use `{env:}`, `{file:}`, and `{dir:}` references throughout your config:

```toml
[providers.cloud]
backend = "openai"
api_key = "{env:OPENAI_API_KEY}"           # From environment
endpoint = "{file:~/.config/endpoint.txt}"  # From file

# Modular provider configs
[llm]
providers = "{dir:~/.config/crucible/providers.d/}"
```

### `[include]` Section (Legacy)

The `[include]` section merges files into top-level sections:

```toml
[include]
gateway = "mcps.toml"
embedding = "embedding.toml"
```

### Path Resolution

| Path Format | Example | Resolution |
|-------------|---------|------------|
| Relative | `mcps.toml` | Same directory as main config |
| Home | `~/crucible/mcps.toml` | User's home directory |
| Absolute | `/etc/crucible/mcps.toml` | Exact path |

### Merge Behavior

When files are included:
- **TOML files** are parsed and merged as structured data
- **Plain text files** are used as string values (trimmed)
- **Tables** are deep-merged (nested keys combined)
- **Arrays** are appended

## MCP Server Configuration

Configure upstream MCP servers in `mcps.toml`. See [[MCP Gateway]] for details.

### Stdio Transport (spawn a process)

```toml
[[servers]]
name = "github"
prefix = "gh_"  # Tools become gh_search_code, gh_get_repo, etc.

[servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[servers.transport.env]
GITHUB_TOKEN = "ghp_your_token_here"
```

### SSE Transport (connect to HTTP endpoint)

```toml
[[servers]]
name = "remote-tools"
prefix = "remote_"

[servers.transport]
type = "sse"
url = "https://mcp.example.com/sse"
auth_header = "Bearer your-api-key"
```

### Tool Filtering

Control which tools are exposed:

```toml
[[servers]]
name = "github"
allowed_tools = ["search_*", "get_*"]  # Whitelist with glob patterns
blocked_tools = ["delete_*"]           # Blacklist (takes priority)
```

## Environment Variables

Crucible uses a minimal set of environment variables for system-level configuration:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_CONFIG` | Path to config file (default: `~/.config/crucible/config.toml`) |
| `CRUCIBLE_CONFIG_DIR` | Config directory path |
| `CRUCIBLE_LOG_LEVEL` | Logging level (off, error, warn, info, debug, trace) |
| `CRUCIBLE_TEST_MODE` | Enable test mode (for development) |

**For API keys and secrets**, use `{env:VAR}` syntax in your config file instead of dedicated Crucible environment variables. This gives you full control over which environment variables to use:

```toml
[providers.openai]
api_key = "{env:OPENAI_API_KEY}"  # or any env var you prefer
```

## Profiles

Define multiple profiles for different environments:

```toml
profile = "development"  # Active profile

[profiles.development]
# Development-specific settings

[profiles.production]
# Production-specific settings
```

## Storage Modes

Crucible supports two storage modes that control how the CLI connects to SurrealDB:

### Embedded Mode (Default)

```toml
[storage]
mode = "embedded"
```

The CLI connects directly to SurrealDB in-process. This is simple and fast for single-session use:

- **Pros**: Fast startup, no extra processes, simple setup
- **Cons**: File locked to one session, cannot run multiple cru commands simultaneously

Use embedded mode when:
- Running a single CLI session at a time
- Using scripts that call cru commands sequentially
- Maximum simplicity is desired

### Daemon Mode

```toml
[storage]
mode = "daemon"
idle_timeout_secs = 300  # Optional, default 5 minutes
```

The CLI connects through a shared daemon process that manages the database. The daemon is automatically started when needed and shuts down after `idle_timeout_secs` of inactivity with no connections.

- **Pros**: Multiple concurrent CLI sessions, shared connection pooling, better for workflows
- **Cons**: Slightly slower initial connection (may need to start daemon), extra process

Use daemon mode when:
- Running multiple cru commands in parallel (e.g., in separate terminals)
- Using editor integrations that query while you work
- Running background processing while using the TUI

### Socket Location

The daemon socket is created at:
- `$XDG_RUNTIME_DIR/crucible-db.sock` (if XDG_RUNTIME_DIR is set)
- `/tmp/crucible-db.sock` (fallback)

### Daemon Management

The daemon is managed automatically. For manual control:

```bash
# The daemon starts automatically when needed
# To force shutdown, kill the db-server process
pkill -f "cru db-server"
```

## Tips

1. **Secure API keys**: Use `{env:VAR}` syntax so secrets never appear in config files
2. **Use file references** for shared configs: `gateway = "{file:mcps.toml}"`
3. **Test your config**: Run `cru config show` to see effective configuration
4. **Validate**: Run `cru config validate` to check for errors
5. **Use daemon mode** if you frequently run multiple cru commands at once
