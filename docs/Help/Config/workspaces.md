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

### Kiln Attachment Fields

Each `[[kilns]]` entry in `.crucible/project.toml` takes three fields:

```toml title=".crucible/project.toml"
[[kilns]]
path = "./notes"
name = "Main Notes"
data_classification = "confidential"   # public | internal | confidential
```

| Field | Type | Description |
|---|---|---|
| `path` | path | Kiln directory — absolute, or relative to the project root. Required |
| `name` | string | Optional display label. **Parsed but unread** — it round-trips through config rewrites, but nothing consults it at runtime today |
| `data_classification` | string | `"public"`, `"internal"`, or `"confidential"` (lowercase). Optional |

`data_classification` is what the trust gates read: the daemon resolves a kiln's
classification from its `[[kilns]]` entry, and multi-kiln search skips any non-primary
kiln whose classification exceeds the session provider's `trust_level`. An entry with no
classification resolves to *none*, which the search filter treats as public. See
[[Help/Concepts/Trust and Classification]].

## Project File Access from the Web UI

Alongside `shell`, the `[security]` table in `.crucible/project.toml` has one more knob:

```toml title=".crucible/project.toml"
[security]
project_files = "read-only"   # read-write (default) | read-only | off
```

It governs how the **web UI** (`cru web`) may touch files inside the registered project
root that are *outside any attached kiln* — source code, configs, README. Kiln notes are
always read-write; this policy is only about the project file tree. Values are
kebab-case:

| Value | Effect |
|---|---|
| `read-write` | Open and save any file under the project root (the default) |
| `read-only` | Files open, but saves are refused |
| `off` | Project files are not served by the web UI at all (kiln notes only) |

It is enforced by the web server's file routes — the file browser's open and save
paths, media serving under a project root, and canvas documents that live under one.
The CLI, TUI, and agent tools do not consult it.

## Shell Security

The `bash` tool honours a per-project shell policy from `.crucible/project.toml`:

```toml title=".crucible/project.toml"
# .crucible/project.toml
[security.shell]
# Non-empty whitelist restricts commands to these prefixes
whitelist = ["git", "cargo", "aws", "terraform"]

# Blacklist blocks these prefixes (wins over the whitelist)
blacklist = ["docker run"]
```

Both lists are **prefix matches**, checked per shell statement — a chained command
(`git log; curl …`) is split on `;`, `&&`, `||`, and `|` (operators inside quotes are
left alone; a bare newline is **not** a split point), and every statement must pass, so
an unrelated command can't ride a whitelisted prefix. An unset or empty
policy imposes nothing; there is no built-in default whitelist in effect.

A violating command is **refused with an error**, not prompted for — there is currently
no interactive approval UI for shell-policy violations. (Tool permissions in
[[Help/Config/permissions]] are the layer that can prompt.) The policy is
defense-in-depth against straightforward misuse, not a sandbox: env tricks and `eval`
are out of scope.

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
