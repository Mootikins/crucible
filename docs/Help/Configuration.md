---
tags: [help, configuration, reference]
---

# Configuration Reference

Crucible uses TOML configuration files. The main config file is at `~/.config/crucible/config.toml`.

## Quick Start

```toml
# Minimal config - just set your kiln path
kiln_path = "/home/user/notes"

[chat]
provider = "ollama"
model = "llama3.2"
```

## Configuration Sections

### Root Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `kiln_path` | path | current dir | Path to your notes directory (kiln) |
| `agent_directories` | list | `[]` | Additional directories to search for agent cards |

### [chat] - Chat Configuration

Controls the chat interface and LLM settings for internal agents.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `model` | string | provider default | Model to use (e.g., "llama3.2", "gpt-4o") |
| `provider` | string | `"ollama"` | LLM provider: `ollama`, `openai`, `anthropic` |
| `agent_preference` | string | `"acp"` | Prefer `acp` (external) or `crucible` (internal) agents |
| `endpoint` | string | provider default | Custom API endpoint URL |
| `temperature` | float | `0.7` | Generation temperature (0.0-2.0) |
| `max_tokens` | int | `2048` | Maximum tokens to generate |
| `timeout_secs` | int | `120` | API timeout in seconds |
| `enable_markdown` | bool | `true` | Enable markdown rendering |
| `size_aware_prompts` | bool | `true` | Enable size-aware prompt optimization for small models |

**Size-aware prompts:** When enabled, models under 4B parameters get explicit tool guidance and read-only tools only, preventing tool loops. Disable if you want to experiment with small models having full tool access.

### [acp] - Agent Client Protocol

Controls external agent communication (Claude Code, OpenCode, etc.).

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `default_agent` | string | auto-discover | Default agent to use |
| `enable_discovery` | bool | `true` | Enable agent auto-discovery |
| `session_timeout_minutes` | int | `30` | Session timeout |
| `max_message_size_mb` | int | `25` | Maximum message size in MB |
| `streaming_timeout_minutes` | int | `15` | Timeout for streaming responses |
| `lazy_agent_selection` | bool | `true` | Show agent picker on startup |

#### [acp.agents.<name>] - Agent Profiles

Define custom agent profiles with environment overrides:

```toml
[acp.agents.opencode-local]
env.LOCAL_ENDPOINT = "http://localhost:11434/v1"
env.OPENCODE_MODEL = "ollama/llama3.2"

[acp.agents.claude-proxy]
extends = "claude"
env.ANTHROPIC_BASE_URL = "http://localhost:4000"

[acp.agents.custom-agent]
command = "/usr/local/bin/my-agent"
args = ["--mode", "acp"]
env.MY_API_KEY = "secret"
```

### [embedding] - Embedding Configuration

Controls how text embeddings are generated for semantic search.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `provider` | string | `"fastembed"` | Provider type (see below) |
| `model` | string | provider default | Model name |
| `api_url` | string | provider default | API endpoint for remote providers |
| `batch_size` | int | `16` | Batch size for processing |
| `max_concurrent` | int | provider default | Max concurrent embedding jobs |

**Providers:**
- `fastembed` - Local CPU-friendly (default: `BAAI/bge-small-en-v1.5`)
- `ollama` - Local Ollama (default: `nomic-embed-text`)
- `openai` - OpenAI API (default: `text-embedding-3-small`)
- `anthropic` - Anthropic API
- `burn` - Local GPU via Burn framework
- `llamacpp` - Local GPU via llama.cpp

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
- `sse` - HTTP SSE: `url`, `auth_header`

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

### [processing] - Processing Configuration

Controls how notes are processed during indexing.

```toml
[processing]
# Processing options here
```

### [logging] - Logging Configuration

```toml
[logging]
# Logging options here
```

## Environment Variables

Some settings can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_KILN_PATH` | Override kiln path |
| `OPENAI_API_KEY` | OpenAI API key |
| `ANTHROPIC_API_KEY` | Anthropic API key |

## Config File Locations

- **Global config:** `~/.config/crucible/config.toml`
- **Workspace config:** `.crucible/config.toml` (in project root)

Workspace config overrides global config.

## Example Configurations

### Local Ollama Setup

```toml
kiln_path = "/home/user/notes"

[chat]
provider = "ollama"
model = "llama3.2"
endpoint = "http://localhost:11434"

[embedding]
provider = "ollama"
model = "nomic-embed-text"
```

### Remote Ollama (e.g., llama-swappo)

```toml
kiln_path = "/home/user/notes"

[chat]
provider = "ollama"
model = "qwen2.5-coder-32b"
endpoint = "https://llama.example.com"

[embedding]
provider = "ollama"
model = "nomic-embed-text"
api_url = "https://llama.example.com"
```

### OpenAI Setup

```toml
kiln_path = "/home/user/notes"

[chat]
provider = "openai"
model = "gpt-4o"

[embedding]
provider = "openai"
model = "text-embedding-3-small"
```

### Mixed Setup (Local embeddings, Cloud chat)

```toml
kiln_path = "/home/user/notes"

[chat]
provider = "openai"
model = "gpt-4o"

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
```

### Small Model Optimization Disabled

```toml
[chat]
model = "granite-3b"
size_aware_prompts = false  # Give small model all tools
```

## See Also

- [[Help/Config/mcp|MCP Configuration]] - Upstream MCP server setup
- [[Help/Config/llm|LLM Configuration]] - Language model providers
- [[Help/Config/embedding|Embedding Configuration]] - Text embeddings
- [[Help/Config/workspaces|Workspace Configuration]] - Multi-workspace setup
- [[Rules Files]] - Project-specific agent instructions
- [[Help/Extending/Internal Agent]] - Built-in agent configuration
