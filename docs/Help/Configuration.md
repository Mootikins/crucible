---
title: "Configuration Reference"
description: Documentation note for Configuration.
tags: [help, configuration, reference]
---

# Configuration Reference

Crucible has one configuration file: `~/.config/crucible/init.lua`. It is
Lua, the daemon evaluates it exactly once at boot, and the configuration is
whatever the file has set when it finishes. A `config.toml` left over from an
earlier install sets nothing: the daemon stopped reading it, and warns once per
boot while the file is there. Run `cru config migrate` to move its values into
Lua (see [[#Migrating from config.toml]]).

## Quick Start

Create `~/.config/crucible/init.lua`:

```lua
cru.config.set({
  default_kiln = "notes",
  kilns = { notes = "~/notes" },

  llm = {
    default = "local",
    providers = {
      ["local"] = {
        type = "ollama",
        default_model = "llama3.2",
        endpoint = "http://localhost:11434",
      },
    },
  },
})
```

`cru config init` writes a commented example file. `cru doctor` evaluates
your file in isolation and reports the first error with its file and line.

## How configuration works

The daemon evaluates `init.lua` once, at boot, **before** it loads plugins.
The full contract lives in [[Help/Lua/Configuration|Lua Configuration]]; the load-bearing rules:

- **Any line may set any key; inside one file the last write wins.** The
  daemon reads the result when the file finishes. Between files the LAYER
  decides, not the order the daemon reads them in: a plugin's declared
  default never replaces what you saved, and a saved setting never replaces
  a line in your own file.
- **`cru.config.set` deep-merges.** Tables merge key by key; arrays and
  scalars replace. To replace a whole table instead of merging into it, put
  `__replace = true` inside it — the one replacement mechanism, spelled the
  same in Lua and over the `config.set` RPC.
- **A file that does not parse stops the daemon.** Crucible names the file
  and the line, and refuses to start: a mistyped bracket says nothing about
  what you meant, and a daemon that started on the defaults would report your
  whole config as "no config". The rule reaches one level down — a file
  `cru.include` loads, and a module under your own `lua/` directory, count as
  your config too.
- **A file that parses and then raises fails open.** The daemon warns with
  the file and line, discards everything the file did, and boots on the seed
  values. `cru doctor` reports the same error as a failed check. A plugin
  that does not parse fails open the same way: you did not write it.
- **Edits do not apply to a running daemon.** Every daemon-backed command
  warns when `init.lua` changed since the daemon booted; `cru daemon restart`
  applies it.
- **Values are code.** `os.getenv("OPENAI_API_KEY")`, a computed path, a
  loop over kilns — anything Lua can produce is a config value. Put *actions*
  in hooks: top-level side effects run on every evaluation (the daemon's
  boot, and each bootstrap command's throwaway evaluation).
- **A saved setting loses to your file.** The settings UI writes
  `settings.json` beside your `init.lua`. It beats a plugin's declared
  default and loses to any key your own file sets; a save of such a key is
  refused, and the refusal names the file and line to edit instead.
- **Modules resolve from `~/.config/crucible/lua/`.** `require("my.mod")`
  reads `lua/my/mod.lua` beside your `init.lua` — split a long config into
  modules and `require` them.

`cru config show` prints the effective config; `--sources` annotates every
leaf with where it came from (`default`, a plugin's declared default,
`settings.json`, or `init.lua` with its exact `file:line`).

The author is the *file* that wrote the value, not the moment it ran. A
plugin's `setup()` runs while your `init.lua` evaluates, but the value reads
as `plugin <name>` rather than as your own line, because a plugin supplies a
default and you supply a decision. Your `init.lua` outranks it.

## Configuration keys

Set every key below with `cru.config.set({...})`. The tables give the key's
shape; the examples are Lua.

### Root options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `kiln_path` | path | current dir | Path to your notes directory (kiln). Legacy — prefer `kilns`. |
| `default_kiln` | string | first alphabetically | Name of the default kiln (session storage, tool scoping) |
| `session_kiln` | path | *(unset)* | Kiln where `cru chat` stores sessions, if not the default kiln |
| `data_home` | path | `$CRUCIBLE_HOME`, else `~/.crucible` | Daemon data root — project registry, default session storage, home kiln |
| `agent_directories` | list | `[]` | **Deprecated.** Extra directories holding agent cards. Use `runtimepath` instead: one entry there supplies `agents/`, `skills/`, `plugins/` and `themes/` alike. Still honoured, warns once. |
| `runtimepath` | list | `[]` | Extra roots. Each entry's `agents/`, `skills/`, `plugins/` and `themes/` subdirectories are searched, ahead of the shipped runtime. |
| `runtimepath` | list | `[]` | *Extra* runtime roots for plugins and themes, searched after the well-known ones (`~/.config/crucible/runtime`, `$CRUCIBLE_RUNTIME`, next to the binary). Skills discovery does not read it yet |

The location-naming keys (`kiln_path`, `kilns`, `projects`, `data_home`,
`session_kiln`, `agent_directories`, `runtimepath`) freeze when the boot
evaluation ends: a runtime `config.set` — from a plugin or over RPC — cannot
change where the daemon acts. Your `init.lua` may set them freely; changing
them afterwards takes a restart.

### kilns — named kiln registry

Register kilns by name. Each entry is a path string, or a table with
options.

```lua
cru.config.set({
  kilns = {
    vault = "~/vault",
    docs = "~/crucible/docs",
    work = { path = "~/work/notes", lazy = true },
  },
})
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | required | Filesystem path to the kiln root |
| `lazy` | bool | `false` | If true, the kiln is not opened at daemon start; it must be opened explicitly |

If `kilns` is empty or absent, Crucible falls back to `kiln_path`
(synthesized as a kiln named `"default"`). When `kilns` is present,
`kiln_path` is ignored.

Config-declared kilns are one of two layers — see
[[#Registration versus authorship]] for the other.

### projects — project registry

Register projects (code repositories, workspaces) and bind them to kilns.
The daemon auto-opens a project's kilns when a session starts in that
directory.

```lua
cru.config.set({
  projects = {
    crucible = { path = "~/crucible", kilns = { "docs", "vault" } },
    website = { path = "~/website", kilns = { "vault" } },
  },
})
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `path` | string | required | Filesystem path to the project root |
| `kilns` | list | `[]` | Named kilns this project uses (resolved from `kilns`) |

`cru init` in a project directory registers the project with the **daemon**
(`projects.json`); declaring one here is the hand-authored alternative.

### chat — chat configuration

Controls the chat interface and LLM settings for internal agents.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `model` | string | provider default | Model to use (e.g., "llama3.2", "gpt-4o") |
| `agent_preference` | string | `"crucible"` | Prefer `acp` (external) or `crucible` (internal) agents |
| `endpoint` | string | provider default | Custom API endpoint URL |
| `show_thinking` | bool | `false` | Show extended thinking/reasoning blocks in chat output |
| `show_diffs` | bool | `true` | Render diff bodies under edit/write tool calls |

There is no `provider` key here — a config containing `chat.provider` is
rejected at load. The provider is selected by `llm.default`.

### enrichment — embedding configuration

Controls how text embeddings are generated for semantic search.

```lua
cru.config.set({
  enrichment = { provider = { type = "fastembed" } },
})
```

**Provider types:** `fastembed` (default, local CPU), `ollama`, `openai`,
`mock`. The types `cohere`, `vertexai`, `custom` and `burn` were removed: a
config that names one fails at load with an error that lists the supported
types. Each type has its own fields — see
[[Help/Config/embedding|Embedding Configuration]].

Without an `enrichment` key the daemon skips embedding generation entirely.

### context — context configuration

Controls how project context is loaded.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `rules_files` | list | see below | Files to search for project rules |

**Default rules files:** `["AGENTS.md", ".rules", ".github/copilot-instructions.md"]`

Rules files are loaded hierarchically from git root to workspace directory.
See [[Rules Files]] for details.

```lua
cru.config.set({
  context = {
    -- Add Cursor and Claude Code compatibility
    rules_files = { "AGENTS.md", "CLAUDE.md", ".rules", ".cursorrules" },
  },
})
```

### cli — CLI behavior

The removed fields `show_progress`, `confirm_destructive` and `verbose`
still load without an error; the values are ignored. Verbosity comes from
the `-v` CLI flag. The one `cli` feature is syntax highlighting:

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `highlighting.enabled` | bool | `true` | Enable syntax highlighting |
| `highlighting.theme` | string | `"base16-ocean.dark"` | Syntect theme name |

```lua
cru.config.set({
  cli = { highlighting = { theme = "base16-ocean.dark" } },
})
```

### llm — named LLM providers

Define multiple LLM provider instances by name:

```lua
cru.config.set({
  llm = {
    default = "local",
    providers = {
      ["local"] = {
        type = "ollama",
        endpoint = "http://localhost:11434",
        default_model = "llama3.2",
      },
      cloud = {
        type = "openai",
        default_model = "gpt-4o",
        api_key = os.getenv("OPENAI_API_KEY"),
      },
    },
  },
})
```

`llm` has three keys: `default` (which provider to use), `providers` (the
named instances above), and `models` — a specialty → model mapping used by
agent cards that declare a `specialty:` instead of a fixed `model:`:

```lua
cru.config.set({
  llm = {
    models = {
      reasoning = "openai/o1",
      coder = "qwen2.5-coder",  -- unprefixed = provider inherited
    },
  },
})
```

See [[Help/Config/llm|LLM Configuration]] for the full provider field
reference.

### mcp — MCP gateway configuration

Configure upstream MCP (Model Context Protocol) servers to aggregate
external tools.

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

```lua
cru.config.set({
  mcp = {
    servers = {
      {
        name = "github",
        prefix = "gh_",
        transport = {
          type = "stdio",
          command = "npx",
          args = { "-y", "@modelcontextprotocol/server-github" },
          env = { GITHUB_TOKEN = os.getenv("GITHUB_TOKEN") },
        },
      },
    },
  },
})
```

`mcp.servers` is an array, so a second `cru.config.set` with a `servers`
list **replaces** the first (arrays replace; tables merge). Build the whole
list in one place.

See [[Help/Config/mcp|MCP Configuration]] for full details.

### logging — logging configuration

```lua
cru.config.set({
  logging = { level = "info" },  -- off | error | warn | info | debug | trace
})
```

`level` is the **only** field the logging setup reads, and it applies to the
daemon's own process. It sets the base level when neither `--log-level` nor
`--verbose` is given (the flags win); `RUST_LOG` directives still override
it per target (`RUST_LOG=crucible_daemon=debug`). With nothing set at all,
the default is `warn` for server and stdio commands (`daemon serve`,
foregrounded `daemon start`, `web`, `chat`, `mcp --stdio`, `acp`) and `off`
for everything else. For a daemon-backed CLI invocation the config value
cannot apply before dispatch — use the flags or `RUST_LOG` for CLI log
level.

The section once carried eleven more keys — `format`, `console`, `file`,
`file_path`, `component_levels`, `rotation`, `max_file_size`, `max_files`,
`timestamps`, `target`, `ansi` — which parsed, validated, and reached
nothing. They are removed rather than left as settings that do nothing.

Log destination is decided by the command, not by config: stdio commands
(`chat`, `mcp --stdio`, `acp`) write to `~/.crucible/<command>.log`
(override the path with `CRUCIBLE_LOG_FILE`); everything else logs to
stderr.

### Other keys

| Key | What it holds | Covered in |
|---------|---------------|-----------|
| `acp`, `acp.agents.*` | External agents over ACP | [[Help/Config/acp|ACP Configuration]] |
| `permissions` | Tool allow/deny/ask rules | [[Help/Config/permissions|Permission Configuration]] |
| `web` | Browser UI served by `cru web` | [[Help/Config/web|Web UI Configuration]] |
| `workspace` | The default workspace directory the daemon scans, and the `scm.clone` destination | `docs/init.lua` |
| `server` | `auto_archive_hours` and `idle_shutdown_minutes`, and nothing else. `host`/`port` and the TLS keys were removed — the daemon binds a Unix socket and the web address is `web` | `docs/init.lua` |
| `schedules` | Recurring Lua snippets run on an interval — `cru.schedule` in `init.lua` is the native spelling | `docs/init.lua` |
| `plugins.*` | Free-form per-plugin tables, fed to that plugin's `setup(cfg)`; plus the reserved `plugins.declare` table below | [[Help/Lua/Configuration|Lua Configuration]] — the two plugin-config forms |

There is no `storage` key and no `discovery` key; both were removed.

### plugins.declare — git-hosted plugin declarations

Declare a plugin in your config and the daemon clones and loads it at every
boot. Each entry is a URL string, or a table with `url`, `branch`, `pin`
and `enabled`; the key must equal the URL-derived name (the last path
segment, without `.git`).

```lua
cru.config.set({
  plugins = {
    declare = {
      greeter = "user/greeter",
      review = { url = "someone/review", pin = "v1.2" },
    },
    -- Options stay per-plugin, beside the declarations:
    greeter = { greeting = "hello" },
  },
})
```

Declaration and configuration are different acts: `plugins.declare.<name>`
says the plugin should exist; `plugins.<name>` configures it. The name
`declare` is therefore reserved — a discovered plugin actually named
`declare` is refused at discovery with an error naming its path.

The machine's own record is separate: `cru plugin add` and the web install
button write `<data_home>/plugins.installed.json`, never your config. The
daemon loads the union, and when both name the same plugin your declaration
wins — the boot says so by name. `cru plugin remove` removes installed
plugins only; for a declared one it refuses and names the `file:line` of
the declaration, because Crucible never edits your config file.

`plugins.toml`, which used to hold declarations, is no longer read. Its
entries are imported into the installed manifest automatically, and the
boot warns while the leftover file exists; delete it to silence the
warning. (A kiln-local `.crucible/config.toml` needs no migration at all:
nothing ever read it, and `cru doctor` says so when one exists.)

## Secrets and computed values

Lua replaces the TOML reference syntax: read an environment variable with
`os.getenv`, a file with `io.open`, and build tables with plain code.

<!-- crucible:not-config -->
```lua
local key = os.getenv("OPENAI_API_KEY")
local work_key = assert(io.open(os.getenv("HOME") .. "/.secrets/work.key"))
    :read("l")
```

The TOML forms `{env:VAR}`, `{file:path}` and `{dir:path}` are gone with the
file that carried them. A string your Lua sets is the value, verbatim.

## Environment Variables

Some settings can be overridden via environment variables:

| Variable | Description |
|----------|-------------|
| `CRUCIBLE_CONFIG` | Path to the seed config file (same as `-C`); its directory is the config root |
| `CRUCIBLE_CONFIG_DIR` | The config root — where `init.lua` lives |
| `CRUCIBLE_KILN` | Kiln path, when no `--kiln` flag and no ancestor `.crucible/` is found |
| `CRUCIBLE_HOME` | Daemon data root — project registry, default session storage, home kiln. Defaults to `~/.crucible` |
| `CRUCIBLE_SOCKET` | Daemon socket path |
| `CRUCIBLE_RUNTIME` | Runtime root for plugins, themes, and skills |
| `CRUCIBLE_PLUGIN_PATH` | Extra plugin search paths, prepended to the runtime path |
| `CRUCIBLE_LOG_FILE` | Log file path. Defaults to `~/.crucible/<command>.log` |

## Config File Locations

There is one config **root**, resolved in this order:

1. The directory of `cru -C <path>` / `$CRUCIBLE_CONFIG`
2. `$CRUCIBLE_CONFIG_DIR`
3. The platform config directory — `~/.config/crucible` on Linux,
   `~/Library/Application Support/crucible` on macOS,
   `%APPDATA%\crucible` on Windows

The root holds `init.lua`, the `lua/` module directory and `settings.json`. A
daemon-backed command whose resolved root differs from the running daemon's is
refused, naming both roots.

`settings.json` is the machine's half of the config, and Crucible owns it: a
saved setting rewrites the file whole, with sorted keys. You may edit it by
hand, and the next save keeps what you wrote, but prefer `init.lua` — a key
your `init.lua` sets wins over the saved value, and the save is refused
rather than lost, naming the line that holds the key.

A save also replaces a value you set with `:set` for this run, so the value
you save is the value in force immediately. A save the daemon refuses changes
nothing: your `:set` value stands until the session ends.

The browser writes that file. In `cru web`, open the settings gear, then
**Configuration**: the section shows the same keys, with the daemon's own
descriptions. A key your `init.lua` holds shows as locked — the control is
disabled, the note names the file and the line, and a button opens that line
in the editor. A pin can sit inside a test on the hostname, so the note says
the line may be conditional on this host. A key that names where the daemon
acts shows read-only, with the reason it takes no control. The fonts, the
terminal size, the vim mode and the microphone stay in the browser: they are
per-device, and they do not reach `settings.json`.

The same dialog installs a plugin. In the **Plugins** section, give a git URL
or the `user/repo` shorthand, then confirm the URL. The plugin's declared
settings appear immediately, with no restart.

A kiln's `.crucible/kiln.toml` holds only the kiln's display name, and a
project's `.crucible/project.toml` holds project metadata and security
policy. Both stay TOML — they are identity manifests, not configuration —
and neither is a place to put the keys on this page. A kiln's
`.crucible/init.lua` (which `cru init` generates) loads into that kiln's
session runtimes, not into the daemon's config evaluation.

## Registration versus authorship

Two layers answer "which kilns exist", and they meet by name:

- **Config-declared** — the `kilns` / `projects` tables in your `init.lua`.
  You author these; the daemon never edits your file.
- **Registered** — what `cru init`, the chat preflight and
  `cru kiln register` told the daemon. The daemon records these in
  `<data_home>/kilns.json`, `projects.json` and `llm.json`, and they serve
  immediately, with no restart.

`cru kiln list` shows every entry with its origin (`config`, `registered`,
`discovered`) and marks a *shadowed* entry — a registered name that a config
declaration also uses; the config layer wins. Deleting a config line does
not unregister the state entry: `cru kiln forget <name>` /
`cru project forget <name>` is the removal. `forget` refuses on a
config-declared entry and names the declaring file and line.

A provider selection made by `cru init` takes effect immediately the first
time; changing an existing selection takes effect at the next daemon start,
and the command says which happened.

## Example Configurations

### Single kiln (simplest)

```lua
cru.config.set({
  default_kiln = "notes",
  kilns = { notes = "~/notes" },
  llm = {
    default = "local",
    providers = {
      ["local"] = {
        type = "ollama",
        default_model = "llama3.2",
        endpoint = "http://localhost:11434",
      },
    },
  },
  enrichment = { provider = { type = "fastembed" } },
})
```

### Multi-kiln (work / personal split)

```lua
cru.config.set({
  default_kiln = "personal",
  kilns = {
    personal = "~/vault",
    work = "~/work/notes",
    reference = { path = "~/reference-docs", lazy = true },
  },
  projects = {
    ["my-app"] = { path = "~/projects/my-app", kilns = { "work" } },
    dotfiles = { path = "~/dotfiles", kilns = { "personal" } },
  },
  llm = {
    default = "cloud",
    providers = {
      cloud = {
        type = "openai",
        default_model = "gpt-4o",
        api_key = os.getenv("OPENAI_API_KEY"),
      },
    },
  },
})
```

### Mixed setup (local embeddings, cloud chat)

```lua
cru.config.set({
  kilns = { vault = "~/vault" },
  llm = {
    default = "cloud",
    providers = {
      cloud = {
        type = "openai",
        default_model = "gpt-4o",
        api_key = os.getenv("OPENAI_API_KEY"),
      },
    },
  },
  enrichment = {
    provider = { type = "fastembed", model = "BAAI/bge-small-en-v1.5" },
  },
})
```

## Migrating from config.toml

`cru config migrate` converts a `config.toml` once, and verifies the result
before writing anything:

- Machine-written entries move to the state files where they belong: `auto`
  kiln entries (and a `default_kiln` naming one) go to `kilns.json`,
  `projects.*` entries to `projects.json`.
- The remaining, hand-authored keys are emitted as Lua. With no `init.lua`,
  the chunk **becomes** your `init.lua`; with an existing one, it is written
  to `lua/migrated_config.lua` and the command prints the one
  `require("migrated_config")` line to add — no tool edits an existing Lua
  file.
- `config.toml` is renamed `config.toml.migrated`.

Until you run it, the file sets nothing. v0.30.0 read it as a seed under your
`init.lua` and warned once per boot; this release drops the reader, and the
boot warns that the file no longer applies, naming the command that ends it.

Migrating from the ancient `kiln_path` form is the same move spelled small:
`kiln_path = "/home/user/notes"` becomes
`kilns = { default = "/home/user/notes" }` — or register the kiln with
`cru init` and author nothing.

## See Also

- [[Help/Lua/Configuration|Lua Configuration]] - The boot order, the merge rule, plugins, statusline, themes
- [[Help/Config/permissions|Permission Configuration]] - Tool allow/deny rules
- [[Help/Concepts/Permission Precedence]] - Which layer wins when they disagree
- [[Help/Config/mcp|MCP Configuration]] - Upstream MCP server setup
- [[Help/Config/llm|LLM Configuration]] - Language model providers
- [[Help/Config/embedding|Embedding Configuration]] - Text embeddings
- [[Help/Config/workspaces|Workspace Configuration]] - Multi-workspace setup
- [[Rules Files]] - Project-specific agent instructions
- [[Help/Extending/Internal Agent]] - Built-in agent configuration
