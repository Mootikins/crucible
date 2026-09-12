---
title: Plugin Seams Alignment
description: One consolidation that replaces five patches, plus the defects and unifications around it
type: plan
status: proposed
updated: 2026-09-11
tags:
  - meta
  - plugins
  - lua
  - plan
---

# Plugin Seams Alignment

Four items went out for scoping. They returned nine defects and three
unifications. Most are alignment: a seam that half exists, or a document that
describes code nobody has run in months.

**Nothing here is a feature.** Every item makes the tree say what it does.

## The shape of it

Two findings recur, and they explain most of the list.

**An absent owner reads as the operator.** A Lua registration records its owner
from the plugin context. No context means `None`, and three separate places
read `None` as "trusted": the interception grant answers `true`, the clear path
skips it, and the config layer ranks it above `settings.json`. That is correct
for the user's own `init.lua`. It is wrong for a socket call.

**A registry announces changes it makes and not changes it unmakes.** The
surface registry announced a row change and not a withdrawal. The same shape
may exist elsewhere; item 9 asks.

## A — the consolidation

**The owner's ruling.** The Lua ownership system is overengineering. Neovim has
no operator or plugin scopes. Nine registries is too many. Consolidate.

That ruling replaces five separate patches in an earlier draft of this plan.
They were one design fact read three ways, and patching them apart would have
deepened it.

### A1. `LuaOwner` — one total enum, no `Option`

"Who registered this" carries THREE meanings today: lifecycle (can it be
cleared), authority (may it intercept), and provenance (does it pin config). An
absent owner reads as the operator in all three, which is right for the user's
own `init.lua` and wrong for a socket call.

```rust
enum LuaOwner {
    /// A plugin load. The name scopes `cru.storage` and a per-session variable.
    Plugin(String),
    /// The user's own `init.lua` and what it includes.
    UserLua,
    /// `runtime/defaults/init.luau`, compiled in.
    Builtin,
    /// One `lua.eval` RPC call.
    Eval,
}
```

**Every variant names what it is, not what it is not.** An earlier draft spelled
the second one `Config`, which reads as "configuration" — a subject, not an
author — and the whole defect being removed is a name that carries more than one
meaning. `UserLua` says who wrote it. `Builtin` says it shipped. `Plugin(name)`
says which one. `Eval` says it came over a socket.

The same rule applies one level down: a per-session variable is namespaced by
its `LuaOwner`, so a value reads as "this plugin's setting for this session".

`set_plugin_context` takes `LuaOwner`, not `Option<LuaOwner>`. **A registration
outside every group becomes impossible at the type level**, so `None` stops
meaning trusted. The host assigns it at the bracket sites; a plugin cannot name
its own, which is the property `plugin_context.rs:1` already defends.

Lifecycle stays on the owner. Authority reads a total function — `Plugin(p)`
asks its declaration, `UserLua` and `Builtin` are true, `Eval` is false for
consistency rather than security, because an eval already runs arbitrary code.
Provenance gives `lua.eval` the `Rpc` layer, which ranks highest, pins nothing,
and drops at a save.

### A2. Six registries merge; three stay apart with cause

The nine are seven stores already — the stage and event handlers share one.

Six merge into `LuaScriptHandlerRegistry`, keyed by four new `HookName`
variants. That deletes the `__crucible_hooks__` and `__crucible_auth_hooks__`
Lua globals and their six parallel tables, a Lua-global counter, and the
`guard.len()` id allocator that `cru_on.rs:62` already documents as wrong.

Three stay apart: `on_load` and `on_unload` are one slot per plugin keyed by the
plugin name; the options trees are reached by a dotted path; and `cru.schedule`
owns a tokio task that must not sit behind the per-tool-call lock.

`clear_owner(runtime, &owner)` — one free function — replaces four partial
clear paths and the two that were missing entirely.

### A3. `Scope` — which sessions a handler fires for

**The owner's requirement.** A workflow plugin must fire for the sessions it was
turned on for, and not for the rest. The owner's example was a Ralph loop that
forces the model to take another turn when it was cut off mid-answer — firing
that on a session nobody turned it on for is a real harm, not untidiness.

Registration has no session. **Firing has one**: `registry.rs:277` sets
`ctx.session_id`. So the filter is a property of the registration, checked at
the fire site. It needs no ambient session and no per-session VM.

`pattern` is taken — it means "which tool name" in both `cru.on` and
`cru.permissions.on_request`. The session scope needs its own word.

```lua
cru.on("pre_tool_call", { session = session.id }, handler)  -- one session
cru.on("tool_result", handler)                               -- every session
```

`enum Scope { Any, Session(String) }`, a third field beside `owner` and
`pattern`.

**Two variants, not three.** An earlier draft added `OptIn(tag)`, where a
session joined a list and a handler registered once read the list at fire time.
The owner ruled against it:

> I expect that the implementation would be something like adding a handler
> when activating the plugin, rather than it being adding a session ID to a
> list of sessions.

So activation REGISTERS. A plugin that a session turns on registers a handler
scoped to that session, at the moment it is turned on. The set of sessions a
handler serves is the set of registrations that exist, and nothing has to be
looked up while a turn is running.

That is smaller in three ways. No tag vocabulary, so nothing can typo a tag and
fail silently. No per-session state read on the hot path. And no second
mechanism for "which sessions" beside the one `Scope::Session` already is.

It costs one thing, and the sweep already pays it: per-session registration
accumulates, which `registry.rs:200` records as a real defect — one stale copy
per session for the daemon's life. The session-end sweep below is therefore not
an optimisation. **It is what makes this design legal**, and it must land with
`Scope`, not after it.

Lifecycle and scope stay separate all the same, as Neovim keeps an augroup
separate from `buffer=`. A handler registered on activation has BOTH: its owner
is the plugin that registered it, and its scope is the session it was activated
for. Clearing the plugin drops it; ending the session drops it; neither implies
the other.

**What a plugin needs to register on activation.** The session id, at a moment
it holds one. `on_session_start` receives a session handle
(`session_lifecycle.rs:258`). A mid-session activation — a command, a key, a
surface button — has `ctx.session_id` at the point the user acts.

**A scoped registration MUST be idempotent, and this is not optional.** `oci`
already records the bug this would otherwise recreate, at
`runtime/plugins/oci/init.luau:531`:

> Handlers register ONCE, at load — not inside `on_session_start`. The registry
> is append-only with no unregister, so registering per session left one stale
> copy per session firing against a dead container for the daemon's lifetime.

And `oci/init.luau:59` gives the sharper reason: **`on_session_start` fires on
create, on resume AND on `resume_from_storage` — and a web history fetch calls
`resume_from_storage` on every request**, while `on_session_end` fires once.

So "activation registers" as first written produces N handlers per session, one
per history fetch, not one. The session-end sweep cleans up afterwards; it does
nothing during.

**Key a scoped registration by `(LuaOwner, HookName, pattern, Scope, name)` and
REPLACE rather than append.** A second registration of the same key is the same
registration.

The `LuaOwner` and the `name` are not optional parts of that key. A triple of
`(HookName, pattern, Scope)` alone would let two plugins collide silently on
`cru.on("pre_tool_call", { session = id }, h)` — which is the fault this same
section warns about for per-session variables, one level down. And without a
plugin-supplied `name`, one plugin could not register two handlers on one event
for one session, which is a legal thing to want. A1 makes `LuaOwner` total, so the
wider key costs nothing new. That makes `on_session_start` safe to register from, which is the
one place a plugin author will naturally reach for.

Without this, A3 asks every plugin author to solve the problem `oci` solved by
avoiding the seam entirely — and the comment above is the evidence that they
will not.

`SessionVariables` (`session_api.rs:116`) stays useful, and is no longer load
bearing. It lives on the slot, is seeded from storage before hooks run and
written back (`session_config.rs:46`), and survives a resume. A plugin that
wants its activation to outlive a restart records it there and re-registers on
the next `session:start`. That is the plugin's decision, not the host's
mechanism.

**Record WHO owns each such variable.** A bare key in a shared per-session map
is the same mistake as an untyped owner one level down: two plugins pick the
same key and neither can tell. Namespace a variable by its `LuaOwner` the way
`cru.storage` already namespaces by plugin, so a value reads as "this plugin's
setting for this session" rather than "a setting".

A `Session` scope on a sessionless dispatch — a file event, a webhook — **never
fires**, and is refused at registration by `HookName::carries_session()`.

Five of the nine need the filter, and all five sit in the store that merges, so
it is written once — in two closures, `registry.rs:178` beside the glob and its
synchronous twin at `permission.rs:191`. No dispatch site checks. A scope on a
name that carries no session is refused at REGISTRATION, gated by a
`HookName::carries_session()` method, rather than failing silently at fire time.

**The measured cost of A3:** one field, one three-variant enum, one method,
three match arms across two sites, two Lua options, two Lua methods, no new
store, one sweep line, about nine files for the widened signature, five tests. Session end sweeps `Scope::Session` entries beside the
release that `session_lifecycle.rs:200` already performs, for the reason above:
without the sweep, activation-registers accumulates.

**Per-session UNLOAD stays unaffordable**, and nobody asked for it. A
`RegistryKey` is valid only against the VM that made it, and the per-session VMs
were deliberately deleted. A scope is a field, not a `RegistryKey`.

### A4. `make_plugin_inert` states the invariant it breaks

Its own doc comment
(`crates/crucible-daemon/src/daemon_plugins/mod.rs:1180`) is exact:

> Remove every registration attributed to `name`. "Not Active" must imply
> "nothing of this plugin's is registered or running."

It does not. It clears five registries and misses the permission hooks, the
schedules, and both plugin-hook maps. So a plugin marked Not Active still holds
live registrations, and the comment says why that matters better than this plan
could.

**`clear_owner` is that invariant made true.** One call, every store, and the
doc comment stops being aspirational. That is the whole argument for the
consolidation in one place.

What falls out with it:

- Permission hooks leak on every reload; no owner, no clear path.
- An eval's `cru.on` handler can never be removed.
- An eval's `cru.on` handler may intercept, because `None` reads as `true`.
- An eval's `cru.config.set` pins a leaf that no file holds.
- `on_load_hooks` has no remove path on uninstall.

And one asymmetry that an earlier draft resolved the wrong way.

`cru.permissions.on_request` can answer `Allow` and skip the prompt with no
declaration, while `pre_tool_call` needs `intercepts_tools` for the equivalent
power. That draft proposed turning `Allow` into `Prompt` for an owner that may
not intercept.

**That reverses the owner's ruling, and it contradicts A5a fifty lines later.**
A plugin is code the operator installed; it gets the API the way an editor
plugin gets the editor. A5a refuses a depth cap for exactly that reason. The two
cannot both hold.

`intercepts_tools` is also not a gate on this surface.
`crates/crucible-lua/src/lifecycle/spec.rs:240` reads it from the table the
plugin itself returns — a plugin grants itself the right in one line of its own
Luau. It is a disclosure. And the recorded reason for keeping it
(`Plugin API Plan.md:396`) is narrow: `handled` fabricates a result the model
reads as the tool's own, before the permission gate. An `Allow` from a
permission hook fabricates nothing, so the reason does not reach it.

**So keep only the half that needs no gate.** `Deny` is honoured from any owner,
because a refusal can only narrow — the rule `cancel` already gets. `Allow`
stays as it is. The asymmetry is real and it is the price of the ruling, not a
defect to patch.

## A5 — a premature stop needs a sound way to re-prompt

**The requirement.** A model stops before the work is done. Something has to be
able to say "not done" and start another turn.

**The host does not decide "done".** A plugin does, from whatever source of
truth it chooses: a plan file it reads, a subsession that evaluates whether the
plan is complete, a tool result, the text of the reply. An earlier draft of this
section designed around one example — a loop that matches prose for "I will
continue" — and treated that heuristic as the feature. It is not. It is one
plugin's opinion, and the host should hold no opinion at all.

**So ship no phrase list, no regex and no `intends_to_continue` flag.** A
shipped pattern becomes a de facto API, and Crucible would then own the accuracy
of a guess about another vendor's prose. Prose matching misfires both ways:
"next, I explain why" announces nothing, and "moving on to the imports" shares
no words with "I continue with the migration". Policy over model output belongs
in Lua, beside the permission modes.

So the work splits in two: **make the re-prompt mechanism sound**, and **give a
plugin enough to decide with**.

### A5a. The mechanism is not sound yet

The inject path works — a handler returns `Inject { content, position }` and the
scheduler re-enters the stream with `is_continuation = true`
(`crates/crucible-daemon/src/agent_manager/messaging/stream.rs:924`). Two things
are wrong with it.

**`position` is deleted, not honoured.** An earlier draft said to honour it.
That was wrong, and the executing agent disproved it.

`script_handler.rs:27` documents `user_prefix` and `user_suffix`, and a test at
`crates/crucible-daemon/src/agent_manager/tests/dispatch.rs:397` asserts the
value round-trips. `stream.rs:885` then discards it. But there is nothing to
prefix or suffix: the inject is read only at `turn:complete`, and the injected
text becomes the WHOLE continuation message. There is no second string to
compose with. `position` was parsed in one commit and never implemented in the
next, and `docs/Help/Extending/Event Hooks.md:569` describes a mechanism that
does not exist — nothing waits for a next user prompt.

The two meanings available both cost more than they buy. Relative to the
model's reply rewrites history and invalidates the prompt-cache prefix.
Relative to the user's prompt re-sends a message already in the conversation
tree. Neither serves a stated use case.

**A field that has never meant anything is not a feature to honour.** Delete it.

**Ordering within a merged inject is a separate, open question.** If A5d's
`merge` lands, "which order do merged contents appear in" becomes real. It
wants a field designed for it — not this one revived. A field designed for one
meaning and repurposed for another is how this one got here.

**And the test is the more useful finding.** It asserted the value survived the
parse. It never asserted the value did anything, and it passed for six months
while both positions behaved identically. That is the same shape as the four
source-text greps this project has already replaced: a gate satisfied without
the property it was meant to require.

**The winner is whichever handler ran last.** Not the highest priority, not the
first: the registry order. See A5d.

#### No depth cap, and no budget check

An earlier draft called both preconditions. **The owner ruled against both.**

> I think a depth cap is inherently bad. Plenty of plugins could make a very,
> very, very large plan, and a depth cap would prematurely stop the execution
> of the plan. Budget check is specifically a plugin concern, not a concern of
> the internal engine.

That is right, and it follows from a ruling already in this tree. A plugin is
code the operator installed; it gets the API the way an editor plugin gets the
editor. A plugin that re-prompts forever is no different from a plugin that
writes `while true do end` — and the host caps neither.

A 500-step plan needs 500 re-prompts. A cap set anywhere is wrong for some
plan, and a cap set high enough for every plan stops nothing.

**What stops it is the same thing that stops any runaway plugin: the user.** The
cancel reaches every recursion depth through `tokio::select!` at
`crates/crucible-daemon/src/agent_manager/messaging/send.rs:420`. That is a real
bound and it already works.

**One correction to how this was argued.** An earlier draft wrote "the host caps
neither", citing Neovim. Neovim DOES cap: `*autocmd-nested*` `*E218*` — "The
nesting is limited to 10 levels to get out of recursive loops" — and re-entry is
off by default. So the ruling stands on its own merits, and not on this
precedent. A 500-step plan needs 500 turns; Neovim's 10-level nesting limit is
answering a different question, about an autocommand that triggers itself.

The budget is the plugin's business for the same reason. A plugin that knows its
plan knows whether the next turn is worth the window; the engine knows neither.
`cru.session` already exposes what a plugin needs to check for itself.

**One thing the host should still add:** `continuation_depth` in the payload. Not
as a cap — as a fact, so a plugin that wants a bound can set its own, and a
plugin that does not can ignore it. The host counts; the plugin decides.

### A5b. What a plugin can decide with

Three inputs, and a plugin may use none, one or all of them.

**The stop reason.** `crucible_core::turn::StopReason`
(`crates/crucible-core/src/turn/mod.rs:157`) already exists with `EndTurn`,
`Cancelled` and `Empty`. **Six sites overwrite it.** `genai` normalises every
provider's answer into `StreamEnd::captured_stop_reason`; Crucible binds that
struct at `crates/crucible-daemon/src/provider/genai_handle.rs:295`, never reads
the field, and hard-codes `EndTurn` at line 319. Line 1274 then recomputes it
from a local flag, so a fix at 319 alone does nothing. The ACP path collapses
`MaxTokens` to `EndTurn` at
`crates/crucible-daemon/src/acp_handle/translate.rs:51`, with a comment naming
the gap.

Add `MaxTokens` and `Refusal`. Refuse a stop-sequence variant — Crucible sets no
stop sequence — and refuse a tool-use variant, because the turn loop emits
`Done` only when no tool calls are pending. About 14 files; an Anthropic-only
version costs more than the full one, because one `From` impl covers every genai
provider.

**Do not wire this to auto-compaction.** `should_autocompact` compares prompt
tokens against the INPUT window; `max_tokens` reports the OUTPUT cap. Connecting
them would compact sessions that need no compaction.

**The reply text.** `dispatch_turn_complete_handlers` (`stream.rs:1013`) already
takes `response: &str`, and line 1024 converts it to `response.len()`. The text
is in hand and one line throws it away. Send a tail with a `response_truncated`
flag; make the size a config value, default 2000, zero meaning no limit.

A plugin can read the reply today through `session.jsonl`, at the cost of a full
file read per turn that grows with the session. A tail costs one slice of a
string already in memory, bounded by the model's output cap.

**This widens a wire type, so it reaches every renderer.** `AGENTS.md` asks what
a change did out from under the front ends, and answers that a front end fed
unfamiliar data is where this breaks while the producing side's tests still
pass. A new key on the `turn:complete` payload and a new field on
`MessageComplete` must be tested where they are DRAWN — the TUI transcript and
the web — not only where they are produced.

**Two fields that are free, because the scheduler already computes them.**
`continuation_depth`, which A5a adds as a fact rather than a cap — a handler
cannot count its own re-prompts today. And `saw_tool_activity`
(`stream.rs:232`), which removes a
class of false positive no text can: a turn that ends right after a tool result
is a model still working, not a model that stopped early.

**Whatever the plugin fetches itself — and this needs NO host change.** A plan
file, a tool result, or a second model asked whether the plan is done.
`cru.session.complete` already exists (`crates/crucible-lua/src/session_api.rs:455`),
and `runtime/plugins/auto-title/init.luau:75` is the worked example: a plugin
already calls a model from inside a handler.

So the subsession case — an agent evaluating whether the plan is actually
finished — is buildable the day A5a lands. It needs a sound re-prompt and
nothing else.

### A5c. What a continuation looks like to a user

A continuation calls `accumulated_response.clear()` (`stream.rs:891`), and the
earlier part stays in the conversation tree as its own `Agent` node. So a
session re-prompted four times paints four bubbles, not one joined reply.

That is a product decision. If nobody makes it, the TUI and the web will each
invent an answer.

### A5d. Two injects: the injects decide, not the host

Today the later handler wins, within a registry and across registries, and the
earlier one is dropped in silence.

**The owner's ruling has two parts.**

> I'd really like to get rid of priority altogether, as I don't think Neovim has
> a priority for autocommands running. […] My main point was that multiple
> triggers on the same response that try to make a new session should basically
> be able to choose whether they all get put together, or are forced separately
> when the new "user response" comes.

#### Priority goes

Neovim has no priority. `nvim_create_autocmd` takes eight option fields —
`buf`, `callback`, `command`, `desc`, `group`, `nested`, `once`, `pattern` — and
none of them orders anything. `autocmd.txt` contains no match for "priorit". The
ruling is correct.

**Two corrections to how an earlier draft justified it.**

First, the rule is definition order, NOT "definition order within a group".
Neovim holds one list per event, and every matching autocommand runs in the
order it was defined, across all groups. A group does not partition the order.
An implementer reading the qualifier would build ordered buckets that Neovim
does not have.

Second — and this is the one that changes the work — **registration order is
not deterministic in Crucible.** In Neovim it is knowable because three
documented rules give a total order the author controls: `'runtimepath'`
position, then alphabetical per directory with `*.vim` before `*.lua`, then
`after/` last. An author who must run last writes into `after/plugin/`.

Crucible specifies none of it.
`crates/crucible-lua/src/lifecycle/discovery.rs:83` iterates
`std::fs::read_dir(search_path)?` and never sorts, and
`crates/crucible-daemon/src/daemon_plugins/mod.rs:929` loads in that same
unsorted order. `read_dir` order is unspecified and varies by file system.

So dropping `priority` for "registration order" today trades a coordination
number for a file-system coin flip. **That is worse, and it is not what Neovim
does.**

**Drop `priority`, and specify load order in the same change.** Sort
`discover()` deterministically, define the rank between source paths, and
document it — the shape `'runtimepath'` and `*load-plugins*` already give. A
plugin that must run first gets a documented position, not a number every author
negotiates with strangers.

**There are FIVE sites, not four, and the one an earlier draft missed is the
only one that must not change.** `runtime/defaults/init.luau:143` registers the
plan-mode deny at `priority = 1000`.
`crates/crucible-lua/src/handlers/permission.rs:56` gives the reason: the
permission gate is FIRST-MATCH-WINS, and the shipped defaults load before every
user file. So registration order gives the exact opposite of what that file
needs — the default deny would win over a user's own hook, permanently. Two
tests defend it, and one names the constant.

**That single site decides the shape of this change.** It is not an ordering
preference; it is a rule that must invert the load order. Any replacement has to
express "run last" for one shipped handler.

**The other four, read by both reviews:** The live rule
today is `matching.sort_by_key(|h| h.priority)`
(`crates/crucible-lua/src/handlers/registry.rs:188`), which is stable, so it
means priority ascending then registration order. The default is 100.

| Site | Value | Effect today |
|---|---|---|
| `runtime/plugins/oci/init.luau:538` | 10 | Runs BEFORE every default handler |
| `runtime/plugins/reflection/init.luau:740` | 100 | The default; no effect |
| `runtime/plugins/retrieval-lab/init.luau:811` | 100 | The default; no effect |
| `runtime/plugins/retrieval-lab/init.luau:814` | 100 | The default; no effect |

Three set the default and lose nothing — pure deletions.

The reviews disagree on `oci`, and the disagreement is worth recording. One
reads `priority = 10` as load bearing, because taking the call over IS the
sandbox. The other argues 10 is actively wrong: running `oci` FIRST means a
guardrail plugin returning `Cancel` is skipped for the six tools `oci`
intercepts, so registration order is closer to correct than 10 is.

**Prior art says both are right about their half, and the number is the wrong
tool for either.** Three agent frameworks solved this independently, and every
one removed relocation from the handler chain. Not one raised a number.

**Pi** (`github.com/earendil-works/pi`) has no priority number anywhere — a grep
for `priority` in its extension system and agent loop returns zero hits.
Handlers run in extension load order, then registration order, and only `block`
short-circuits.

**Its container extension registers no tool-call handler at all.** It replaces
seven built-in tools through `registerTool`, and `prepareToolCall` resolves the
tool BEFORE it runs the hook chain. **So a blocking guardrail always runs, for
every tool**, no matter which extension owns the implementation. That ordering
is a property of the loop, not a race anyone can win.

Pi's pre-tool result type admits `{ block, reason, terminate }` and nothing
else. A handler there CANNOT take a call over, because the type does not let
it.

**Strands Agents** (AWS) keeps a numeric `order` with NAMED constants
(`HookOrder.SDK_FIRST = -100`) — and keeps the container out of the hook chain
entirely, as an `Agent(sandbox=...)` backend that vends its own tools.
**OpenHands** does the same through a `Workspace`.

### Pi's three mechanisms, and the first is the one to steal

**A. The guard event and the takeover event are DIFFERENT EVENTS with different
result types.** Most frameworks give one pre-tool hook a union result covering
deny, allow and replace. Pi splits them: `tool_call` returns
`{ block, reason, terminate }`; `user_bash` returns `{ operations, result }`. A
plugin that wants to own execution registers a tool or answers `user_bash`. **It
cannot reach that power through the guard event at all.**

The capability difference is a TYPE difference, not a permission flag on one
shared event. Compare Crucible: `pre_tool_call` carries `cancel`, `handled` and
a transform on one event, and `handled` is gated by a declaration a plugin
writes about itself. Pi makes the gate structural — there is no declaration to
check, because the wrong power is not reachable from that event.

**That dissolves A4's asymmetry rather than resolving it.** `Deny` and `Allow`
stop being two answers of different weight on one hook, because the hook that
can relocate execution is a different hook.

**B. Built-in tools ship a pluggable operations seam.** Every built-in tool
factory takes an `operations` object — `ReadOperations`, `BashOperations` and so
on. A container plugin replaces only the I/O primitives and keeps the host's
truncation, renderers, result shapes and path handling. Pi's container extension
is 531 lines, most of it a grep implementation rather than policy.

**C. A stated composition rule**, in Pi's own words:
`telemetry(permission(sandbox(coreBash)))`. The permission wrapper sits OUTSIDE
the sandbox wrapper — the gate runs first, the sandbox runs inside it, and the
host owns the order. *Contributions configure rebuilt behaviour; hooks intercept
live operations — separate mechanisms.*

**C is not shipped.** Its own document is marked a design specification, and the
registry it describes does not exist in the tree. Treat A and B as evidence and
C as intent.

### Working directory: the answer you asked for

Pi treats a change of work location as a property of the TOOL, never as a
handler result. `ctx.cwd` is read-only with no setter, and `process.chdir`
appears nowhere. The root directory is a constructor argument to the tool
factory. A relocation is `spawnHook`, which rewrites the command, the cwd and
the environment at spawn time — on the tool, not in the chain.

**So `oci` should not be a `pre_tool_call` handler at all.** A container backend
supplies the six tool implementations at session start, so a tool that runs in a
container is the ONLY implementation of that tool for that session. Nothing
intercepts it, so no guardrail can be skipped, the permission gate runs where it
always runs, and `intercepts_tools` stops being the sandbox's load-bearing
capability.

It also answers the working-directory case the owner raised: a working directory
belongs on the execution backend, not as a side effect one handler leaves for
the next.

**This is a large change and it is out of scope here.** Record it as the
direction. Do not delete `priority` on the assumption it has happened.

### The ordering ladder

Strongest to weakest, with who does each:

1. **Structure** — the container owns the tool, so no order question exists.
   (Strands, OpenHands)
2. **Phase** — a wrapper surrounds every plain handler, whatever it declares.
   (pluggy's `hookwrapper`). This is the one Crucible most visibly lacks:
   `handled = true` can skip a `Cancel` guardrail, and pluggy's equivalent
   cannot, because `firstresult` short-circuits the plain handlers while still
   invoking every wrapper.
3. **Total declared order** — a config file the author writes and reads.
   (OpenHands, `pre-commit`)
4. **A named numeric order over a fully sorted load.** (Strands)
5. **A bare number over an unsorted `read_dir`.** (Crucible today)

**Crucible sits at 5.** Level 1 removes the dispute rather than settling it.

**Whatever else is decided, sort the directory read.** That is small, and it is
correct at every level of the ladder. pytest issue #2364 is what happens
otherwise: the same ordering complaint stays open for years.

**Crucible's number does not lie, which is worth knowing.** BeeAI keeps a
priority its dispatch ignores — it spawns a task per listener, so the number
orders nothing. Crucible's does order: `registry.rs:188` sorts, and
`crates/crucible-daemon/src/agent_manager/messaging/tool_call.rs:78` iterates
that sorted list and awaits each handler in turn. Whatever else changes, the
current behaviour is honest.

**If a number survives, name it.** `priority = 10` tells a reader nothing;
`HookOrder.SDK_FIRST = -100` states the job. That is the enumerated-table
pattern `tools/surface.rs` already sets here. Then prove it — register a
`Cancel` guardrail and a `handled` interceptor together and assert the guardrail
wins, then break the order and watch it fail.

#### The inject declares how it combines

Not a host policy, not a rule about who wins. Each inject says whether it
merges:

```lua
return { inject = { content = "…", merge = true } }   -- ride along
return { inject = { content = "…" } }                  -- its own turn
```

Two injects that both merge become one new user turn carrying both. An inject
that does not merge gets its own turn. Several non-merging injects produce
several turns, in registration order.

Nothing is silently dropped, which is the defect today.

**Precognition is the precedent, and it is the other kind of composing.** It
adds a system message above the user's response
(`runtime/defaults/init.luau:15`), which is a contribution to ONE prompt rather
than a claim to start a turn. That is what `merge = true` looks like from the
inject side: an annotation riding on somebody else's turn.

**What each of the five cases spells.** A guardrail adding "stay inside
`crates/foo`" merges. A session-scoped plugin adding to a global one merges. A
watchdog reporting turn count merges. Two evaluators that disagree each do not
merge, and the disagreement becomes two visible turns rather than one silent
loss — which is the honest outcome, and the user can see it and turn one off.

**Unconfirmed:** I did not establish what a multi-turn injection sequence does
to `is_continuation`, or whether the scheduler can enqueue more than one
follow-up turn today. `stream.rs:885` reads a single `Option`, so this is
probably a real change to that path and not only to the result type. Size it
before committing to the multi-turn half; the merging half is smaller and may
land first.

## B — two live defects the consolidation does not touch

### B1. A withdrawn surface stays on the TUI

**The daemon half landed.** `remove` and `release_plugin`
(`crates/crucible-lua/src/surfaces.rs:336`) now announce, with three tests. The
web is correct already: `SurfacesPanel.tsx:60` refetches the whole list on any
change.

The TUI is not. `crates/crucible-cli/src/tui/oil/chat_runner/actions.rs:667`
reads `if let Ok(Some(msg))` and drops an empty answer in silence — right for a
failed refresh, wrong for a withdrawal.

**Work:** a `ChatAppMsg` variant for "withdrawn", closing the modal only when it
shows that surface. Four call sites. A TUI change needs a story in
`docs/Meta/TUI User Stories.md` plus T1 and T2 coverage.

### B2. `cru.session.current()` is unsafe with two live sessions

It reads a single `Option<Session>` slot (`session_api.rs:495`) and names
whichever the daemon bound last. A handler must decide from `ctx.session_id`.

`Scope` removes the reason to reach for it, so fix this in the same pass:
document it, or refuse it from a handler context.

## C — two gates that do not gate

### C1. `every_rpc_session_knob_is_reachable_from_the_tui` greps its own source

`crates/crucible-cli/tests/architecture_tests.rs:534` runs
`captures(r#""session\.set_([a-z0-9_]+)""#, &dispatch)` over the whole text of
`dispatch.rs`. That file holds the name in the `rpc_methods!` table, in the
setter router, and in `#[cfg(test)] mod tests`. The regex cannot tell them
apart. The web twin at line 426 shares the flaw.

**It has already cost two red CI runs.** Commit `a039dbf8a` deleted the
`context_budget` knob and left the literal in two unit-test sample values, so
the gate still read the knob as advertised. The commit changed the samples to
another knob name — a workaround.

**Two holes nobody had named.** `session.switch_model` carries no `set_`
prefix, so the `model` knob has never been gated on either front end. And check
(3) accepts any error but `UnknownKey`, so a knob demoted to
`SetEffect::TuiLocal` passes.

**Work:** the enumerated source mostly exists. `SessionKnob`
(`crates/crucible-core/src/types/knob.rs:41`) carries `ALL`, `id()`, two clippy
denies and an `EnumIter` completeness test.

**But it enumerates a SMALLER set than the gate guards, and the missing name is
the one that motivated the gate.** `SessionKnob::ALL` holds four variants —
`Model`, `Mode`, `ContextStrategy`, `Precognition`. The gate guards four names
after `SCOPE_MUTATIONS` removes `title` and `workspace`, and one of those four
is `agent_option`, which `SessionKnob` has no variant for.

The gate's own doc comment
(`crates/crucible-cli/tests/architecture_tests.rs:522`) says why that matters:

> Without this half a knob can ship to one renderer and pass review — which is
> what happened to `agent_option`.

So a naive swap narrows the gate and drops the knob the gate was built for.
**Add a `SessionKnob::AgentOption` variant, or state in the plan why
`agent_option` leaves the gate.** Do not leave that choice to whoever picks the
work up: the whole point of an enumerated table is that omission is visible, and
this omission is currently invisible. Add an exhaustive `rpc_set_method()` under the existing denies,
join each name against `crucible_daemon::rpc::METHODS`, and tighten the TUI
check to require `Ok(SetEffect::DaemonRpc(_))`. That is the technique the four
earlier replaced greps converged on. One to two hours, two files.

### C2. `AGENTS.md` misstates five things, and one of them is in the code too

`CLAUDE.md` symlinks to this file, so every agent reads it first. An audit
found five WRONG claims and three STALE ones.

**The one that does the most damage** is line 124: "a plugin needs the
`intercept_tools` capability in its manifest". There is no manifest —
`plugin.yaml` is deleted and `PluginManifest` is never read from a file
(`crates/crucible-lua/src/lifecycle/discovery.rs:113` builds it from directory
defaults). There is no capability — that names the deleted ten-name enum. The
truth is a boolean in the plugin's Lua spec table. An agent that believes this
sentence goes looking for a file format that does not exist.

**The one already proved wrong**, line 123, is worse than a document error.
`StageId::ALL` holds 13 and `EventName::ALL` holds 10; the document says 11 and
8. So does the module doc of the file that defines them, at
`crates/crucible-lua/src/handlers/hook_name.rs:6`: "Eight of the names are
events … Eleven are stages". **Correcting the document alone leaves the source
of the error in place.**

The other three: line 53 names `runtime/defaults/init.lua`, and the file is
`init.luau` (`crates/crucible-lua/src/lib.rs:192` includes the real name); line
125 says `--agent` names an agent card, which holds for `cru session create`
and NOT for `cru chat`, where it is an alias of `--acp`
(`crates/crucible-cli/src/cli/mod.rs:124`) — and the sentence names `cru chat`
first; line 37 calls ACP and MCP "both vendored", which line 49 of the same
document contradicts correctly.

Stale: `ToolSurface` lives in `crucible-core`, not in `tools/surface.rs`;
`protocol/session_events/` is in `crucible-core`, not `crucible-daemon`; and
the REPL is the TUI's `:` command table, not a separate front end.

**Work:** correct all eight, and fix `hook_name.rs:6` in the same commit.

**Then the second half, which matters more.** The audit lists ten claims that
are correct, load-bearing and undefended — a fact nothing would catch if it
drifted. Among them: the stage and event counts, `vendor/` holding one crate,
`ModeRegistry` having no Rust default, `ToolSurface` having no `Default`, and
`runtime/plugins/oci/` being the only legitimate interceptor.

Do NOT add a fifth self-grepping gate. Pick the two or three where drift would
do real damage, and give each a test that derives its expectation from the
running system.

**The counts test is the obvious first, AND it is a trap.** One half comes from
`StageId::ALL.len()`, which the compiler knows. The other half comes from a
regex over prose in `AGENTS.md`. Reword that sentence and the regex matches
nothing, the test compares a count against nothing, and it passes — the exact
defect `AGENTS.md:86` records for the four greps already replaced.

The tree already holds the answer.
`crates/crucible-cli/tests/architecture_tests.rs:434` asserts that its scan
found something before trusting what it found, with the reason written beside
it: "the regex probably broke". **Any test that reads a number out of prose must
first refuse an empty match.** Without that line, C2 builds the thing its own
first sentence forbids.

## D — three unifications

Worth doing. Worth doing last. None of them changes what a user sees.

### D1. Three storage shapes for one need

`ChangeNotifier` (`crates/crucible-lua/src/statusline_exprs.rs:64`) and
`PublicationChangeHook` (`crates/crucible-lua/src/publications.rs:32`) sit in
`Mutex<Option<_>>`. `SurfaceEmitter` (`crates/crucible-lua/src/surfaces.rs:47`)
sits in `OnceLock`.

No production path re-registers any of them: the loader is built once and never
replaced, and a plugin reload keeps the registry instance. So all three want
install-once, and two chose storage that permits a silent double-install and
costs a lock plus an `Arc` clone on every write.

**Work:** a `HostHook<T>` newtype over `OnceLock` in `crucible-lua`. All three
setters return `bool`. Warn at the two call sites that do not.

**Leave the arities alone.** A shared payload type forces owned structs and a
one-field `ExprChange` that exists only for symmetry. That costs more than it
saves.

### D2. Nine Lua registries repeat one pattern three times

An inventory found 31 callback types in five groups. Group L — Lua asks to be
called back — holds nine registries, and they repeat one monotonic name
allocator, one owner-tagging scheme and one clear-on-reload, three times each.

A1 is a symptom: `PermissionHook` is one of the nine, and it is the one that
never got the owner field.

**Work:** name the pattern once. Fix A1 first and let its shape decide.

### D3. Two smaller items, recorded so they are not lost

`SurfaceChange` allocates three `String`s
(`crates/crucible-lua/src/surfaces.rs:237`), and
`crates/crucible-daemon/src/event_map.rs:292` clones every field again.

`publication_changed` is untyped on the wire
(`crates/crucible-daemon/src/server/mod.rs:374`); `surface_changed` is typed.
The web matches the first by string name.

## Not in this plan

**One enum over every hook.** The two families share no firing site, no
storage, no payload, no return type and no timing contract.
`LuaScriptHandlerRegistry` stores a `RegistryKey`, which holds only a Lua
value, so a registry holding both Rust and Lua handlers does not exist
anywhere — the enum is zero-built, not half-built. It would also make firing
worse: 29 sites that know their variant statically would each gain a match.

**Session-local `lua.eval`.** The daemon has no key to be local to. Nothing
sets `CurrentSession` on the plugin VM in production, the RPC carries no
session id, and both callers send only `code`. This is NOT the same question as
`Scope`: a handler always fires inside a session, an eval never does.

**Deleting `lua.eval`.** No existing surface reads an arbitrary Lua value —
`plugin.list`, `surface.list`, `config.get` and `cru plugin check` each read
one named subsystem. `scripts/retrieval-lab.sh` reads a plugin global and has
no replacement. Deletion removes the owner's stated use case.

**A blanket "refuse all registration" eval.** `_G.x = 1`,
`package.loaded[…] = x` and a monkey-patch of `cru` call no Rust closure, so no
host seam sees them. A gate that refused thirteen registries would still leak,
and would advertise a guarantee it cannot keep.

## Order

**A is one change, and it is the plan.** `LuaOwner` first — it is the type change
the rest rests on, and it fixes four defects by removing a case. Then the merge,
then `Scope`. A4's items fall out; do not patch them separately.

**A5 before anyone promises the Ralph loop.** Scope the stop reason. It is
independent of everything else here.

**B1 and B2 whenever.** Both are small and neither waits on A.

**C1 whenever.** Independent, bounded, known technique.

**C2 when someone touches `AGENTS.md`.** Correct `hook_name.rs:6` in the same
commit, or the wrong counts stay in the code that defines them.

**D last.** None of it changes what a user sees.

## Links

- [[Meta/Analysis/Plugin Merge Plan]] — the work this follows
- [[Meta/Analysis/The Plugin Contract]] — the design both serve
