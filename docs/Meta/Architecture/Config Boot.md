---
title: Config Boot
description: The one-VM Lua config boot — seed, evaluate, extract — and the ordering contract that follows from it.
tags: [meta, architecture, config, lua]
status: as-built
---

# Config Boot

The daemon's configuration is the result of one Lua evaluation. This note
records the inversion that made it so, the sequence, the contract each part
enforces, and where each claim lives in the code. Paths are relative to
`crates/`.

## The inversion

Before the inversion, the daemon bound with a config parsed from TOML, loaded
plugins, and evaluated `init.lua` last — so the user's file could only
decorate a daemon that was already shaped. Now the order is reversed: the
daemon creates THE plugin VM first, evaluates `init.lua` once inside it, and
binds with the config that evaluation produced
(`crucible-daemon/src/daemon_plugins/boot.rs:1-10`).

One VM, one evaluation. There is no separate config VM and no second pass:
the VM that evaluated `init.lua` is the VM the plugins run in, handed to
`Server::bind_with_plugin_config` inside `BootConfig` (`boot.rs:169`,
`crucible-daemon/src/server/mod.rs:152`).

## The sequence

`evaluate_boot_config` (`boot.rs:203`, injectable-paths variant at `:219`):

1. **Resolve the config root.** An explicitly named `--config` file must
   exist; the default path may be absent (`boot.rs:227-238`).
2. **Seed the store.** The defaults, and nothing else from a file: the
   `config.toml` reader is gone. A leftover file is warned about once per
   boot, with a pointer at `cru config migrate`, and sets nothing. An
   `init.lua` that RAISES is not a hard error; one that does not PARSE is
   (see the failure rule below).
   Then `settings.json`, the machine layer, from the same root
   (`load_settings_layer`, `boot.rs`). An absent file is the normal case. A
   file that does not read, or that does not extract, is warned about and
   skipped — see below.
3. **Create the VM with a live search path.** Module search membership is
   seeded from the config root's `lua/` convention, the default plugin
   locations, and the seed's own `runtimepath` (`seed_boot_search_path`,
   `boot.rs:508`). A `runtimepath` write during the evaluation extends the
   search space *inside* the `cru.config.set` call, before it returns —
   Neovim's invalidate-and-rebuild, installed as the runtimepath extender
   (`crucible-lua/src/config.rs`, `set_runtimepath_extender`).
4. **Evaluate `init.lua` once**, top to bottom, under a 30-second budget
   (`BOOT_EVAL_BUDGET`, `boot.rs:30`).
5. **Extract.** The effective config is what the store holds when the
   evaluation finishes; per-leaf provenance (`file:line` for Lua writes)
   travels with it as `source_map` (`boot.rs:351-364`).

Plugin **activation** is a deferred phase after the file finishes, and it has
one body: `activate` (`crucible-daemon/src/daemon_plugins/activate.rs`). The
spec-driven pass at boot (`load_plugins_from_spec`, `daemon_plugins/mod.rs`),
a `require` from `init.lua`, a runtime install and a reload all end there.
`activate` refuses a plugin whose resolved `enabled` is `false`
(`daemon_plugins/resolve.rs`), runs `init.luau` once in the daemon VM, reads
the returned module table with `spec_from_table`, registers its tools,
commands and services, seeds the module cache so a later `require` answers
the same table, then runs the entry's `config`, else the module's
`setup(opts)`. `on_load` follows. `docs/Meta/CONTEXT.md` defines the words.

A `require` of a plugin entry module during the evaluation cannot await
`activate`: `require` is a sync Rust function and the module hook is a sync
closure with no loader handle. So the boot hook (`install_boot_hook`,
`boot.rs`) runs the module body sync under `LuaSource::Plugin(name)` and
nothing else, and the resolver records which file answered the name.
`activate` later matches that instance by file, skips the body eval and the
source clear, and runs declarations, registration, hooks and `config`. The
host's `config` therefore runs after `init.lua` has finished, so a
`require("x").setup{}` line in `init.lua` runs `setup` twice, the user's call
first. An operator who wants custom setup writes
`config = function(m, opts) ... end` in the spec entry, which replaces the
default call. A boot-required plugin whose entry says `enabled = false`
activates anyway, with a warning that names both sites. The alternative, a
sync `activate` behind a `try_lock`, was refused because it would stop a
`setup` that awaits a `cru.*` API.

The spec itself is Lua: `cru.plugin.setup` in `init.lua` writes the store
(`crucible-lua/src/plugin_spec_store.rs`), ranked by the `LuaSource` in
force. The shipped defaults' entry list in `runtime/defaults/init.luau` lands
at `SpecRank::Builtin`, and so does the installed manifest; the operator's
`init.lua` lands at `SpecRank::Operator` and wins. `enabled` resolves across
the spec store and the config store in `resolve_enabled`: the operator's
entry, then the config leaf `plugins.<name>.enabled` at any layer, then the
fragments, then `true`. The Builtin fragment must not write `enabled`
through the config store, because a `cru.config.set` under
`LuaSource::Builtin` lands at rank `lua` and would outrank the web's
`settings.json`.

## The layer decides the leaf, not the merge order

The boot order inverts the layer order. `settings.json` merges at step 2, and
a plugin's `cru.config.set` runs during step 4 — so the lower layer writes
LAST on every boot. The store therefore ranks each write instead of taking the
last one: `ConfigStore::merge` compares `ConfigSource::rank` against the rank
recorded for that leaf, and drops a write that ranks below it
(`crucible-core/src/config/store.rs`). The order is `default` < `plugin` <
`settings` < `toml` < `lua` < `registered` < `cli` < `rpc`, and
`rank` is its one definition (`crucible-core/src/config/provenance.rs`).

Three properties follow, and each one is a test:

- **One write, one leaf.** An overlay is flattened at the door
  (`flatten_leaves`, `crucible-core/src/config/merge.rs`), so
  `cru.config.set { chat = { model = "x" } }` is one write at `chat.model`. A
  scalar and an array are terminal values; an empty table sets nothing. The
  rank gate is therefore one row at one path, and the pin walk, the rank gate
  and `config.save` all ask about the same paths — they used to hold two
  definitions of a leaf, and the disagreement cost three defects.
- **A dotted key is a path.** `{ ["myplugin.debug"] = true }` and
  `{ myplugin = { debug = true } }` name one leaf, so `:set myplugin.debug=1`
  writes where a config file writes.
- **The decision is per leaf.** A plugin default that loses `chat.model` still
  contributes the siblings no higher layer holds.
- **Equal rank still writes.** Two lines of one file are ordered by the file,
  and the second is the one the author meant.

A dropped write records nothing: no value, no provenance row, no pin.

A flat write can add a key and change a key; it can never remove one. That is
what `config.unset` is for: it takes a key and everything under it out of the
layers `ConfigSource::reset_drops` names — the same layers `config.reset` drops —
so a stale `llm.providers.old` goes without any verb editing a file.

### The file that made the call decides the layer

`AuthorRoots` (`crucible-lua/src/authorship.rs`) reads the chunk name of the
Lua file that called `cru.config.set`. A write from the config root is the
human's own line (`ConfigSource::Lua`) and pins the leaf. A write from a plugin
root is that plugin's default (`ConfigSource::PluginDefault`) and loses to
`settings.json`. The more specific root wins a tie, because a plugin
directory can sit under the config directory.

**One caller holds no file, and its owner decides instead.** A `lua.eval` — `cru
lua`, or `:lua` in the TUI — arrives over a socket, and its chunk name is
`=lua.eval`. That name matches no root, so the path rule alone fell back to
`ConfigSource::Lua` and pinned the leaf: one `cru lua 'cru.config.set{…}'` made
`config.save` refuse that key for the rest of the daemon's life, and the
settings UI named a file that does not exist.

`config_layer` (`crucible-lua/src/authorship.rs`, beside `AuthorRoots` because
the two are one rule with two halves) answers before the path rule, and only
for the source that holds no file. An eval writes `ConfigSource::Rpc`, the same
layer the `config.set` RPC writes, because both are socket calls: the layer
ranks highest, so an eval still overrides anything for this run, it pins
nothing, and a later `config.save` takes the leaf back. Every other source
answers `None` and lets the file decide, which keeps the rule above true
wherever a file exists.

### A Lua layer carries the SOURCE, not a re-spelled name

`ConfigSource::Lua` and `ConfigSource::PluginDefault` each hold one `last_set:
LastSet` — named after Vim's `last_set_sid` and the `:verbose set` output "Last
set from …". `LastSet` is a `LuaSource` plus the file and the line.

`LuaSource` (`crucible-core/src/lua_source.rs`) is the same value the handler
registry, the timer registry and `cru.storage` key on, so the config store and
the registration side cannot disagree about who a plugin is. It lives in
`crucible-core` rather than beside the VM because five subsystems key on it and
only one of them is the VM; the `mlua`-dependent half — the ambient slot and the
brackets that set it — stays in `crucible-lua/src/plugin_context.rs` and
re-exports the type.

It also separates two authors that were indistinguishable. The user's own
`init.lua` and the shipped `runtime/defaults/init.luau` both landed as a `lua`
row naming a file, so a default that ships with the daemon read as a line the
user wrote.

**There is deliberately no load sequence number.** Vim's `sctx_T` carries one
(`sc_seq`) because Vimscript gives each *sourcing* of a file its own `s:` scope,
so two sourcings must not share script-local variables. Our per-source state is
`cru.storage`, whose requirement is the opposite: it MUST survive a reload, or a
plugin loses its state whenever the operator edits a file. So a sequence number
here would not merely be unread — anything reading it for storage would be a bug.

### `ConfigSource::Rpc` names WHICH client

`Rpc` carries `chan: Option<u64>`, the `ClientId` the dispatcher already holds
(`handle_config_set`, `rpc/dispatch.rs`). This is `sctx_T`'s `sc_chan`, and Vim
reports it as "Last set from API client (channel id 3)".

A field, not a variant: the layer is one layer whatever client reaches it, and
"which client" is not a closed set. Without it every client flattened into one
`rpc` row, so `cru config show --sources` and the settings pane could not say
that a value came from somewhere else this run. `None` where no client is in
scope — a `cru.config.set` from an eval, and every in-process merge.

The boot installs both root lists (`install_author_roots`, `boot.rs`), from
the directories that exist at that moment. A plugin the user installs later
creates a directory the boot never saw, so `plugin.install` registers that
directory before it loads the plugin (`learn_plugin_author_root`, `boot.rs`;
the call is in `server/plugin_install.rs`). Without that step the installed
plugin sits under the config root, its `setup()` writes pin as if the user
had written them, and `config.save` refuses those keys for ever.

## Roll back entirely; fail open only on a runtime error

A broken `init.lua` must mean exactly what the warning says. The rollback is
total for BOTH failures — the store, theme, layout, geometry, syntax and
highlight state snapshot back, and the failed VM is dropped with everything
it registered (`boot.rs`, fresh-loader rebuild below the evaluation scope).
"Seed plus whatever registered before the error line" would depend on where
the file failed, so it is not offered.

What the failure decides differs, and `InitFailure` (`boot.rs`) is the split:

- **Syntax.** The file, or a config file it loaded, does not parse. Fatal:
  `evaluate_boot_config` returns an error naming the line, and every caller
  propagates it. There is no intent to fall back FROM, and a daemon that
  starts on defaults after a mistyped bracket reports the user's whole config
  as "no config". `config.toml` used to supply this strictness; deleting it
  moves the strictness rather than dropping it.
- **Runtime.** It parsed, then raised, or overran the 30-second budget.
  Fail-open, on the rolled-back seed.

The classification happens AT THE LOAD, not at the top. `cru.include`
(`crucible-lua/src/config.rs`) and `require` (`modules.rs`) both load a file
inside a Rust callback, and Luau reports a callback's failure to its caller
as a runtime error — so without a mark that survives the callback, the same
mistake would stop the boot in `init.lua` and pass silently one `include`
down. `crucible-lua/src/config_syntax.rs` owns the mark and its reader.

A PLUGIN file is not the user's config: the user did not write it and cannot
fix its line, so `RootKind::Plugin` stays fail-open where `RootKind::User`
does not.

The failure itself is recorded, not just logged: `BootConfig.eval_error`
carries the message the boot failed open on. `cru doctor` reports it as the
isolated-evaluation check (`crucible-cli/src/commands/doctor.rs`,
`evaluate_config_check`) — the one sanctioned second evaluation, because its
output is a report.

## The phase flag: location keys

The store has two phases (`crucible-core/src/config/store.rs`). During the
boot evaluation, `LocationPolicy::Accept`: any line of `init.lua` may set any
key, locations included. When the boot ends (`end_boot_phase`), the seven
`LOCATION_CONFIG_KEYS` (`crucible-core/src/config/config/cli_app.rs:40`) are
dropped from the plugin-visible value and withheld from every later merge,
with a warning naming the key and the Lua call site. The RPC socket has no
authentication; these keys answer *where the daemon acts*, so they freeze at
boot.

The drop takes the keys out of the **value** only. Their provenance rows and
their pins stay, because they answer a different question: *did anybody
configure this*. `config.effective` reads that answer as
`kiln_path_is_default`, and a client that gets `true` substitutes its own
working directory (`crucible-cli/src/config.rs`). Each retained layer
therefore records the policy in force when it was first applied, and a replay
repeats each layer under its own — a boot layer legitimately set `kiln_path`,
so a rebuild must restore its row. The value is stripped once at the end of
the rebuild, which reproduces `end_boot_phase`'s post-condition. A runtime
`config.set` of a location key is still refused whole, at every replay.

## Four verbs, one store

Five RPC methods change the app-config store, and they hold different
authority (`crucible-daemon/src/rpc/dispatch.rs`). Two write a layer; three
drop one.

| Verb | Layer it changes | Persists | Refuses a pin | Caller |
|---|---|---|---|---|
| `config.set` | writes `ConfigSource::Rpc` | No | No | `:set key=value`, one run |
| `config.save` | writes `ConfigSource::Settings`, drops `Rpc` on the leaves it writes | Yes — `settings.json` | Yes | a settings UI |
| `config.reset` | drops `ConfigSource::Rpc` | No | n/a | `:set key&` |
| `config.pop` | drops the highest layer holding the leaf | No | n/a | `:set key^` |
| `config.unset` | drops `ConfigSource::Rpc`, for the key AND everything under it | No | n/a | removing a stale map entry |

`config.set` is the runtime knob. It never refuses, because a user must be
able to raise a value `init.lua` holds for one turn without editing a file,
and the write dies with the process.

`config.save` is the durable preference. Its layer loads BELOW the human's
Lua line, so a value saved over a pinned leaf would be shadowed at the next
boot and the click would act nowhere. It therefore refuses that leaf and
answers with the file and the line that holds it, per leaf: the siblings the
same click changed still save.

**A save also takes the leaf back from the runtime knob.** `Rpc` outranks
`Settings`, so a leaf a `:set` already holds keeps the scratch value. The user
saved from the settings pane, saw nothing change, and got the saved value only
at the next boot — the same shape of failure this store exists to end: a write
the system accepts and does not apply. `ConfigStore::save` therefore drops the
ephemeral hold on every leaf it accepts, by the rule `config.reset` uses. It
drops it on those leaves only, so a `:set` on an unrelated key survives
someone else's save.

`config.unset` is the verb a flat store needs. `config.set` writes one leaf per
terminal value, so it can add `llm.providers.old.endpoint` and change it, but
it can never say the provider is gone. `unset` says that, over a whole subtree,
and it reaches the same layers `config.reset` does — so it edits no file, and a
provider `init.lua` declares goes away only when the line goes away.

**The refusal and the drop are one flatten**
(`crucible-core/src/config/store.rs`). A leaf a pin refuses is neither merged
nor dropped, which is what leaves the value a user raised over a pinned line
standing after a refused save. Two walks would answer "which leaves am I
saving" twice, and the second answer would clear a value nothing replaced.

The pin is a second record in the store, beside the provenance map
(`crucible-core/src/config/store.rs`). Provenance names whoever wrote LAST,
and the ephemeral `:set` writes last all the time; the pin is what runs again
at the NEXT boot. Only a pinning source writes or clears a pin, so one `:set`
cannot open a pinned key to a save. `ConfigSource::pin` decides which sources
pin, exhaustively (`crucible-core/src/config/provenance.rs`).

`config.reset` and `config.pop` are the undo half, and both work in memory
only. The store retains every overlay it merged
(`crucible-core/src/config/store.rs`), so a drop takes the leaf out of one
layer and merges the layers again from nothing. **The re-merge IS the merge
rule**, which is why it is a re-merge and not a per-leaf undo stack: a stack
would be a second copy of `ConfigSource::rank`, of the wholesale-replace rule and
of the pin rule, free to drift from the first copy.

`config.reset` drops exactly the layer `config.set` writes, and
`ConfigSource::reset_drops` decides that exhaustively. It does NOT drop
`Settings`: that layer is a durable file, so dropping it in memory would
report a value the next boot takes back, and deleting the leaf from the file
would let a one-key undo of a session tweak destroy a preference the user
saved through the settings UI. `config.save` writes that layer, and
`config.save` unwrites it. `config.pop` is the door for looking under it for
one run.

Both drop verbs withhold the location keys, exactly as `config.set` does. A
drop changes what the store holds, so a caller that could pop `runtimepath`
would re-point the trees the daemon reads code from without the floor ever
seeing a path.

**The state overlay is the one pin the store does not hold.** `llm.json`
carries the provider selection `cru init` recorded, and
`LlmStateStore::overlay_onto` merges it UNDER the config at bind — so it never
enters the store, and a store merge would invert that precedence. Its leaves
are marked `ConfigSource::Registered` at answer time by one derivation,
`fold_state_overlay` (`crucible-daemon/src/rpc/dispatch.rs`), which
`config.effective`, `config.origin` and `config.save` all call. They must, or
they answer differently about one leaf: `config.effective` used to call a
`cru init` provider `registered` while `config.origin` called it `default`,
and `config.save` accepted a write that put a half-described provider in
`settings.json` — where the next boot loads it as the config layer and it
shadows the working entry. `config.save` refuses a registered leaf and names
`cru init` and the provider surfaces as the route to change it.

## The machine layer: `settings.json`

Two authors write one store. A human writes `init.lua`; a machine — the
settings UI, and every `config.save` behind it — writes `settings.json`,
beside it in the config root. The permission prompt's "always allow" is NOT
one of those machines: that grant goes to the daemon's pattern store, under
the whitelists directory
(`crucible-core/src/config/settings_file.rs`). The data root is the wrong
home for it: `data_home` is itself a config key.

Crucible owns the file. A save rewrites it whole, with sorted keys, through
an atomic write-and-rename, and the `_` key at the top says so; the loader
drops that key. A hand edit survives, because the rewrite merges over what
the file already holds.

Two rules decide what the file may contain:

- **A save writes the accepted DELTA, never the store.** The store holds
  every layer at once. Writing it back would record each `init.lua` leaf as a
  `Settings` leaf, and at the next boot those leaves would load as settings —
  which is to say the refusal that protects a human's line would be bypassed
  permanently, and silently.
- **A key the store withheld is withheld from the file too.** The location
  keys freeze at boot, but this file LOADS in the boot phase, where
  `LocationPolicy::Accept` holds. A persisted `kiln_path` would come back at
  the next start as exactly the authority the save was refused.

That boot-phase authority is deliberate for a HAND edit: a person editing
`settings.json` may set a location key, the same authority `config.toml`
held while it was read. The socket may not.

The load fails open twice, because this layer holds preferences and a daemon
that refuses to start over one leaves the user no door to fix it: an
unreadable file is warned about and skipped, and so is one that does not
extract — a wrong-typed hand edit is reported against `settings.json`
rather than surfacing later against the defaults, which the user did not
write.

`config.origin` answers `{key, value, source, file?, line?, pinned}` for one
key or for every recorded leaf. A settings UI needs it to render a lock and to
offer a jump to the line that locks the key. `pinned` is `ConfigSource::pin`'s own
answer — whether a save of this leaf would be refused — reported per leaf
rather than left to each frontend to derive from the source word: which layers
pin IS the refusal rule, and a copy of it in a renderer goes wrong the next
time a layer is added.

**The row names the pin, not the last writer.** `ConfigStore::origin` reads
the pin map first and falls back to provenance only for a leaf nothing pins,
so one projection serves both callers: the origin the UI locks from, and the
refusal `config.save` answers with. The two maps hold different answers all
the time, because the ephemeral `:set` is the last writer after every routine
adjustment while the human's line still re-applies at the next boot. A row
built from provenance would report that leaf unpinned and file-less, the
control would unlock, and the save it invited would come back refused by a
file the row never named.

## The control tree: what a settings UI may draw

A store and a provenance row are not yet a settings pane. The pane needs to
know, per leaf, what kind of widget it is, what it defaults to, what it means,
and — for a leaf whose value is one of a fixed set — what each choice does.
That description lives in `crucible-lua/src/options/app_config.rs`.

It reuses the plugin control vocabulary rather than starting a second one:
`Control` (`options/control.rs`) already names the eleven kinds, already
refuses a kind it does not know, and already has a frontend that draws them.
One vocabulary, two producers — a plugin declares its tree as a live Lua
table, the app config declares its tree as a Rust constant, and both render to
the same JSON. The module sits beside the plugin producer because that is
where the shared vocabulary is: `Control::value_type` answers with a
`LuaType`, so the enum cannot move into `crucible-core` alone.

No descriptor repeats a default. `app_config_defaults()` serialises
`CliAppConfig::default()`, fills each unset optional subtree with its own
type's default, and every control reads its default out of that value. A
control therefore cannot state a default the type does not have.

**Parity is a leaf property.** `SETTINGS_CONFIG_KEYS` lists 15 top-level
names, and `config.set` accepts all 15 whole, so a gate over that list passes
before any control exists. The gate walks the serialised LEAVES of
`CliAppConfig::default()` instead — the running system, not a name list — and
fails on a leaf that has neither a control nor an entry on a read-only list
that states why it has none. There are three such lists, and each carries its
reason:

- `LOCATION_CONFIG_KEYS`, through one shared `LOCATION_REASON`: a location key
  names where the daemon acts, and the RPC socket has no authentication.
- `READ_ONLY_LEAVES`: the unbounded maps and lists, the tagged union under
  `enrichment`, and the two subtrees whose leaves are themselves authority —
  `web` (the browser surface's own gate) and `workspace` (paths the web API
  then reaches).
- `FREE_FORM_SUBTREE`: `plugins`, excluded by name. Its leaves are whatever
  the installed plugins read, so no fixed list can be complete. Each plugin
  describes its own section through `cru.plugin.options{}`.

`config.controls` serves both halves — `{options, read_only}` — and the answer
is static, because the tree describes a Rust type and not this daemon's state.
It travels over the RPC all the same: that is the seam every frontend already
speaks, and a second copy of the vocabulary in a browser is what one
vocabulary exists to prevent. The read-only half travels WITH its reason, so a
key that draws no control still draws a row that says why.

## Acquisition per command

- A **daemon-backed command** fetches `config.effective`
  (`crucible-cli/src/config.rs:20`, handler at
  `crucible-daemon/src/rpc/dispatch.rs:1705`). Two client-side checks ride
  along: the root-mismatch refusal (an invocation resolving a different
  config root than the daemon's is refused, naming both roots) and the
  staleness warning (a changed `boot_input_hash` over the config root's
  `init.lua`, `init.luau` and `settings.json` warns "restart to apply",
  `boot.rs`). `settings.json` is in that hash because `load_settings_layer`
  reads it at every boot and a hand edit survives a rewrite; without it,
  `cru doctor` called a stale daemon current. `config.toml` is deliberately
  NOT in it: the boot does not read that file, so editing it is not a reason
  to restart.
- A **bootstrap command** (daemon start itself, and commands that may run
  daemonless) runs one throwaway evaluation through the same construction
  (`local_evaluation`, `crucible-cli/src/config.rs`). Same code path, so the
  values agree with a daemon boot by construction.
- The **browser** reads `GET /api/config`, which forwards `config.effective`,
  `config.origin` and `config.controls`, and serves the effective config, its
  `config_root`, one origin row per leaf and the control tree
  (`crucible-web/src/routes/config.rs`). It writes `POST /api/config`, which
  forwards `config.save` and hands the answer back unchanged, refusals
  included: the web layer holds no copy of the store, the layer order or the
  refusal rule. The settings dialog draws the tree with one generic renderer
  (`web/src/components/settings/AppConfigSettings.tsx`), locks a leaf whose row
  says `pinned`, and offers a jump that opens the file at the line.
  **Credentials do not cross this door.** Every answer the web layer reads is
  redacted first (`crucible-web/src/services/daemon_config.rs`, rule in
  `crucible-core/src/config/redact.rs`): a leaf whose name ends in `_key`, or
  in `token`, `secret`, `password`, `passphrase` or `credential`, arrives as
  `[redacted]` — in the config tree, and in the origin row that carries the
  same value under its own `value` field. The rule reads a NAME, not a path,
  so a credential a future struct adds is covered on the day it is added. The
  daemon's RPC does not redact: its socket is per-uid and `0700`, so a caller
  already reads `init.lua` anyway, and the CLI builds an embedding client from
  the resolved key (`crucible-cli/src/factories/embedding.rs`).

Consequence, documented as contract: top-level side effects in `init.lua`
run once per evaluation — the daemon's boot and each bootstrap command's
throwaway evaluation. Actions belong in hooks; values are free.

## Which front end reaches the durable verb

`config.save` and `config.controls` reach a user through the **web console
only**. The TUI reaches the three verbs that write no file — `config.set`,
`config.reset` and `config.pop` — plus the two reads `config.get` and
`config.origin`, which answer `:set key?` and `:set key??`.
`cru config` has `init`, `show`, `migrate` and `dump`, and no write verb. One
CLI command saves: `cru models embeddings use`, which calls `config.save` with
two keys (`crucible-cli/src/commands/models/embeddings.rs`).

`config.unset` is a fourth memory-only verb, and **no front end spells it yet**.
It is reachable over the socket by any RPC client. The TUI has no `:set`
spelling for it, and the web console draws no control for it; giving it one is
its own change, because `:set` grammar and the settings pane both need a shape
for "remove a key" that neither has today.

CLAUDE.md asks a feature that ships to one front end and not the other to say
which, and why. This is that statement.

**The TUI has no durable app-config verb.** `:set` writes the `Rpc` layer,
which dies with the daemon, and there is no `:save`. A TUI user who wants a
preference to survive a restart edits `init.lua` — the human author's file,
and the format this whole design puts first — or opens the web console.

**Why the web console first.** The durable verb is not one keystroke. It needs
the control tree per leaf (widget kind, default, description, the choices of an
enum with their meanings), a lock on a pinned leaf, and a jump that opens the
pinning file at its line. The web frontend already draws all four, through the
generic renderer the plugin option panes share. The TUI has none of them: no
generic control renderer, and no route from a lock to an editor at a line.

**What that costs a TUI-only user, stated rather than hidden.** They cannot
save a preference from the TUI, and they cannot see which leaf a pin locks.
Both are visible in `cru config show --sources`, which renders the per-leaf
source, but that is a read, not a write.

A TUI settings pane is a TUI feature. It needs a story in
[[TUI User Stories]] and T1 plus T2 coverage, and it is deliberately not part
of the config unification.

## What this note is not

The kiln-local `.crucible/init.lua` is not part of this boot. It loads into
per-session Lua runtimes (`crucible-lua/src/config.rs`, `ConfigLoader`), and
whether a kiln-local file gets config-layer rights is an explicitly deferred
trust question. `kiln.toml` and `project.toml` stay TOML identity manifests.

See also [[State Stores]], [[Actual]], and `docs/Help/Configuration.md` for
the user-facing story.
