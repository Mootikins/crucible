---
title: "Workspace Configuration"
description: Documentation note for workspaces.
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

### Project (`.crucible/project.toml`) and Kiln (`.crucible/kiln.toml`)

Project-level settings:
- Shell command whitelist/blacklist
- Resource access permissions
- Attached kilns
- Provider restrictions

### Kiln (`.crucible/kiln.toml`)

Kiln identity and metadata:
- Kiln name
- Data classification

> **Backward compatibility:** Crucible still reads `.crucible/workspace.toml` as a read-only fallback if neither `project.toml` nor `kiln.toml` exists. New setups should use the split config files.
## Workspaces vs Kilns

A **workspace** is where work happens—a project directory, repository, or development environment. It owns security policies.

A **kiln** is a knowledge system—your notes, documentation, or team knowledge base. It owns content preferences but has no security control.

A kiln is *attached to* a workspace. The same kiln can be attached to multiple workspaces with different security contexts.

## Setting Up a Workspace

### Implicit Discovery

Any directory with `.crucible/project.toml` or `.crucible/kiln.toml` is automatically recognized as a workspace:

```bash
mkdir -p myproject/.crucible
cat > myproject/.crucible/kiln.toml << 'EOF'
[kiln]
name = "myproject"
EOF

cat > myproject/.crucible/project.toml << 'EOF'
[[kilns]]
path = "docs"  # Relative path to kiln
EOF
```

### Registered Projects

For daemon mode or explicit control, register projects globally. Projects bind to one or more named kilns from the `[kilns]` registry.

```toml
# ~/.config/crucible/config.toml

[kilns]
docs = "~/crucible/docs"
shared = "~/shared-knowledge"

[projects.myproject]
path = "~/projects/myproject"
kilns = ["docs", "shared"]
default_kiln = "docs"
```

| Field | Type | Description |
|---|---|---|
| `path` | path | Project root directory |
| `kilns` | list | Named kilns from `[kilns]` that this project uses |
| `default_kiln` | string | Primary kiln for session storage |

## Shell Security

Plugins can execute shell commands via `shell::exec()`. This is controlled by whitelist/blacklist policies.

### Default Whitelist

Crucible ships with a default whitelist of common safe commands: `git`, `cargo`, `npm`, `docker`, etc.

### Workspace Customization

```toml title=".crucible/project.toml"
# .crucible/project.toml
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

## Restricting Providers Per Kiln

There is no per-project provider allow/deny list. What exists is trust-based: a kiln
carries a `data_classification`, a provider carries a `trust_level`, and Crucible refuses
to send classified content to a provider that is not trusted enough for it. See
[[Help/Concepts/Trust and Classification]].

## Splitting Configuration Across Files

Any string value in `config.toml` can be a reference that the loader resolves before
parsing:

| Reference | Resolves to |
|---|---|
| `{env:VAR}` | The environment variable's value |
| `{file:path}` | The file's contents — parsed as TOML for a `.toml` file, otherwise the trimmed text |
| `{dir:path}` | Every non-hidden `.toml` file in the directory, merged in filename order |

That makes drop-in directories work per *section*. Setting
`llm = "{dir:~/.config/crucible/llm.d/}"` at the top level of `config.toml` replaces the
whole `[llm]` section with the merged contents of that directory:

```
~/.config/crucible/
├── config.toml           # llm = "{dir:~/.config/crucible/llm.d/}"
└── llm.d/                # merged in filename order
    ├── 00-default.toml   # default = "local"
    └── 50-cloud.toml     # [providers.cloud] …
```

The reference is resolved best-effort: if the directory is missing, the raw string is left
in place and the config then fails to parse, so a typo surfaces immediately.

Use `{file:}` to keep a secret out of the config itself:

```toml
[llm.providers.work]
type = "openai"
api_key = "{file:~/.secrets/work-openai.key}"
```

## See Also

- [[Help/Config/llm]] - LLM provider configuration
- [[Help/Config/embedding]] - Embedding configuration
- [[Help/Extending/Creating Plugins]] - Writing plugins with shell access
