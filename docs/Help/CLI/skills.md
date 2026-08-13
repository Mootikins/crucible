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

Lists every discovered skill, sorted by name, with its scope and description. A skill that
shadows a same-named skill from a lower scope is annotated with the number it shadows.

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

When nothing is found, the command prints the directories it searched rather than an empty
list.

## `cru skills show`

Prints one skill's metadata (name, scope, description, source path, originating agent,
license) followed by its full markdown body — the instructions an agent would receive.

```bash
cru skills show commit
```

If the name doesn't resolve, the command lists the available names instead of failing.

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
name, the one from the **higher scope** wins and the loser is recorded as shadowed.

| Scope | Searched | Precedence |
|-------|----------|------------|
| `builtin` | `<runtime root>/*/skills/` — the skills Crucible ships | lowest |
| `personal` | `~/.config/crucible/skills/` | |
| `workspace` | `<workspace>/.claude/skills/`, `.codex/skills/`, `.opencode/skills/`, `.crucible/skills/` | |
| `kiln` | `<kiln>/.crucible/skills/` | highest |

Runtime roots come from `$CRUCIBLE_RUNTIME` when set, otherwise from the layout next to the
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
