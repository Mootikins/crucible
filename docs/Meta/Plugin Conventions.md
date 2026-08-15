---
title: Plugin Conventions
description: Superseded Rust/WASM plugin architecture study — the shipped plugin system is Lua/Fennel
status: superseded
tags:
  - meta
  - plugins
  - design
---

# Plugin Conventions (superseded design study)

> [!warning] None of this was built.
> This page is an early design study for a **Rust plugin system** — a
> `trait Plugin` with `PluginContext`, a `CrucibleEvent` enum, an event bus
> with priorities and dead-letter queues, VSCode-style contribution points,
> and a WASM/Extism capability sandbox. **None of it exists in the codebase.**
>
> The plugin system Crucible ships is **Lua/Fennel**: plugins are directories
> with an `init.lua` returning a spec table (tools, commands, services), hooks
> register via `crucible.on()`, and manifest capabilities are informational
> rather than sandbox-enforced. See [[Help/Extending/Creating Plugins]] and
> [[Help/Extending/Plugin Manifest]] for the real system, and
> [[Help/Extending/Event Hooks]] for the real event set.

## What this page was

A synthesis of plugin-architecture patterns from Neovim, Emacs, Obsidian,
VSCode, Bevy, and Extism, sketched as Rust APIs. The ideas that survived did
so in Lua form:

| Studied here | What shipped instead |
|---|---|
| `trait Plugin` with `on_load`/`on_unload` | Spec-table `on_load`/`on_unload` functions in `init.lua` |
| `CrucibleEvent` enum, pre/post pairs | The closed set of fourteen `crucible.on()` hook names |
| Priority-ordered `EventSubscription` | `crucible.on(..., { priority = N })`, ascending order |
| Lifecycle-aware registration with auto-cleanup | Plugin reload clears the plugin's handlers, tools, and services |
| VSCode-style contribution points | The spec table (`tools`, `commands`, `services`) |
| WASM sandbox with granted capabilities | Not built — one shared Lua VM; manifest `capabilities` are documentation only |
| Advice/interception system | `pre_tool_call` returning cancel / transform / handled |

The unbuilt remainder — the event bus with dead-letter queues, lazy
activation events, per-plugin resource limits, the WASM sandbox — is recorded
in git history if it is ever wanted; it is not planned work.

## References

- Neovim API Documentation: https://neovim.io/doc/user/api.html
- Emacs Hooks Reference: https://www.gnu.org/software/emacs/manual/html_node/elisp/Hooks.html
- Obsidian Plugin API: https://github.com/obsidianmd/obsidian-api
- VSCode Extension API: https://code.visualstudio.com/api
- Bevy ECS: https://docs.rs/bevy_ecs/latest/bevy_ecs/
- Extism: https://extism.org/docs
