---
title: "Configuration Reference"
description: Documentation note for Configuration.
tags: [help, configuration, reference]
---

# Configuration Reference

Crucible uses TOML configuration files. The main config file is at `~/.config/crucible/config.toml`.

## Quick Start

```toml
# Minimal config — single kiln (legacy shorthand, still supported)
kiln_path = "/home/user/notes"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
```

Or with the newer named kilns:

```toml
default_kiln = "vault"

[kilns]
vault = "~/vault"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
```

## Configuration Sections

### Root Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `kiln_path` | path | current dir | Path to your notes directory (kiln). Legacy — prefer `[kilns]`. |
| `default_kiln` | string | first alphabetically | Name of the default kiln (session storage, tool scoping) |
| `agent_directories` | list | `[]` | Additional directories to search for agent cards |

### [kilns] — Named Kiln Registry

Register kilns by name. Each entry can be a **shorthand** (just a path string) or a **full table** with extra options.

```toml
[kilns]
# Shorthand — path only
vault = "~/vault"
docs = "~/crucible/docs"

# Full form — path plus options
[kilns.work]
path = "~/work/notes"
lazy = true           # Don't open until explicitly requested
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | required | Filesystem path to the kiln root |
| `lazy` | bool | `false` | If true, the kiln is not opened at daemon start; it must be opened explicitly |

If `[kilns]` is empty or absent, Crucible falls back to `kiln_path` (synthesized as a kiln named `"default"`). When `[kilns]` is present, `kiln_path` is ignored.

### [projects.*] — Project Registry

Register projects (code repositories, workspaces) and bind them to kilns. The daemon auto-opens a project's kilns when a session starts in that directory.

```toml
[projects.crucible]
path = "~/crucible"
kilns = ["docs", "vault"]     # Kiln names from [kilns] section
default_kiln = "vault"        # Primary kiln for this project

[projects.website]
path = "~/website"
kilns = ["vault"]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | required | Filesystem path to the project root |
| `kilns` | list | `[]` | Named kilns this project uses (resolved from `[kilns]`) |
| `default_kiln` | string | first in list | Which kiln is primary for sessions in this project |

Projects are registered automatically by `cru init` when run inside a project directory, or manually by editing the config file.

### [chat] - Chat Configuration

Controls the chat interface and LLM settings for internal agents.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `model` | string | provider default | Model to use (e.g., "llama3.2", "gpt-4o") |
| `agent_preference` | string | `"crucible"` | Prefer `acp` (external) or `crucible` (internal) agents |
| `endpoint` | string | provider default | Custom API endpoint URL |
| `temperature` | float | `0.7` | Generation temperature (0.0-2.0) |
| `max_tokens` | int | `2048` | Maximum tokens to generate |
| `timeout_secs` | int | `120` | API timeout in seconds |
| `enable_markdown` | bool | `true` | Enable markdown rendering |
| `show_thinking` | bool | `false` | Show extended thinking/reasoning blocks in chat output |
| `show_diffs` | bool | `true` | Render diff bodies under edit/write tool calls |

There is no `provider` key here — a config containing `chat.provider` is rejected at load.
The provider is selected by `[llm].default`.

### [enrichment] - Embedding Configuration

Controls how text embeddings are generated for semantic search.

```toml
[enrichment.provider]
type = "fastembed"
```

**Provider types:** `fastembed` (default, local CPU), `ollama`, `openai`, `cohere`,
`vertexai`, `burn`, `custom`, `mock`. Each has its own fields — see
[[Help/Config/embedding|Embedding Configuration]].

Without an `[enrichment]` section the daemon skips embedding generation entirely.

> The older flat `[embedding]` section is no longer supported; a config containing it fails
> to load.

### [context] - Context Configuration

Controls how project context is loaded.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `rules_files` | list | see below | Files to search for project rules |

**Default rules files:** `["AGENTS.md", ".rules", ".github/copilot-instructions.md"]`

Rules files are loaded hierarchically from git root to workspace directory. See [[Rules Files]] for details.

```toml
[context]
# Add Cursor and Claude Code compatibility
rules_files = ["AGENTS.md", "CLAUDE.md", ".rules", ".cursorrules"]
```

### [cli] - CLI Behavior

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `show_progress` | bool | `true` | Show progress bars for long operations |
| `confirm_destructive` | bool | `true` | Confirm destructive operations |
| `verbose` | bool | `false` | Enable verbose logging |

### [llm] - Named LLM Providers

Define multiple LLM provider instances by name:

```toml
[llm]
default = "local"

[llm.providers.local]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"

[llm.providers.cloud]
type = "openai"
default_model = "gpt-4o"
api_key = "OPENAI_API_KEY"  # Uses env var
temperature = 0.9
max_tokens = 8192
```

### [mcp] - MCP Gateway Configuration

Configure upstream MCP (Model Context Protocol) servers to aggregate external tools.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `servers` | list | `[]` | List of upstream MCP server configurations |

Each server in the list has these options:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `name` | string | required | Unique identifier for this upstream |
| `prefix` | string | required | Prefix for tool names (must end with `_`) |
| `transport` | table | required | Connection configuration |
| `allowed_tools` | list | all | Whitelist of tool patterns (glob) |
| `blocked_tools` | list | none | Blacklist of tool patterns (glob) |
| `auto_reconnect` | bool | `true` | Reconnect on disconnect |
| `timeout_secs` | int | `30` | Tool call timeout |

**Transport types:**
- `stdio` - Spawn subprocess: `command`, `args`, `env`
- `sse` - HTTP SSE: `url`, `auth_header` — parses, but connecting is not implemented yet

```toml
[[mcp.servers]]
name = "github"
prefix = "gh_"

[mcp.servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp.servers.transport.env]
GITHUB_TOKEN = "{env:GITHUB_TOKEN}"
```

See [[Help/Config/mcp|MCP Configuration]] for full details.

### [logging] - Logging Configuration

```toml
[logging]
level = "info"           # off | error | warn | info | debug | trace
format = "text"          # text | json | compact
console = true
file = false
file_path = "~/.local/share/crucible/crucible.log"

[logging.component_levels]
crucible_core = "debug"
```

### Other sections

| Section | Covered in |
|---------|-----------|
| `[acp]`, `[acp.agents.*]` | [[Help/Config/acp|ACP Configuration]] |
| `[permissions]` | [[Help/Config/permissions|Permission Configuration]] |
| `[storage]` | [[Help/Config/storage|Storage Configuration]] |
| `[web]` | [[Help/Config/web|Web UI Configuration]] |
| `[scm]`, `[server]`, `[[schedules]]`, `[plugins.*]`, `runtimepath` | `docs/Config.toml` |

## Value References

Any string value can be a reference that the loader resolves before parsing.

| Reference | Resolves to |
|-----------|-------------|
| `{env:VAR}` | The environment variable's value |
| `{file:path}` | The file's contents — parsed as TOML when the path ends in `.toml`, otherwise used as a trimmed string |
| `{dir:path}` | Every non-hidden `.toml` file in the directory, merged in filename order |

```toml
[llm.providers.openai]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"

[llm.providers.work]
type = "openai"
api_key = "{file:~/.secrets/work-openai.key}"
```

Paths resolve relative to the config file's own directory, unless they start with `/`
(absolute) or `~/` (home). References work at any nesting depth and inside arrays.

For `{dir:}`, later files override earlier ones on conflicting keys; tables are
deep-merged and arrays are appended. That gives you `config.d`-style drop-ins per section:

```
~/.config/crucible/
├── config.toml          # llm = "{dir:~/.config/crucible/llm.d/}"
└── llm.d/
    ├── 00-base.toml
    ├── 10-local.toml
    └── 99-override.toml
```

> An `[include]` section also exists in the codebase, but the CLI config loader does not
> apply it — only the value references above. Use `{dir:}` or `{file:}`.

## Environment Variables

Some settings can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_CONFIG` | Path to the config file (same as `-C`) |
| `CRUCIBLE_CONFIG_DIR` | Directory containing `config.toml` |
| `CRUCIBLE_KILN` | Kiln path, when no `--kiln` flag and no ancestor `.crucible/` is found |
| `CRUCIBLE_HOME` | Daemon data root — project registry, default session storage, home kiln. Defaults to `~/.crucible` |
| `CRUCIBLE_SOCKET` | Daemon socket path |
| `CRUCIBLE_RUNTIME` | Runtime root for plugins, themes, and skills |
| `CRUCIBLE_PLUGIN_PATH` | Extra plugin search paths, prepended to the runtime path |
| `CRUCIBLE_LOG_FILE` | Log file path. Defaults to `~/.crucible/<command>.log` |

Provider API keys are referenced from the config with `{env:VAR_NAME}` rather than being
read from a fixed variable — see [[Help/Config/llm|LLM Configuration]].

## Config File Locations

There is one config file, resolved in this order:

1. `cru -C <path>` / `$CRUCIBLE_CONFIG`
2. `$CRUCIBLE_CONFIG_DIR/config.toml`
3. The platform config directory — `~/.config/crucible/config.toml` on Linux,
   `~/Library/Application Support/crucible/config.toml` on macOS,
   `%APPDATA%\crucible\config.toml` on Windows

A kiln's `.crucible/kiln.toml` holds only the kiln's display name, and a project's
`.crucible/project.toml` holds project metadata and security policy. Neither is a place to
put the sections on this page.

## Example Configurations

### Single Kiln (Simplest)

```toml
default_kiln = "notes"

[kilns]
notes = "~/notes"

[llm]
default = "local"

[llm.providers.local]
type = "ollama"
default_model = "llama3.2"
endpoint = "http://localhost:11434"

[enrichment.provider]
type = "fastembed"
```

### Multi-Kiln (Work / Personal Split)

```toml
default_kiln = "personal"

[kilns]
personal = "~/vault"
work = "~/work/notes"

[kilns.reference]
path = "~/reference-docs"
lazy = true                   # Only opened on demand

[projects.my-app]
path = "~/projects/my-app"
kilns = ["work"]

[projects.dotfiles]
path = "~/dotfiles"
kilns = ["personal"]

[llm]
default = "cloud"

[llm.providers.cloud]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"
```

### OpenAI Setup

```toml
[kilns]
vault = "~/vault"

[llm]
default = "cloud"

[llm.providers.cloud]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"

[enrichment.provider]
type = "openai"
api_key = "{env:OPENAI_API_KEY}"
model = "text-embedding-3-small"
```

### Mixed Setup (Local Embeddings, Cloud Chat)

```toml
[kilns]
vault = "~/vault"

[llm]
default = "cloud"

[llm.providers.cloud]
type = "openai"
default_model = "gpt-4o"
api_key = "{env:OPENAI_API_KEY}"

[enrichment.provider]
type = "fastembed"
model = "BAAI/bge-small-en-v1.5"
```

## Migrating from `kiln_path` to `[kilns]`

The old `kiln_path` field still works and is read for backward compatibility. When `[kilns]` is empty, `kiln_path` is synthesized as a kiln named `"default"`. To migrate:

**Before:**
```toml
kiln_path = "/home/user/notes"
```

**After:**
```toml
default_kiln = "notes"

[kilns]
notes = "/home/user/notes"
```

You can also run `cru init` in your kiln directory to register it by name, or `cru init` in a project directory to create a project entry with kiln bindings.

`CRUCIBLE_KILN` points Crucible at a kiln directly when no `--kiln` flag is given and no ancestor `.crucible/` directory is found.

## See Also

- [[Help/Config/permissions|Permission Configuration]] - Tool allow/deny rules
- [[Help/Concepts/Permission Precedence]] - Which layer wins when they disagree
- [[Help/Config/mcp|MCP Configuration]] - Upstream MCP server setup
- [[Help/Config/llm|LLM Configuration]] - Language model providers
- [[Help/Config/embedding|Embedding Configuration]] - Text embeddings
- [[Help/Config/workspaces|Workspace Configuration]] - Multi-workspace setup
- [[Rules Files]] - Project-specific agent instructions
- [[Help/Extending/Internal Agent]] - Built-in agent configuration
