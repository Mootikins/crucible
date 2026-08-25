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
2. **Seed the store.** Defaults first, then `config.toml` where it still
   exists — the deprecated seed, warned about once per boot with a pointer at
   `cru config migrate` (`boot.rs:241-254`). A malformed seed is a hard
   error, exactly as it was under the old loader; a broken `init.lua` is not
   (see fail-open below).
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

Plugin **activation** is a deferred phase after the file finishes. A user who
calls `require("<plugin>").setup{...}` at the top of `init.lua` owns that
plugin's setup: the activation phase reuses the same module instance and
skips its default `setup(cfg)` call (`BootRequireState::user_owns_setup`,
`boot.rs:102`; the skip at `daemon_plugins/mod.rs:1052`). A plugin configured
both ways — a direct `setup` call AND a `plugins.<name>` store section — takes
the direct call, and the loader records one notice per such plugin
(`daemon_plugins/mod.rs:165`).

## Fail-open, entirely

A broken `init.lua` must mean exactly what the warning says: the daemon
continues on the seed. The rollback is total — the store, theme, layout,
geometry, syntax and highlight state snapshot back, and the failed VM is
dropped with everything it registered (`boot.rs:295-311`, fresh-loader
rebuild at `:340`). "Seed plus whatever registered before the error line"
would depend on where the file failed, so it is not offered.

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

## Acquisition per command

- A **daemon-backed command** fetches `config.effective`
  (`crucible-cli/src/config.rs:20`, handler at
  `crucible-daemon/src/rpc/dispatch.rs:1705`). Two client-side checks ride
  along: the root-mismatch refusal (an invocation resolving a different
  config root than the daemon's is refused, naming both roots) and the
  staleness warning (a changed `boot_input_hash` over `config.toml` +
  `init.lua` warns "restart to apply", `boot.rs:147`).
- A **bootstrap command** (daemon start itself, and commands that may run
  daemonless) runs one throwaway evaluation through the same construction
  (`local_evaluation`, `crucible-cli/src/config.rs`). Same code path, so the
  values agree with a daemon boot by construction.

Consequence, documented as contract: top-level side effects in `init.lua`
run once per evaluation — the daemon's boot and each bootstrap command's
throwaway evaluation. Actions belong in hooks; values are free.

## What this note is not

The kiln-local `.crucible/init.lua` is not part of this boot. It loads into
per-session Lua runtimes (`crucible-lua/src/config.rs`, `ConfigLoader`), and
whether a kiln-local file gets config-layer rights is an explicitly deferred
trust question. `kiln.toml` and `project.toml` stay TOML identity manifests.

See also [[State Stores]], [[Actual]], and `docs/Help/Configuration.md` for
the user-facing story.
