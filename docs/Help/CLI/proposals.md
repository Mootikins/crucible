---
title: "cru proposals"
description: Review, accept and reject the notes, updates and skills the reflection and consolidation passes propose
tags:
  - reference
  - cli
  - reflection
---

# cru proposals

Review what the [[Help/Concepts/Reflection Pass|reflection pass]] and the consolidation pass proposed, then accept or reject each proposal.

A proposal is a markdown file in `<kiln>/.crucible/proposals/`. That directory is outside the index, so a staged proposal never reaches search or precognition. Nothing lands in the kiln without `cru proposals accept` or a move by hand. The commands need no daemon RPC: they read and move plain files under the kiln the CLI resolves from your configuration.

## Synopsis

```
cru proposals list [-f table|json|plain]
cru proposals show <ID>
cru proposals accept <ID>
cru proposals reject <ID>
```

`<ID>` is the file name without `.md`, for example `reflection-20260702-143210-1-socket-path`. An id with a path separator or `..` is refused.

## list

Print the pending proposals. The `rejected/` directory is not listed.

```bash
cru proposals list
```

```
╭────────────────────────────────────────┬────────┬────────────────────────┬────────────┬──────────────────────┬────────────────────────╮
│ ID                                     │ Kind   │ Title                  │ Target     │ Created              │ Session                │
╞════════════════════════════════════════╪════════╪════════════════════════╪════════════╪══════════════════════╪════════════════════════╡
│ reflection-20260702-143210-1-socket-p… │ create │ How the daemon resolv… │            │ 2026-07-02T14:32:10Z │ [[chat-20260702-1430]] │
│ reflection-20260702-143210-2-just-ci   │ update │ Run just ci before a … │ Notes/ci.md│ 2026-07-02T14:32:10Z │ [[chat-20260702-1430]] │
╰────────────────────────────────────────┴────────┴────────────────────────┴────────────┴──────────────────────┴────────────────────────╯
```

The **Kind** column says what accept will do. The **Target** column names the note an `update` replaces, or where a `create` lands when the reviewer named a path. A terminal gets the table; a pipe gets one record per line. `-f json` prints the same fields as a JSON array.

## show

Print a proposal and what accept will do with it.

```bash
cru proposals show reflection-20260702-143210-2-just-ci
```

What you see depends on the kind:

| Kind | `show` prints |
|------|---------------|
| `create` | The staged file, then `Accept will write <target> in the kiln.` |
| `update` | The staged file, then `Accept will replace <target>:` and a unified diff of the current file against the proposed one |
| `skill` | `Accept will write .crucible/skills/<name>/SKILL.md:` and the exact `SKILL.md` that would land |

The diff and the skill text come from the same functions `accept` calls, so what you read is what accept writes. `show` changes nothing on disk.

## accept

Land the proposal, then remove it from staging. The daemon's file watcher indexes a new or changed note on its next scan.

```bash
cru proposals accept reflection-20260702-143210-2-just-ci
```

| Kind | What accept does | What it refuses |
|------|------------------|-----------------|
| `create` | Writes a new note at `target`, or at `<ID>.md` in the kiln root. The staging keys (`source`, `status`, `session`, `created`, `model`, `kind`, `target`) are stripped; `title`, `tags` and any other key stay | A target that exists, an absolute target, a `..` component, any path under `.crucible/`, and any `SKILL.md` |
| `update` | Replaces the target. A note gets the proposal's frontmatter (minus the staging keys) and body as its whole file. A skill keeps its frontmatter, and only its body is replaced | A target that does not exist, a target under `.crucible/proposals/`, and any path under `.crucible/` other than a `SKILL.md`. A symlink is resolved before the check, so a link into the staging area or out of the kiln is refused too |
| `skill` | Writes `.crucible/skills/<name>/SKILL.md` with the six spec fields only (`name`, `description`, and `license`, `compatibility`, `allowed-tools` when the proposal has them) and provenance under `metadata: crucible-source: reflection`. The staging keys never reach the file | A name that is not spec-valid (1-64 characters, `a-z`, `0-9` and single hyphens, none at either end), a description that is empty, longer than 1024 characters or spans lines, and a name that already has a skill |

A refused proposal stays staged. Edit its frontmatter, or move it by hand.

## reject

Move the proposal into `<kiln>/.crucible/proposals/rejected/`. The file is kept, not deleted.

```bash
cru proposals reject reflection-20260702-143210-1-socket-path
```

The reviewer reads the titles in `rejected/` before it proposes, and is told not to propose them again. `rejection_memory` (default 20) sets how many of the newest titles it is told.

## The notification at session end

When a reflection pass stages at least one proposal, the plugin calls `cru.log.notify` with this message:

```
reflection: 2 proposal(s) staged. Review with `cru proposals list`.
```

The consolidation pass sends the same message with `consolidation:` as its prefix. Each pass also writes the count to the daemon log, on every path that stages a file.

The daemon does not yet deliver a `cru.log.notify` message to a client. The message stays in the plugin VM, so no TUI or web client shows it. This is a known gap (`docs/Meta/Architecture/Gaps.md`, G122). When a session opens, the startup banner says how many proposals are pending in the attached kilns. To see what a pass staged, read the daemon log, or run `cru proposals list`.

## The by-hand path

The staging area is plain files, so a text editor works as well as the commands:

- To **accept** a `create` by hand, move the file out of `.crucible/proposals/` into the kiln, and delete the staging keys from its frontmatter. The watcher indexes it.
- To **reject** by hand, move the file into `.crucible/proposals/rejected/`. The reviewer counts it the same as a `cru proposals reject`.
- To **change** a proposal before you accept it, edit the staged file. `accept` reads the file as it is at that moment.

An `update` or a `skill` is easier through the command: `accept` applies the target checks, keeps a skill's frontmatter, and writes the spec-shaped `SKILL.md`.

## See Also

- [[Help/Concepts/Reflection Pass]] — the pass that stages proposals, and what its reviewer sees
- [[Help/Concepts/Agent Skills]] — the `SKILL.md` format an accepted skill follows
- [[Help/CLI/Index]] — every command
