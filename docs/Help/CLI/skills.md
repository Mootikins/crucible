---
title: Skills Command
description: CLI reference for working with skills commands.
tags: [help, cli, skills]
---

# cru skills

Inspect the [[Help/Concepts/Agent Skills|Agent Skills]] visible to Crucible. Skills are
folders containing a `SKILL.md` with YAML frontmatter; `cru skills` lists what discovery
found, shows one skill's full instructions, and filters by substring.

All three subcommands go through the daemon, which re-runs discovery on every call — there
is no cache to invalidate after you add a skill.

## Synopsis

```
cru skills list [--scope <scope>] [-f <format>]
cru skills show <name>
cru skills search <query> [-n <limit>]
```

## `cru skills list`

Lists every discovered skill, sorted by name, with its scope and description. The skill
of the highest source is shown by its bare name; every skill it shadows is shown as
`source:name`. The count of shadowed skills shows in the table.

Scope is a *label* for where a skill came from, not the precedence mechanism.
Precedence is the priority of each source (see the table below), and `cru doctor` prints
the search paths per asset kind — start there when a skill you expect is not listed.

| Option | Default | Description |
|--------|---------|-------------|
| `--scope <scope>` | all | Keep only skills whose resolved scope is `builtin`, `personal`, `workspace`, or `kiln` |
| `-f, --format <format>` | terminal: `table`, piped: `plain` | `table`, `plain`, or `json` |

```bash
cru skills list
cru skills list --scope kiln
cru skills list -f json
```

`json` emits an array of `{ name, scope, description, shadowed_count }`.

When nothing is found, the command prints a short hardcoded hint of common skill
locations (personal, workspace, kiln) rather than an empty list. The hint names
`<kiln>/skills/` for the kiln scope, but discovery actually reads
`<kiln>/.crucible/skills/` — see the table below for the real search paths.

## `cru skills show`

Prints one skill's metadata (name, scope, description, source path, originating agent,
license) followed by its full markdown body — the instructions an agent would receive.

```bash
cru skills show commit
```

The full `source:name` always works. A short name goes to the source with the highest
priority. If two sources at that priority hold the name, the error lists the full names. If the name doesn't resolve, the daemon returns an RPC error and the command fails —
use `cru skills list` to see the available names.

## `cru skills search`

Case-insensitive substring match over skill **names and descriptions**. This is plain text
matching, not semantic search — it does not use embeddings.

| Option | Default | Description |
|--------|---------|-------------|
| `-n, --limit <n>` | `10` | Maximum results |

```bash
cru skills search git
cru skills search review -n 25
```

## Discovery and precedence

Discovery collects `<dir>/*/SKILL.md` from each search path below. When two skills share a
name, the higher priority takes the short name, and both stay available under their
source-qualified names.

| Scope | Searched | Source name | Priority |
|-------|----------|-------------|----------|
| `builtin` | `$CRUCIBLE_RUNTIME/skills/` | `env-runtime` | 1000 |
| `personal` | `~/.config/crucible/skills/` | `personal` | 900 |
| `workspace` | `<workspace>/.crucible/skills/` | `workspace` | 800 |
| `workspace` | `<workspace>/.agents/skills/`, `.claude/skills/`, `.codex/skills/`, `.opencode/skills/` | `workspace-agents`, ... | 790 |
| `kiln` | `<kiln>/.crucible/skills/` for each attached kiln | the kiln name | 700 |
| `personal` | `<entry>/skills/` for each `runtimepath` entry | `config-1`, ... | 600, 599, ... |
| `builtin` | `~/.config/crucible/runtime/skills/` | `runtime` | 300 |
| `builtin` | `<plugin>/skills/` for each active plugin | the plugin name | 200 |
| `builtin` | the skills Crucible ships | `builtin` | 100 |

`<workspace>` is the directory where you run `cru`, not the directory of the daemon.

The shipped roots come from `$CRUCIBLE_RUNTIME` when set, otherwise from the layout next to the
`cru` binary.

### Cross-harness skills are opt-in

Crucible can also read skill libraries other coding agents keep in your home directory —
`~/.claude/skills/`, `~/.codex/skills/`, `~/.opencode/skills/`, and `~/.pi/agent/skills/`.
This is **off by default**: a skill body becomes LLM instructions, so anything that can write
to those directories could inject into your sessions. Enable it deliberately:

```bash
CRUCIBLE_CROSS_HARNESS_SKILLS=1 cru skills list
```

Cross-harness paths resolve at `personal` scope, so workspace and kiln skills still win, and
the `agent` field on `cru skills show` records which harness a skill came from.

## See Also

- [[Help/Concepts/Agent Skills]] — the skills specification and frontmatter schema
- [[Help/CLI/Index]] — full CLI command reference
