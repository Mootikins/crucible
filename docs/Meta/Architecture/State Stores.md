---
title: State Stores
description: The daemon's machine-written JSON state files, the locked store shape they share, and how state overlays config.
tags: [meta, architecture, config, state]
status: as-built
as_of: 582c5e6c1
---

# State Stores

Configuration is what the user authors; state is what the machine records.
The two used to share `config.toml`, which meant every registration command
edited a hand-authored file — comment loss under `toml_edit`, machine entries
one careless edit from vanishing, and "restart the daemon" caveats where a
value should have served immediately. The split gave each kind of state its
own JSON file under the daemon data root, with the [[Daemon Server|daemon]] as
the one writer. Paths are relative to `crates/`.

## The shared shape

`RegistryStore<T>` (`crucible-daemon/src/registry_store.rs`) is the pattern,
once: a `<file>.lock` sidecar taken exclusively, read, mutate, atomic
write-beside-then-rename. The sidecar is the lock because the data file is
renamed on every write — locking the data file would let two writers hold
locks on two different inodes. A mutation returning `Err` leaves the file
untouched, which is what makes a refusal (a name already pointed elsewhere)
safe: the check and the write cannot be separated by another writer. `T` is
the whole file; the store replaces, never merges. `kilns.json`, `projects.json`,
`llm.json`, `notifications.json` and `plugins.installed.json` (see "Plugin
declarations" below) all go through this store, inside the
[[Daemon Server|daemon]]. `<data_home>/proposals/` and
`<data_home>/diff-comments/` also go through this store, one file for each
proposal or diffset instead of one file for the whole daemon; see [[Review]]
for their behavior.

`kilns.json`, `projects.json`, `llm.json`, `notifications.json` and
`plugins.installed.json` each carry a `version` field its reader refuses to
exceed, rather than rewriting an unknown schema through the wrong struct.
`plugin-options.json`, each `<data_home>/proposals/<id>.json` and each
`<data_home>/diff-comments/<diffset-id>.json` carry no such field. All three
still go through `RegistryStore` like every other file here, so a lock and
an atomic write guard them too. For `plugin-options.json`, a file that fails
to parse is left untouched, not reset to empty, and the error names the file
(`crucible-daemon/src/daemon_plugins/option_store.rs`). The
[[Luau APIs|plugin setter]] the value came from already ran and took effect
in memory, so a failed write is logged rather than returned to the RPC
caller.

## The files

| File | Owner | Holds | Written by |
|---|---|---|---|
| `<data_home>/kilns.json` | `crucible-daemon/src/kiln_state.rs` | kiln registrations: canonical path, `auto` marker, timestamp | `kiln.register` RPC (`cru init`, `cru kiln register`, `cru acp` auto-attach, chat preflight) |
| `<data_home>/projects.json` | `crucible-daemon/src/project_manager.rs` | project registrations and kiln bindings | project registration RPCs |
| `<data_home>/llm.json` | `crucible-daemon/src/llm_state.rs` | the provider selection and default | `llm.register_provider` RPC |
| `<data_home>/plugin-options.json` | `crucible-daemon/src/daemon_plugins/option_store.rs` | settings-pane values, replayed through the setter of each plugin at boot ([[Luau APIs]]) | `plugin.option_set` |
| `<data_home>/notifications.json` | `crucible-daemon/src/notifications.rs` | the newest `RING` notifications, and the ids each session hid | `cru.log.notify` (any VM) and the daemon's own no-kiln/failed-search/failed-embedding warnings, through `NotificationHub`; served by `notification.list`/`notification.dismiss` and `session.add_notification`/`session.list_notifications`/`session.dismiss_notification` |
| `<data_home>/proposals/<id>.json` | `crucible-daemon/src/proposals/store.rs` | one proposal, kept even after rejection | the `proposal.*` RPCs ([[Review]]) |
| `<data_home>/diff-comments/<diffset-id>.json` | `crucible-daemon/src/diff/comments.rs` | one diffset's comments, its branch source (when it has one) and whether the session journal was migrated into it | the `diff.comment`/`diff.resolve_comment`/`diff.delete_comment` RPCs ([[Review]]) |

`data_home` resolves once at bind and is injected in tests; no store reads
the environment per operation. See [[Core Config]] for the config-side files
this state overlays, and [[CLI Commands]] for the client-side commands named
above.

## State overlays config

Config-declared entries and registered state meet in one overlay: the
`overlay_registrations` function in `crucible-core/src/config/overlay.rs`,
called from `KilnRegistry::overlay_state` in
`crucible-daemon/src/kiln_registry.rs`. The config layer wins by name; each
shadowed state entry is surfaced, not swallowed — the shadow row in
`cru kiln list`, and a log line at bind. See [[Core Config]] for the config
layer this overlays, and [[CLI Commands]] for `cru kiln list` and
`cru kiln forget`. The rules this preserves:

- **Registration is additive at runtime.** A registered name serves
  immediately; no restart, no config edit.
- **Deleting a config line does not unregister** what the state layer also
  holds; the survivor shows as `registered`, and `cru kiln forget <name>` /
  `cru project forget <path>` is the removal. `cru kiln forget` on a
  config-declared entry refuses and names the declaring file and line
  (provenance from the config store); `cru project forget` has no such
  check yet and only ever removes the state entry.
- **Never re-point silently.** Registering a known name at a different path
  is a refusal inside the RPC, under the store lock.

## Plugin declarations — the C11 split

The last mixed file, `~/.config/crucible/plugins.toml`, is split and no
longer read:

- **Declared** — a spec entry with a `Git` source, written by
  `cru.plugin.setup({ "user/greeter" })` in `init.lua`
  (`crucible-lua/src/plugin_spec_store.rs`; the data type is `SpecEntry` in
  `crucible-core/src/config/plugin_spec.rs`). See [[Luau Host]] for the VM
  `cru.plugin.setup` runs on, and [[Core Config]] for `SpecEntry`, `SpecRank`
  and the config store. User authorship; the daemon never writes it. The
  entry is not configuration: `enabled` and `opts` live on the entry, and the
  config store's `plugins.<name>` section is a separate layer that
  `resolve_opts` merges beneath the entry's `opts`.
- **Installed** — `<data_home>/plugins.installed.json`
  (`crucible-daemon/src/plugin_ops.rs`), a versioned `RegistryStore` file
  written by `cru plugin add/remove`, the `plugin.install`/`plugin.remove`
  RPCs and the web plugin routes. See [[CLI Commands]] for `cru plugin
  add/remove` and [[Web Server]] for the plugin routes. `boot_plugins`
  merges each installed entry into the spec store at `SpecRank::Builtin`
  before the spec-driven activation pass: an install is the operator's act
  through a tool, so it sits below their own `init.lua` and above a
  plugin's fragment.

The two meet in one spec. The operator's entry lays over the installed one
for a shared name, and the boot says so by name; `{ "greeter", enabled =
false }` in `init.lua` disables an installed plugin too. The bootstrap clones
the spec's enabled `Git` entries whose directory is missing
(`bootstrap_entries`), deciding with the same `resolve_enabled` activation
uses, so a plugin the web disabled in `settings.json` is not fetched.
Removing a declared plugin is a refusal that names the `cru.plugin.setup`
entry to edit. A leftover `plugins.toml` is imported into the manifest idempotently and
warned about each boot (`sweep_legacy_plugins_toml`); the file is inert.

See also [[Config Boot]] for how the config side is produced. Session state
([[Session Services]]) and note storage ([[Knowledge Storage and Retrieval]])
fall outside this note.
