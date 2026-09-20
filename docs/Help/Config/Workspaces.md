---
title: "Project and Kiln Configuration"
description: Documentation note for projects and kilns.
tags:
  - config
  - security
  - projects
  - kilns
---

# Project and Kiln Configuration

Crucible uses a three-tier configuration system that separates security policies from content preferences.

## The Three Tiers

### Global (`~/.config/crucible/`)

User-wide settings that apply across all projects:
- Provider credentials (API keys)
- Default security policies
- Registered projects

### Project (`.crucible/project.toml`) and Kiln (`.crucible/kiln.toml`)

Project-level settings:
- Shell command whitelist/blacklist
- Resource access permissions
- Attached kilns

### Kiln (`.crucible/kiln.toml`)

Kiln identity and metadata:
- Kiln name
- Data classification

> **Backward compatibility:** Crucible still reads `.crucible/workspace.toml` as a read-only fallback if neither `project.toml` nor `kiln.toml` exists. New setups should use the split config files.

## The `.crucible/` Directory

One marker covers both roles. `.crucible/` is where a **project** keeps its policy
(`project.toml`) and where a **kiln** keeps its identity (`kiln.toml`) and the index
Crucible builds from its notes (`crucible-sqlite.db`). [[Help/Config/storage]] says what
is in the database. A workspace — one running instance of a project directory — owns no
config file of its own.

Discovery is an *upward walk*. `cru` takes the nearest ancestor holding a `.crucible/`
as the kiln, and the daemon matches a session's directory against registered projects
by walking ancestors the same way. The marker belongs at the root of the tree it
governs — a repository's root for a project, a notes directory for a kiln, which need
not be a git repository at all — so that everything beneath it resolves to one kiln.

**You do not make `.crucible/` by hand.** `cru init` creates it for a new kiln, and the
first `cru chat` in a kiln whose `.crucible/` holds no `init.lua` scaffolds one. (The
daemon's own data root is also a `.crucible/`, under `$HOME`, and is created on first
run; the walk deliberately skips it, because a kiln that resolved to the home directory
would put your whole home tree in the indexing and file scope.)

Two consequences of the upward walk are worth knowing, because both are quiet:

- A `.crucible/` *below* the tree root — inside a subdirectory of a repository, say —
  wins the walk for everything run beneath it, so one repository becomes two kilns with
  two indexes. A project's `.crucible/` belongs at the repository root only, so one
  found deeper is an accident: delete it and keep the root's.
- A directory with no `.crucible/` anywhere above it gives the walk nothing. Crucible
  does not invent one and does not fall back to the home directory: run `cru init` in
  the directory you mean, or register a kiln with `cru kiln register` for the daemon to
  fall back on.

## Projects, Workspaces and Kilns

A **project** is where work output goes — a repository or directory you register. It owns the security policies (the `[security]` tables of `.crucible/project.toml`).

A **workspace** is one running instance of a project directory — the root or a worktree. It is a runtime concept and owns no config file.

A **kiln** is where knowledge lives — your notes and documentation. It carries its identity and `data_classification`, not access policy.

A project *binds* kilns through `.crucible/project.toml`, and a session attaches a flat set of kilns. The same kiln can serve several projects, with a different `data_classification` per attachment.

## Setting Up a Project

### Implicit Discovery

A directory becomes the kiln for everything run beneath it the moment it holds a
`.crucible/` — the upward walk above needs no registration. `cru init` creates that
directory and writes the files; for a directory that is both a project and a kiln,
they are:

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

For daemon mode or explicit control, register projects globally. Projects bind to one or more named kilns from the `kilns` registry.

```lua
-- ~/.config/crucible/init.lua
cru.config.set({
    kilns = {
        docs = "~/crucible/docs",
        shared = "~/shared-knowledge",
    },
    projects = {
        myproject = {
            path = "~/projects/myproject",
            kilns = { "docs", "shared" },
        },
    },
})
```

| Field | Type | Description |
|---|---|---|
| `path` | path | Project root directory |
| `kilns` | list | Named kilns from `kilns` that this project uses |

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
classification from its `kilns` entry, and multi-kiln search skips any non-primary
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

Your config is Lua, so a long one splits the way any Lua program does. There
is no reference syntax to learn: read a value with a function call.

| Want | Write |
|---|---|
| An environment variable | `os.getenv("VAR")` |
| A file's contents | `assert(io.open(path)):read("a")` |
| Another config file beside `init.lua` | `cru.include("llm.lua")` |
| A module under `~/.config/crucible/lua/` | `require("my.llm")` |

`cru.include` and `require` both count as your own config: a file either of
them loads is held to the same rule as `init.lua`, so a syntax error in it
names its own file and line and stops the daemon.

A drop-in directory is a loop, not a feature:

```lua
-- ~/.config/crucible/init.lua
for _, name in ipairs({ "00-default", "50-cloud" }) do
    cru.include("llm.d/" .. name .. ".lua")
end
```

```
~/.config/crucible/
├── init.lua              # the loop above
└── llm.d/
    ├── 00-default.lua    # cru.config.set({ llm = { default = "local" } })
    └── 50-cloud.lua      # cru.config.set({ llm = { providers = { cloud = … } } })
```

Each included file calls `cru.config.set` itself, and the leaf rule does the
rest: a later file wins per key, and a key no later file names stands. To drop
a key an earlier file set, use `config.unset` — a `cru.config.set` write adds
and changes keys, and never removes one.

Keep a secret out of the config by reading it where it lives:

<!-- crucible:not-config — reads a key file that only exists on the reader's machine -->
```lua
cru.config.set({
    llm = {
        providers = {
            work = {
                type = "openai",
                api_key = assert(io.open(os.getenv("HOME") .. "/.secrets/work-openai.key")):read("l"),
            },
        },
    },
})
```

## See Also

- [[Help/Concepts/Kilns]] - What a kiln is, and the fields in `.crucible/kiln.toml`
- [[Help/Config/llm]] - LLM provider configuration
- [[Help/Config/embedding]] - Embedding configuration
- [[Help/Extending/Creating Plugins]] - Writing plugins with shell access
