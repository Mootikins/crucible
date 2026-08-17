---
title: "CLI Command Reference"
description: Complete reference for Crucible CLI commands
tags:
  - help
  - cli
  - reference
status: implemented
---

# CLI Command Reference

Complete reference for all Crucible CLI commands.

## Core Commands

| Command | Description |
|---------|-------------|
| `cru chat` | Interactive AI chat with session persistence and tool access |
| `cru process` | Process markdown files through the pipeline (parse, enrich, store) |
| `cru search` | Search kiln notes using semantic and/or text search |
| `cru init` | Initialize a new kiln (Crucible workspace) |
| `cru kiln` | Manage the kilns Crucible knows about (register) — [[Help/CLI/kiln]] |
| `cru stats` | Display kiln statistics |
| `cru status` | Display storage status and statistics for the knowledge base |
| `cru models` | List available models from configured LLM provider |

## Agent & Integration Commands

| Command | Description |
|---------|-------------|
| `cru agents` | Manage agent cards (list, show, validate) |
| `cru mcp` | Start MCP server exposing Crucible tools (SSE on port 3847 by default; `--stdio` for stdio transport) |
| `cru acp` | Run Crucible as an ACP agent for editors (speaks ACP over stdio) — [[Help/CLI/acp]] |
| `cru skills` | Discover and manage agent skills (list, show, search) |
| `cru tools` | List tools available to agents (list) |

## Session & Configuration Commands

| Command | Description |
|---------|-------------|
| `cru session` | Manage chat sessions (create, configure, send, pause, resume, end, list, show, search; also open, export, reindex, cleanup, load; hidden debugging subcommands: subscribe, replay, unpause) — [[Help/CLI/session]] |
| `cru config` | Manage Crucible configuration (init, show, dump; `show --sources`/`--trace` traces where values came from) |
| `cru auth` | Manage LLM provider credentials (login, logout, list) |
| `cru set` | Configure a running session's settings (same syntax as TUI :set) |
| `cru proposals` | Review reflection-pass proposals (list, show, accept, reject) |

## System & Development Commands

| Command | Description |
|---------|-------------|
| `cru daemon` | Manage the Crucible daemon (start, stop, restart, status, logs; `serve` runs it in the foreground) |
| `cru storage` | Storage info (mode, stats). `verify`, `cleanup`, `backup`, and `restore` are parsed but call daemon stubs that are not yet implemented — they print a warning and exit 0 |
| `cru workflow` | Workflow notes (`type: workflow` frontmatter): list, show, start, approve, status, cancel |
| `cru tasks` | Manage tasks from a TASKS.md file (list, next, pick, done, blocked) |
| `cru plugin` | Manage and develop Lua plugins |
| `cru install` | Install a plugin from a git URL (alias for `cru plugin add`) |
| `cru lua` | Evaluate Lua code in the daemon's plugin runtime — [[Help/CLI/lua]] |
| `cru setup` | Bootstrap the runtime directory (bundled plugins, themes, template init.lua) — [[Help/CLI/setup]] |
| `cru web` | Start the web UI server for browser-based chat (`cru web webhook` mints webhook secrets) |
| `cru doctor` | Run installation diagnostics (daemon, config, providers, kiln, embeddings) |
| `cru completions` | Generate shell completion scripts (bash, zsh, fish) |

## Global Options

```
-l, --log-level <LEVEL>     Set log level (off, error, warn, info, debug, trace)
-v, --verbose               Enable verbose logging (--log-level=debug)
-C, --config <PATH>         Config file path (defaults to ~/.config/crucible/config.toml)
    --embedding-url <URL>   Embedding service URL (overrides config)
    --embedding-model <MODEL> Embedding model name (overrides config)
    --standalone            Run with in-process daemon (no background server required)
-h, --help                  Show help
-V, --version               Print version
```

## Output formats

Commands that render a result take `-f/--format`. There is no global
`--format` — each command declares its own, because they do not all offer the
same thing.

| Vocabulary | Commands | Values |
|---|---|---|
| Record lists | `search`, `models`, `tools list`, `skills list`, `proposals list`, `workflow list` | `table`, `json`, `plain` |
| Reports and trees | `stats`, `status`, `doctor`, `workflow show` | `text`, `json` |
| Config | `config show`, `config dump` | `toml`, `json` |
| Sessions | `session list`, `session show`, … | `text`, `json` (`markdown` on `show`) |

**The default depends on where output is going.** For the record-list commands, a
terminal gets `table` and a pipe or a redirect gets `plain`, so `cru models`
reads well on screen and piping it gets one record per line without you passing
a flag. An explicit `--format` always wins. Note that `plain` is not fully
unadorned everywhere: `cru models` in plain mode still prints an
`Available models (N):` header and indents each line (and emits a
`Fetching models from daemon...` progress line on stderr).

`table` and `plain` are still accepted on the report commands as aliases for
`text`, because `table` used to be their documented default.

How strict the value is depends on the command. The record-list and report
commands parse `--format` as a closed enum, so an unrecognised value there is
an error. The config and session commands take a free string instead: anything
other than `json` silently falls back to the human-readable default (`toml` for
`config show`/`config dump`, `text` for `session list`/`session show`).
Similarly, `cru search --type` accepts any string and treats anything other
than `semantic` or `text` as `both`.

`-f json` is also not guaranteed to be machine-clean everywhere yet:
`cru status -f json` still prints its progress/summary lines (info and timing)
on stdout around the JSON payload, and `cru storage stats` does the same.
Pipe-safe JSON is currently reliable for the record-list commands, `stats`,
`doctor`, and `session search`.

## cru doctor

Run bounded installation diagnostics:

- **Daemon reachability** — connects to the Unix socket to verify the daemon is running
- **Config validity** — loads and validates your config file
- **Provider connectivity** — pings each configured LLM provider endpoint (2s timeout per provider)
- **Kiln accessibility** — checks the kiln path exists, is a directory, and is writable
- **Embedding backend** — confirms FastEmbed or Ollama embeddings are available
- **Plugins** — asks the daemon how many plugins loaded (skipped if the daemon is down)
- **Kiln references** — every kiln named by a `[projects.*]` entry exists in `[kilns]`
- **Config validation** — the loaded config passed structural validation

```
cru doctor
```

Exits with code 0 if all checks pass, code 1 if any check fails. Warnings (e.g., read-only kiln, no providers configured) don't cause a non-zero exit.

Each failed check prints a suggested fix on the same line:

```
✗ Daemon not running. Try: `cru daemon start`
✗ Config missing at ~/.config/crucible/config.toml. Try: `cru config init`
```

`-f json` prints the raw check results (`check_name`, `status`, `message`) instead of the
table, and always exits 0.

## See Also

- [[Help/CLI/process]] - Processing pipeline details
- [[Help/CLI/chat]] - Chat command reference
- [[Help/CLI/stats]] - Statistics command
- [[Help/CLI/session]] - Session lifecycle and maintenance
- [[Help/CLI/acp]] - Running Crucible as an ACP agent
- [[Help/CLI/lua]] - Evaluating Lua against the daemon
- [[Help/CLI/setup]] - Runtime directory bootstrap
- [[Help/CLI/doctor]] - Installation diagnostics
- [[Help/Config/storage]] - Storage configuration
