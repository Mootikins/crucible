---
tags:
  - config
  - security
  - workspaces
---

# Workspace Configuration

Crucible uses a three-tier configuration system that separates security policies from content preferences.

## The Three Tiers

### Global (`~/.config/crucible/`)

User-wide settings that apply across all workspaces:
- Provider credentials (API keys)
- Default security policies
- Registered workspaces

### Workspace (`.crucible/workspace.toml`)

Project-level settings:
- Shell command whitelist/blacklist
- Resource access permissions
- Attached kilns
- Provider restrictions

### Kiln (`.crucible/config.toml` inside kiln)

Content preferences only:
- Embedding settings
- LLM preferences
- Hooks and discovery

## Workspaces vs Kilns

A **workspace** is where work happens—a project directory, repository, or development environment. It owns security policies.

A **kiln** is a knowledge system—your notes, documentation, or team knowledge base. It owns content preferences but has no security control.

A kiln is *attached to* a workspace. The same kiln can be attached to multiple workspaces with different security contexts.

## Setting Up a Workspace

### Implicit Discovery

Any directory with `.crucible/workspace.toml` is automatically recognized as a workspace:

```bash
mkdir -p myproject/.crucible
cat > myproject/.crucible/workspace.toml << 'EOF'
[workspace]
name = "myproject"

[[kilns]]
path = "docs"  # Relative path to kiln
EOF
```

### Registered Workspaces

For daemon mode or explicit control, register workspaces globally:

```toml
# ~/.config/crucible/workspaces.d/myprojects.toml
[[workspaces]]
name = "myproject"
path = "~/projects/myproject"
kilns = ["docs", "~/shared-knowledge"]
```

## Shell Security

Plugins can execute shell commands via `shell::exec()`. This is controlled by whitelist/blacklist policies.

### Default Whitelist

Crucible ships with a default whitelist of common safe commands: `git`, `cargo`, `npm`, `docker`, etc.

### Workspace Customization

```toml
# .crucible/workspace.toml
[security.shell]
# Add project-specific tools
whitelist = ["aws", "terraform"]

# Block specific subcommands
blacklist = ["docker run"]
```

### Interactive Approval

When a plugin tries a non-whitelisted command, you're prompted:

```
┌─ Shell command not whitelisted ─────────────────────────┐
│ Command: aws s3 ls                                      │
│ Plugin:  deploy.lua                                     │
│                                                         │
│ Whitelist:                                              │
│   [1] aws          [2] aws s3       [3] aws s3 ls       │
│   [d] Deny         [b] Block                            │
│                                                         │
│ Save to: (w)orkspace  (g)lobal  (o)nce                  │
└─────────────────────────────────────────────────────────┘
```

Choose the prefix granularity and where to save it.

## Provider Restrictions

Control which LLM providers are available in each workspace:

```toml
# .crucible/workspace.toml
[security.providers]
allowed = ["ollama-*"]           # Only local models
blocked = ["openai", "anthropic"] # No cloud providers
```

Providers can also restrict which workspaces they're available in:

```toml
# ~/.config/crucible/providers.d/work.toml
[providers.work-openai]
type = "openai"
api_key = "{file:~/.secrets/work-openai.key}"
allowed_workspaces = ["work-*"]  # Only work projects
```

## Drop-in Configuration

Global config supports a `config.d/` pattern for modular configuration:

```
~/.config/crucible/
├── config.toml           # Main config
├── config.d/             # Merged alphabetically
│   ├── 00-defaults.toml
│   └── 50-personal.toml
├── providers.d/          # Provider credentials
│   ├── anthropic.toml
│   └── ollama.toml
└── workspaces.d/         # Registered workspaces
    └── projects.toml
```

Reference directories in your main config:

```toml
[include]
providers = "{dir:~/.config/crucible/providers.d/}"
workspaces = "{dir:~/.config/crucible/workspaces.d/}"
```

## See Also

- [[Help/Config/llm]] - LLM provider configuration
- [[Help/Config/embedding]] - Embedding configuration
- [[Help/Extending/Creating Plugins]] - Writing plugins with shell access
