---
title: Session Command
description: CLI reference for managing daemon chat sessions through their full lifecycle.
tags: [help, cli, sessions]
---

# cru session

Manage daemon sessions through their full lifecycle: create, configure, send, pause,
resume, end — plus inspection (`list`, `show`, `search`), the TUI bridge (`open`), and
maintenance (`export`, `reindex`, `cleanup`, `load`). `s` and `sess` are aliases for
`session`.

Every subcommand that takes a `SESSION_ID` accepts it as an optional positional; when
omitted, the `CRU_SESSION` environment variable is used, and the command errors if
neither is present. The scripting pattern:

```bash
ID=$(cru session create -q)
CRU_SESSION=$ID cru session send "hello"
```

`-f/--format json` is available on `create`, `pause`, `resume`, `end`, `configure`,
`list`, `show`, and `search` (and `show` additionally takes `markdown`). The rest print
fixed text. On `create`, `pause`, `resume`, `end`, `configure`, and `search` the value
is validated; on `list` and `show` it is a free string where anything other than
`json`/`markdown` falls back to text.

## Lifecycle

### `cru session create`

Creates a new daemon session.

| Option | Default | Description |
|--------|---------|-------------|
| `-t, --session-type <type>` | `chat` | `chat`, `agent`, or `workflow` (`mcp` is deprecated and maps to `chat` with a warning) |
| `-a, --agent <card>` | — | Agent card to configure: the prompt, model, and tool policy of an internal agent (`cru agents list`) |
| `--acp <profile>` | — | ACP profile instead: an external agent subprocess (`claude`, `gemini`, `codex`, `cursor`, `opencode`, or `[acp.agents.*]`) |
| `--recording-mode <mode>` | — | `granular` or `coarse` |
| `-q, --quiet` | off | Print only the session ID |
| `-f, --format <format>` | `text` | `text` or `json` |
| `--title <title>` | — | Set a title on the new session |
| `--workspace <path>` | current dir | Working directory for the session |
| `--permissions <mode>` | — | `allow`, `deny`, or `ask`; overrides `CRUCIBLE_PERMISSIONS` |

`--agent` and `--acp` are mutually exclusive — a card and a profile live in different
namespaces. If you pass `--agent` with a name that is actually an ACP profile (the old
meaning of the flag), the error tells you to use `--acp`.

When stdout is not a terminal, `create` prints the bare session ID as if `-q` were
passed — unless you explicitly asked for `-f json`, which always wins over the pipe
default. On a terminal, text output includes `export CRU_SESSION=...` lines to copy.

### `cru session configure <id> -p <provider> -m <model>`

Sets the session's agent backend: `-p/--provider` (e.g. `ollama`, `openai`,
`anthropic`), `-m/--model`, optional `-e/--endpoint <url>`, `-f text|json`. This
replaces the whole agent config with an internal agent using those values. For runtime
parameter tweaks on a live session (model, thinking budget), use `cru set` instead.

### `cru session send [<id>] <message>`

Sends a message and streams the response. With `CRU_SESSION` set, a single positional
is the message; without it, the first positional is the session ID. The message can
also be piped on stdin. Response text streams to stdout; thinking, tool calls, and
status markers go to stderr; the command exits when the turn completes. If the session
is not in daemon memory it is loaded from storage automatically.

| Option | Description |
|--------|-------------|
| `--raw` | Print raw JSON event lines instead of formatted output |
| `--permissions <mode>` | `allow`, `deny`, or `ask` — `--permissions allow` bypasses prompts for automation |

(`--session <id>` still works but is deprecated in favor of the positional.)

### `cru session pause / resume / end [<id>]`

State transitions, each taking `-f text|json`. `resume` returns a paused session to
active. `unpause` is a hidden, deprecated spelling of `resume`: it warns on stderr,
runs the same RPC, and takes no `-f` (text output only).

## Inspection

### `cru session list`

Lists daemon sessions in a table (ID, type, state, started).

| Option | Default | Description |
|--------|---------|-------------|
| `-n, --limit <n>` | `20` | Maximum sessions to show |
| `-t, --session-type <type>` | — | Filter: `chat`, `agent`, `workflow` |
| `--state <state>` | — | Filter by daemon state: `active`, `paused`, `ended` |
| `--all` | off | Also list persisted sessions from storage, in a second section |
| `--include-children` | off | Include delegated child sessions (hidden by default) |
| `-f, --format <format>` | `text` | `text` or `json` |

### `cru session show [<id>]`

Shows session details: for a live daemon session, its metadata (type, state, kiln,
started, title); otherwise the stored transcript. `-f` takes `text`, `json`, or
`markdown`.

### `cru session search <query>`

Case-insensitive substring search over stored session transcripts (each session's
JSONL, line by line) — despite the short help saying "by title", it matches message
content too. Prints matching session IDs with line numbers and a context snippet.
`-n/--limit` (default 20), `-f text|json`. Falls back to a local ripgrep/in-memory
scan when the daemon is unreachable.

## TUI bridge and maintenance

### `cru session open [<id>]`

Opens the session in the TUI — the same as `cru chat --resume <id>`.

### `cru session export [<id>]`

Exports the transcript to markdown. `-o/--output <file>` (defaults to `session.md` in
the session's directory), `--timestamps` to include them.

### `cru session reindex`

Rebuilds the session index from the JSONL files on disk. `--force` re-indexes sessions
that are already indexed.

### `cru session cleanup`

Deletes old sessions. `--older-than <days>` (default 30), `--dry-run` to list what
would be deleted without deleting.

### `cru session load [<id>]`

Loads a persisted session from storage into daemon memory without opening it, printing
the event count and resulting state. (`send` does this implicitly when needed.)

## Hidden debugging subcommands

Not shown in `--help`; unstable surface.

- `cru session subscribe <id>...` — subscribes to the given sessions and prints every
  event as it arrives, until Ctrl+C.
- `cru session replay <recording.jsonl>` — replays a recorded session through the
  daemon. `--speed <multiplier>` (default `1.0`, `0` = instant), `--raw` for JSON event
  lines.

## See Also

- [[Help/CLI/chat]] — the interactive TUI these sessions back
- [[Help/CLI/acp]] — ACP sessions are ordinary daemon sessions too
- [[Help/CLI/Index]] — full CLI command reference
