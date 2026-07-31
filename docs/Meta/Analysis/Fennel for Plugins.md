---
title: Fennel for Plugins
description: Whether to actively promote Fennel for Crucible plugin authoring, or keep it an opt-in power tool
type: analysis
status: active
updated: 2026-07-30
tags:
  - meta
  - analysis
  - plugins
  - lua
---

# Fennel for Plugins

> Extracted from [[Meta/Product]] on 2026-07-30. It is a strengths/weaknesses argument with a
> recommendation, not an inventory of shipped capability, so it belongs here rather than in the
> product map. The decision it produced is recorded in [[Meta/Product Decision Log]] (2026-02-03).

Crucible ships both Lua and Fennel (`FennelCompiler` in `crucible-lua`, on by default —
`crates/crucible-lua/Cargo.toml`:10-12, vendored `crates/crucible-lua/vendor/fennel.lua`).
Fennel compiles to Lua with zero runtime overhead. The question is whether to **actively promote**
Fennel for plugins or keep it as an opt-in power tool.

## What is verified as of 2026-07-30

Re-checked against `master` during the product-map reconciliation:

- **Fennel compiles and runs.** `crates/crucible-lua/tests/integration/fennel.rs`::test_fennel_tool_execution
  and `crates/crucible-daemon/src/server/lua_plugin_suite.rs`::a_fennel_test_file_compiles_and_runs.
- **Fennel test suites run under `cru plugin test`.** A `_test.fnl` file compiles and reports pass counts.
- **The LuaLS gap is still real.** `StubGenerator` emits Lua stubs only; there is no Fennel stub
  generator (`crates/crucible-lua/src/stubs.rs`).
- **A Fennel *plugin* cannot execute in the daemon.** `load_plugin_spec` compiles Fennel
  (`crates/crucible-lua/src/lifecycle/spec.rs`:91-104), but the daemon's real execution path does not:
  `DaemonPluginLoader::execute_plugin` does `read_to_string(init_path)` → `lua.load(&source)` with no
  Fennel branch (`crates/crucible-daemon/src/daemon_plugins/mod.rs`:718-725). An `init.fnl` plugin is
  discovered, its spec is displayed, and then execution fails with a Lua parse error that the loader
  downgrades to `warn!` + `mark_error` — it looks installed and does nothing. No shipped plugin is
  Fennel, so `every_shipped_plugin_executes` cannot catch it. This makes the hot-reload mitigation
  below (*"`FennelCompiler` already handles this"*) **false on the path that matters**.

## Strengths for plugin authors

| Feature | Benefit | Example |
|---------|---------|---------|
| **Macros** | DSLs that eliminate boilerplate; a `defservice` or `deftool` macro could reduce a plugin to its essential logic | `(defservice :discord {:token (secret)} (fn [ctx] ...))` |
| **Pattern matching** | Cleaner event dispatch than if/elseif chains; natural fit for `MESSAGE_CREATE` / `INTERACTION_CREATE` routing | `(match event.t :MESSAGE_CREATE (handle-msg event.d) :READY (on-ready event.d))` |
| **Destructuring** | Concise argument extraction; Lua plugins repeat `local x = args.x` lines | `(fn [{: query : limit}] ...)` |
| **Immutable locals** | Fewer mutation bugs in stateful plugins (services, session managers) | `(local config (validate schema opts))` — can't accidentally reassign |
| **Data literal syntax** | Tables-as-data read naturally; good for config, schemas, API payloads | `{:name "discord" :capabilities [:network :websocket]}` |
| **Lisp composition** | Threading macros (`->`, `->>`) make transform pipelines readable | `(->> text (strip-mentions) (transform-tables) (chunk 2000))` |

## Weaknesses for plugin authors

| Issue | Impact | Mitigation |
|-------|--------|------------|
| **LuaLS doesn't understand Fennel** | Type stubs, autocomplete, diagnostics — all DX investments are Lua-only; Fennel devs get no IDE support | Fennel LSP (`fennel-ls`) exists but immature; alternatively, generate Fennel type stubs alongside Lua ones. *Still true 2026-07-30.* |
| **Smaller community** | Fewer examples, less Stack Overflow help, harder to onboard contributors | Good docs + example plugins can compensate; Fennel community is small but high-quality |
| **Compilation indirection** | Error line numbers reference compiled Lua, not source Fennel; debugging is harder | Fennel has source maps; `FennelCompiler` could propagate them |
| **Parenthetical syntax** | Polarizing; barrier for developers without Lisp experience | Keep Lua as default; Fennel is opt-in for those who prefer it |
| **Hot reload complexity** | Fennel files need recompilation before reload; adds a step vs. pure Lua | ~~`FennelCompiler` already handles this; `:reload` should compile-then-load transparently~~ — **false as of 2026-07-30**: the daemon's plugin execution path has no Fennel branch at all, so an `init.fnl` plugin never executes, reload or otherwise |
| **Macro debugging** | Macros can produce opaque errors; `macrodebug` helps but adds friction | Document macro patterns; keep macros simple |

## Recommendation

Keep Fennel as an **opt-in power tool**, not the default path. Lua examples first in all docs, Fennel
alternatives shown alongside. Invest in Fennel-specific DX only after Lua DX is solid (type stubs, hot
reload, REPL all working). The macro system is genuinely valuable for reducing plugin boilerplate — a
`defservice` macro alone could justify Fennel for service plugin authors. But the LuaLS gap means
Fennel developers trade IDE ergonomics for language ergonomics; that's an informed choice, not a
default.

**Prerequisite before any of that is worth doing:** make Fennel plugins actually execute in the daemon.
The falsifiable item is tracked in [[Meta/Product]] under Extensibility & Plugins. Red test:
`a_fennel_plugin_executes_in_the_daemon_runtime` — write an `init.fnl` returning a spec, load it through
a real `DaemonPluginLoader`, assert `state == "Active"` with no `last_error`.

## Links

- [[Meta/Product]] — capability inventory
- [[Meta/Product Decision Log]] — the 2026-02-03 decision this analysis produced
- [[Help/Concepts/Scripting Languages]] — user-facing Lua/Fennel reference
