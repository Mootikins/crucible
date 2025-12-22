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

## File References

Crucible supports two ways to include external files:

### `{file:path}` References (Recommended)

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

Override configuration with environment variables:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_KILN_PATH` | Path to your kiln (Obsidian vault) |
| `CRUCIBLE_EMBEDDING_URL` | Embedding provider API URL |
| `CRUCIBLE_EMBEDDING_MODEL` | Model name for embeddings |
| `CRUCIBLE_EMBEDDING_PROVIDER` | Provider type (fastembed, ollama, openai) |
| `CRUCIBLE_PROFILE` | Active profile name |
| `CRUCIBLE_LOG_LEVEL` | Logging level (off, error, warn, info, debug, trace) |

## Profiles

Define multiple profiles for different environments:

```toml
profile = "development"  # Active profile

[profiles.development]
# Development-specific settings

[profiles.production]
# Production-specific settings
```

## Tips

1. **Secure API keys**: Store `mcps.toml` with `chmod 600` if it contains tokens
2. **Use environment variables** for secrets in shared configs
3. **Test your config**: Run `cru config show` to see effective configuration
4. **Validate**: Run `cru config validate` to check for errors
