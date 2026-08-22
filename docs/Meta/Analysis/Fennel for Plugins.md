---
title: Fennel for Plugins
description: Whether to actively promote Fennel for Crucible plugin authoring, or keep it an opt-in power tool
type: analysis
status: active
updated: 2026-08-22
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

Crucible ships both Lua and Fennel (`FennelCompiler` at `crates/crucible-lua/src/fennel.rs:53`, on by default at
`crates/crucible-lua/Cargo.toml:10`, vendored `crates/crucible-lua/vendor/fennel.lua`).
Fennel compiles to Lua with no runtime overhead. The question is whether to **actively promote**
Fennel for plugins or keep it as an opt-in power tool.

## What is verified as of 2026-08-22

Checked against `master` at commit 7053bcfe7:

- **Fennel compiles and runs.** `crates/crucible-lua/tests/integration/fennel.rs:99`::test_fennel_tool_execution
  and `crates/crucible-daemon/src/server/lua_plugin_suite.rs:631`::a_fennel_test_file_compiles_and_runs.
- **Fennel test suites run under `cru plugin test`.** A `_test.fnl` file compiles and reports pass counts.
- **The LuaLS gap is still real.** `StubGenerator` (`crates/crucible-lua/src/stubs.rs:34`) emits Lua
  stubs only. There is no Fennel stub generator.
- **A Fennel plugin executes in the daemon.** This was false on 2026-07-30. Then
  `load_plugin_spec` compiled Fennel (`crates/crucible-lua/src/lifecycle/spec.rs:85`), but
  `DaemonPluginLoader::execute_plugin` loaded the raw `.fnl` source as Lua. An `init.fnl` plugin
  looked installed and did nothing. Commit 3728b85b7 (2026-08-13) fixed it: `execute_plugin` now
  compiles a `.fnl` main with `compile_fennel_source` before `lua.load`
  (`crates/crucible-daemon/src/daemon_plugins/mod.rs:1006`).
- **One shipped plugin is Fennel.** `runtime/plugins/graph-view/init.fnl`, from the same commit.
  `every_shipped_plugin_executes` (`crates/crucible-daemon/src/daemon_plugins/tests/shipped.rs:37`)
  now covers the Fennel path. The direct proof is
  `a_fennel_plugin_executes_in_the_daemon_vm` (`shipped.rs:169`). It loads an `init.fnl` through a
  real `DaemonPluginLoader` and asserts `state == "Active"` with an empty `last_error`.

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
| **LuaLS doesn't understand Fennel** | Type stubs, autocomplete, diagnostics — all DX investments are Lua-only; Fennel devs get no IDE support | Fennel LSP (`fennel-ls`) exists but immature; alternatively, generate Fennel type stubs alongside Lua ones. *Still true 2026-08-22.* |
| **Smaller community** | Fewer examples, less Stack Overflow help, harder to onboard contributors | Good docs + example plugins can compensate; Fennel community is small but high-quality |
| **Compilation indirection** | Error line numbers reference compiled Lua, not source Fennel; debugging is harder | Fennel has source maps; `FennelCompiler` could propagate them |
| **Parenthetical syntax** | Polarizing; barrier for developers without Lisp experience | Keep Lua as default; Fennel is opt-in for those who prefer it |
| **Hot reload complexity** | Fennel files need a compile step before reload; this adds a step compared to pure Lua | `execute_plugin` compiles a `.fnl` main on every load, reload included (`daemon_plugins/mod.rs:1006`). This was false from 2026-07-30 to 2026-08-13; commit 3728b85b7 fixed it. |
| **Macro debugging** | Macros can produce opaque errors; `macrodebug` helps but adds friction | Document macro patterns; keep macros simple |

## Recommendation

Keep Fennel as an **opt-in power tool**, not the default path. Lua examples first in all docs, Fennel
alternatives shown alongside. Invest in Fennel-specific DX only after Lua DX is solid (type stubs, hot
reload, REPL all working). The macro system is genuinely valuable for reducing plugin boilerplate — a
`defservice` macro alone could justify Fennel for service plugin authors. But the LuaLS gap means
Fennel developers trade IDE ergonomics for language ergonomics; that's an informed choice, not a
default.

**The prerequisite is done.** Fennel plugins execute in the daemon since commit 3728b85b7. The proof
test is `a_fennel_plugin_executes_in_the_daemon_vm`
(`crates/crucible-daemon/src/daemon_plugins/tests/shipped.rs:169`). The matching row in [[Meta/Product]]
under Extensibility & Plugins still says "nothing"; update it.

## Links

- [[Meta/Product]] — capability inventory
- [[Meta/Product Decision Log]] — the 2026-02-03 decision this analysis produced
- [[Help/Concepts/Scripting Languages]] — user-facing Lua/Fennel reference
