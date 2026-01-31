# Hermit — Knowledge Assistant Plugin

A hermit crab tending your knowledge collection in solitude.

Hermit watches your kilns, connects your notes, surfaces forgotten knowledge, and prepares context digests — all event-driven, not polling. It doesn't create knowledge; it organizes, connects, and guards what's already there.

## Quick Start

1. Copy the plugin to your Crucible plugins directory:
   ```sh
   cp -r plugins/hermit ~/.config/crucible/plugins/hermit
   ```

2. Copy the agent card for chat mode:
   ```sh
   cp plugins/hermit/agent.md ~/.config/crucible/agents/hermit.md
   ```

3. Start a session:
   ```sh
   cru chat
   ```

Hermit bootstraps automatically on session start.

## What It Does

- **Awareness** — Scans your kiln on session start, caches note count, tags, orphans, and recent activity with configurable TTL
- **Link suggestions** — When you create a note, Hermit traverses the graph and suggests unlinked neighbors
- **Broken link detection** — When you edit a note, Hermit checks that all outlinks still resolve
- **Orphan tracking** — Finds notes with zero connections
- **Digest** — Generates a structured summary: note count, top tags, recent activity, orphan list
- **Agent mode** — Chat with Hermit directly (`cru chat --agent hermit`)

## Commands

| Command | Description |
|---------|-------------|
| `/hermit status` | Open splash view with kiln stats |
| `/hermit digest` | Open digest view |
| `/hermit orphans` | List disconnected notes |
| `/hermit soul` | View personality definition |
| `/digest` | Shortcut to digest view |
| `/soul` | Shortcut to soul view |

## Tools (for agents)

| Tool | Description |
|------|-------------|
| `hermit_digest` | Generate activity summary (params: `days`) |
| `hermit_links` | Suggest wikilinks for a note (params: `path`, `depth`) |
| `hermit_orphans` | List notes with no connections |
| `hermit_profile` | Show cached kiln profile |

## Configuration

In your Crucible config, under `plugins.hermit`:

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `soul_file` | string | `""` | Path to custom soul definition (empty = bundled soul.md) |
| `auto_link` | boolean | `true` | Suggest links when notes are created |
| `awareness_cache_ttl` | number | `300` | Seconds before cache expires |
| `quiet_mode` | boolean | `false` | Suppress non-essential notifications |
| `enabled_reactions` | string | `"all"` | Comma-separated: `all`, `none`, or specific names |

## What This Demonstrates

This plugin exercises the major Crucible plugin APIs:

1. **Plugin manifest** — capabilities, config schema, auto-discovery
2. **`@tool` annotation** — 4 tools with typed parameters
3. **`@handler` annotation** — 3 event handlers with priority levels
4. **`@command` annotation** — 3 commands with subcommands
5. **`@view` annotation** — 2 interactive views with keyboard handlers
6. **Oil UI DSL** — `col`, `row`, `text`, `kv`, `hr`, borders, styling
7. **Vault API** — `list`, `get`, `outlinks`, `backlinks`, `neighbors`
8. **Session hooks** — `crucible.on_session_start()`
9. **Notification system** — `crucible.notify()` with log levels
10. **Agent cards** — Persona with inline system prompt
11. **Configuration** — Typed config with defaults and helpers

## Architecture

```
plugins/hermit/
├── plugin.yaml          # Manifest (auto_discover: true)
├── init.lua             # Entry point: tools, commands, session hook
├── soul.md              # Personality definition (user-overridable)
├── agent.md             # Agent card (copy to ~/.config/crucible/agents/)
├── README.md            # This file
└── lua/
    ├── config.lua       # Config defaults and helpers
    ├── awareness.lua    # Kiln scanning, profile caching, TTL refresh
    ├── reactions.lua    # Event handlers (note:created, note:modified, session:started)
    ├── background.lua   # Link suggestion, orphan detection, digest generation
    ├── digest.lua       # Digest formatting and rendering
    └── ui.lua           # Oil UI views (splash, digest)
```

## Known Limitations (v0.1)

| Limitation | Workaround |
|-----------|------------|
| `cru.vault.search()` is a stub | Uses graph traversal instead of semantic search |
| No Lua subagent API | Tools run synchronously when called by agent |
| No plugin statusline components | Uses `crucible.notify()` for status |
| Agent cards not auto-registered | Manual copy to `~/.config/crucible/agents/` |
