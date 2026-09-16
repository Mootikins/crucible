---
title: Extendability and Long-Term Health — 2026-09-16
description: Why one fact lives in several places across the daemon, the Lua runtime and the three frontends, which gates could have refused today's defects, and the rules and structural changes that make a new feature touch one owner, one projection and one test lane
tags: [meta, architecture, health]
status: draft
updated: 2026-09-16
---

# Extendability and Long-Term Health — 2026-09-16

The owner asked two questions. Why is everything so patched together? How does a session focus on extension and long-term ease of addition before it builds? This note answers for the whole system: the daemon, the core types, the Lua runtime, the three frontends, the tests and the process. The web components' composition is a separate note. Related: [[2026-09-15 Why a Simple Change Costs a Day]], [[Architecture/Index]].

## The pattern in one sentence

A fact is stated in more than one place, no gate compares the copies, and a fix lands at the copy nearest the symptom. Every defect on 2026-09-15 has this shape. The seams below are the places where it happens again next month.

## 1. Seams: one fact, several copies

### The session record

| Copy | Where | Field names |
|---|---|---|
| The daemon's list row | `crates/crucible-daemon/src/server/session/list.rs` | `agent_model`, `agent_mode` |
| The daemon's Lua record | `crates/crucible-daemon/src/session_bridge.rs` (`session_json`) | `model`, `session_type`, `event_count` |
| The Lua property gate | `crates/crucible-lua/src/session_api.rs` (`unknown property`) | `id`, `isolation`, `mode`, `model`, `system_prompt`, `workspace` |
| The web type | `crates/crucible-web/web/src/lib/types.ts` (`Session`) | `agent_model`, `agent_mode`, `last_activity` |
| The core summary | `crates/crucible-core/src/session/` (`SessionSummary`) | `event_count`, no model |

The daemon itself spells one field two ways: `agent_model` on the wire to clients and `model` to Lua. The shipped `session-board` plugin (`runtime/plugins/session-board/init.luau`) read `s.agent_model`, a name the Lua record never had, and the release daemon aborted at boot. The plugin typecheck passed, because the record reaches Lua as an untyped table.

One owner: a `SessionRecord` struct in `crucible-core` with serde names, derived once. Projections: the JSON for clients, the Lua table, and the TypeScript interface, all generated from it. A plugin then typechecks against a declared record type, and a wrong name fails activation, as the CLAUDE.md rule for tool types already demands.

### The kiln identity

| Copy | Where |
|---|---|
| Config entry | `crates/crucible-core/src/config/config/registry.rs` |
| Registration | `crates/crucible-core/src/config/overlay.rs` (`Registration`) |
| Registry row | `crates/crucible-daemon/src/kiln_registry.rs` (`RegisteredKiln`) |
| State store | `crates/crucible-daemon/src/kiln_state.rs` |
| Open state | `crates/crucible-daemon/src/kiln_manager.rs` |
| Web row | `crates/crucible-web/web/src/lib/types.ts` (`KilnListEntry`) |

Until commit `d1833e09a` the listing derived a name from a directory's basename while the attach handler asked the registry, so the picker offered a name the daemon refused. Until commit `e1a00c708` the listing reported open state and every kiln-addressed route gated on it, so a registered kiln the boot had not opened answered 404. Both were two answers to "which directories are kilns". The registry now answers, and the manager reports open state as a field. The remaining copies are the config entry, the registration and the web row, three shapes of one record that a generated projection would collapse to one.

### The mode list

The built-in modes exist four times: `crates/crucible-core/src/types/mode.rs` (`default_internal_modes`), `runtime/defaults/init.luau` (`cru.modes`), `crates/crucible-web/web/src/components/ChatModeControl.tsx` (`FALLBACK_MODES`), and `crates/crucible-cli/src/tui/oil/chat_app/state.rs` (`DEFAULT_MODE`). The daemon is the owner, and it already serves `session.list_modes`. The two client fallbacks exist because a client draws before the daemon answers. A generated constant from the core list would keep them equal; a client that draws nothing until the daemon answers would remove them.

### The error envelope

The daemon returns a structured `RpcError` (`crates/crucible-core/src/protocol/rpc/mod.rs`). The client turns it into text, `RPC error: {json}`, in `crates/crucible-daemon/src/rpc_client/client/mod.rs`. The web server re-parses that text in `crates/crucible-web/src/error.rs` (`rpc_error_parts`), and the browser parses the body again in `crates/crucible-web/web/src/lib/api.ts`. Until commit `ca4190a19` the browser showed the JSON. Three parsers exist because the client discarded the structure one hop after it was made. One owner: the client returns the `RpcError` as a typed error, and every hop matches on it.

### The protocol types

`schemars` is a dependency and four files derive `JsonSchema`, but nothing generates a TypeScript contract from it. The web's `lib/types.ts` is written by hand, and the web's slash-command list already recorded the cost in `lib/api.ts`: "the previously hand-maintained frontend copy had already lost `/models`". Every new field on a daemon type is typed twice, and the second copy drifts. One projection: a `cru schema` step that writes a generated `protocol.ts` under the web `lib/generated` directory from the core types, checked in and diffed in CI.

### The model list

This one has one owner. The daemon's `agent_manager/models.rs` answers `models.list` for the TUI and `session.list_models` for the web. Its defect on 2026-09-15 was a rule, not a copy: discovery hid the configured model. The TUI and the web fixed together because the fact lived once.

## 2. Gates that were missing

| Defect | The gate that would have refused it |
|---|---|
| The plugin read `s.agent_model` | A declared record type in the Lua definitions (`crates/crucible-daemon/src/server/lua_plugin_suite.rs` checks every shipped plugin against `cru.d.luau`; the session record is untyped there) |
| `registered` and `open` added to a kiln row by hand in three places | A generated TypeScript contract diffed in CI |
| The kiln list offered a name the attach refused | A test that feeds every row of a lister to its taker: `kiln.list` to `session.connect_kiln`, `models.list` to `session.switch_model`, `project.list` to `fs.list`, the runtime targets to `session.create` |
| A stored session refused by `session.get` | The live lane, which had no session and no restart |
| The daemon read `init.lua` while the owner's providers sat in `config.toml` | A boot warning that names a deprecated file it will not read, and a test that boots over a config directory holding only `config.toml` |
| A Lua error in a host callback aborted a release daemon | A test that activates a plugin whose `setup` throws and asserts the daemon still answers; the release profile's `panic = "abort"` makes the debug suite silent on it |

The route contract lane (`crates/crucible-web/tests/route_contract_tests`, 15 files) runs the real router against a mock daemon, so it proves the router's shape and never the daemon's. The live lane (`crates/crucible-web/web/e2e/live`) is the only place a rule stated in two layers can fail, and until 2026-09-16 it created no session.

## 3. The process that produces this

Three habits push a fix to the nearest layer.

1. **The brief names a symptom.** "The model menu doesn't populate" sends an agent to the picker. The rule that failed lived in the daemon's discovery, and an agent that starts at the picker patches the picker. The client revive-through-history workaround in `crates/crucible-web/web/src/contexts/SessionContext.tsx` was such a patch, and it spread to one call site of two.
2. **The review asks "is it green", not "who else states this".** A `<Show>` branch, a second class string, or a client fallback turns a red test green in one file. Nothing in `CLAUDE.md` asks the reviewer to find the other copies of the fact before the fix lands. The 300 most recent commits split 84 `feat` to 75 `fix`, and 29 of them touch both the daemon and the web: the same fact edited in two crates in one commit, by hand.
3. **Comments and tests defend the copy.** A comment that argues for a choice, and a test that names the markup, both raise the cost of moving the fact to its owner. The retrospective measured a fifth of the web source as comment and more than half of one commit as test rewrites.

The replacement rules, concrete:

- **One owner per fact.** Every diff that adds a field, a name, a list or an error shape names the owner file and the projections. A second hand-written copy fails review.
- **"Who else states this rule?"** Every brief and every fix answers it before the first edit, with the file list.
- **A cross-layer rule gets a live-lane scenario, red first.** Not a mocked one.
- **A lister's rows feed its taker in a test.** Every `list` method has a test that sends each row to the method that consumes it.
- **A shared part exists before a second hand-drawn copy.** The composition note lists the parts.
- **Depth and line gates on components.** Under 250 lines and eight JSX levels; past either, split.

## 4. Five structural changes for the next twelve months

In order. Each makes the next one cheaper.

**1. A generated contract for the wire types.** Add a `cru schema` command (or a build step in `crates/crucible-web/build.rs`) that emits `crates/crucible-web/web/src/lib/generated/protocol.ts` from the core and daemon types that already derive `JsonSchema`, and a CI check that the file is current. Replace the hand-written interfaces in `lib/types.ts` one at a time. Cost: one command, one CI step, one migration pass over about forty interfaces. Removes: every future field typed twice, and the class of drift that added `registered` and `open` by hand.

**2. One session record.** Define `SessionRecord` in `crucible-core` with the serde names the clients see, and derive the Lua table from it in `session_bridge.rs` instead of building JSON by hand. Emit the record type into `cru.d.luau` so the shipped-plugin typecheck refuses `s.agent_model`. Cost: one struct, one projection, a rename of `model` to `agent_model` for Lua with a deprecation shim for one release. Removes: the five field lists above and the abort of 2026-09-16.

**3. Typed errors end to end.** `rpc_client` returns `RpcError` as a typed error variant instead of `anyhow::bail!("RPC error: {}")`. The web maps the variant to a status and a sentence in one match; the TUI shows the sentence. Delete `rpc_error_parts`. Cost: one enum variant and the call sites that match on the text. Removes: two parsers and every bare "422".

**4. A live lane that owns every cross-layer rule.** Keep the tier hermetic as it is now. Add the lister-to-taker specs, the restart scenario, the config-directory scenario, and one plugin-failure scenario. Move each mocked spec whose intent is a journey. Cost: about ten specs and two minutes of CI. Removes: the mocked lane's false confidence for rules that cross a layer.

**5. Data-shaped surfaces with generated defaults.** The settings sections, the panel registry, the tool card kinds and the mode list become records the daemon or the core publishes, drawn by one renderer each, with the client fallbacks generated from the core constants. Cost: four refactors, each one session. Removes: four hand-placed lists and two client copies of the mode list.

After these, a new session knob touches the core type, one generated projection lands in the web and the TUI, and one live spec proves it. File upload, scoped in `docs/Meta/Plans/2026-09-16-file-upload.md`, follows the same route: one `fs.write_bytes` in the daemon, one generated request type, one chip record, one live spec.

## 5. What the owner sets, so this is the default

Lines for the project `CLAUDE.md`, under Design:

```
- One owner per fact. A field, a name, a list or an error shape is defined
  once and projected. A second hand-written copy fails review.
- Before the first edit of a fix, list every file that states the same rule.
  Fix at the owner; the other files read it.
- A rule that crosses a layer gets a red-first live-lane scenario. A mocked
  test does not prove it.
- Every `list` method has a test that feeds each row to its taker.
- A shared part exists before a second hand-drawn copy of any shape.
- A component stays under 250 lines and eight JSX levels.
```

The brief template, for every agent and every session:

```
Target shape: <the data record, the sketch, or the screenshot>
Owner of the rule: <file>. Readers of the rule: <files>.
Non-goals: <what must not change>
Red first: <test file and lane>; report the failure text before the fix.
Evidence to return: diff stat, test output, screenshot.
```

The checks before a commit, in this order: the scoped tests red then green; `just lint`; `grep` for the fact's other copies; the live lane when the rule crosses a layer; the composition gate on any changed component.
