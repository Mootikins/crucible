# Working on crucible-cli

Read the root `AGENTS.md` first. This file adds the rules for the `cru`
binary and the TUI.

## The CLI is a client

The CLI and the TUI show state and send user intent to the daemon. The daemon
decides. The CLI must not hold business logic, session state that other
clients need, or a second agent configuration.

To decide where code goes, ask one question: would another client (the web
client, `cru acp`, a second TUI) need the same logic?
- **Yes:** put it in the daemon or in `crucible-core`.
- **No:** it can stay in the CLI, because it is presentation only.

| Put in the CLI | Put in the daemon |
|---|---|
| Rendering: Oil nodes, layout, themes | Session management and session settings |
| Key handling that becomes an RPC call | Handler registration and execution |
| Display state: popups, scroll, cursor | Injection, turn and tool-call flow |
| Display settings: theme, show-thinking | LLM message processing |
| Output formatting | Event dispatch to Lua handlers |

A setting that two clients on one session must agree on is a session
setting. Use the cross-layer checklist in the root `AGENTS.md`.

## Imports from other crates

The CLI imports types and display helpers from `crucible-daemon`,
`crucible-lua` and `crucible-oil`. It must not hold its own copy of daemon
logic.

A few commands run daemon code in the CLI process, for offline and
diagnostic use. Each one calls the daemon's own function, not a copy:
- `cru plugin add` without a reachable daemon (`plugin_ops::install`).
- `cru plugin stubs --offline` and `cru plugin check` (`DaemonPluginLoader`).
- `cru doctor` and the bootstrap commands (`evaluate_boot_config`), which
  must work when the daemon cannot start.

When a daemon runs, a command uses its RPC. Do not add a client-side copy
of daemon behavior.

## Where things are

| Path | Role |
|---|---|
| `src/main.rs`, `src/cli/` | The `cru` entry point and the clap argument types |
| `src/commands/` | One module for each `cru` subcommand: parse the arguments, call the daemon, format the output |
| `src/formatting/`, `src/output.rs` | Plain-text output for commands |
| `src/tui/oil/chat_app/` | `OilChatApp`: the TUI reducer and its display state |
| `src/tui/oil/chat_runner/` | The event loop: daemon events in, RPC calls out |
| `src/tui/oil/commands/` | The `:` commands, for example `:set` |
| `src/tui/oil/components/` | Reusable TUI components |
| `src/tui/oil/markdown/` | Markdown to Oil nodes |
| `src/tui/oil/theme/`, `src/tui/oil/config/` | Themes, shortcuts and TUI-local settings |
| `src/tui/oil/tests/` | TUI tests and user-story tests |

[TUI Chat App](<../../docs/Meta/Architecture/TUI Chat App.md>),
[TUI Components](<../../docs/Meta/Architecture/TUI Components.md>) and
[CLI Commands](<../../docs/Meta/Architecture/CLI Commands.md>) describe
these modules in full.

## Tests

- Test rendering, key handling and display logic here.
- Use insta snapshots for visual output. Examine each changed snapshot.
- Test that a CLI command sends the correct RPC call.
- Do not test business logic here. The daemon tests it.
