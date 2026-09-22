---
title: "cru proposal"
description: Review proposals and send the decision of the user
tags:
  - reference
  - cli
---

# cru proposal

Review **proposals**: note writes that wait for your decision. A session in
the `propose` mode, such as the reflection pass, does not write a note. It
makes a proposal. The file on disk changes only when you accept it. See
[[Help/TUI/Modes#Propose Mode]].

## Synopsis

```
cru proposal list [--all] [-f text|json]
cru proposal show <id> [--conflict PATH] [-f text|json]
cru proposal accept <id>
cru proposal reject <id> [--reason TEXT]
cru proposal dismiss <id>
cru proposal resolve <id> <path> --from FILE
```

## list

List the proposals in the Inbox: each open, stale, conflicted or superseded
proposal.

```bash
cru proposal list
cru proposal list --all
cru proposal list -f json
```

- `--all` also lists the accepted, rejected and dismissed proposals.
- `-f json` prints the full records.

Each text line shows the id, the state, the file count, the author (a plugin
name, or `session <id>`) and the title.

## show

Show a proposal: its title, author, state and the diff of each file.

```bash
cru proposal show <id>
```

For a conflicted proposal, the command prints each conflicted file with the
markers `<<<<<<< proposal`, `=======` and `>>>>>>> disk`. Edit that text, then
give it to `cru proposal resolve`.

```bash
# Keep the text of a conflicted file, to resolve it
cru proposal show <id> --conflict notes/a.md > a.md
```

- `--conflict PATH` prints only the marked text of that conflicted file, and
  nothing else.
- `-f json` prints the proposal record.

## accept

Accept a proposal: the daemon writes every file of it.

```bash
cru proposal accept <id>
```

When a file changed on disk since the proposal was made, the daemon merges.
When a merge has a conflict, the daemon writes no file and the proposal
becomes conflicted. Then run `cru proposal show <id> --conflict <path>` and
`cru proposal resolve`.

## reject

Reject a proposal. The files do not change.

```bash
cru proposal reject <id>
cru proposal reject <id> --reason "the note says the opposite"
```

- `--reason` names why the proposal is wrong. A later pass that reads
  `cru.proposals.rejected` sees the title and the reason before it proposes
  again.

## dismiss

Take a proposal out of the Inbox with no decision.

```bash
cru proposal dismiss <id>
```

Use this for a proposal you neither want nor want to reject with a reason —
for example one a newer proposal already superseded.

## resolve

Give the settled text of one conflicted file of a proposal.

```bash
cru proposal show <id> --conflict notes/a.md > a.md
$EDITOR a.md
cru proposal resolve <id> notes/a.md --from a.md

# Read the text from stdin
cru proposal resolve <id> notes/a.md --from - < a.md
```

- `<path>` is the path of the file, relative to its kiln root, as the
  proposal names it.
- `--from FILE` names the file that holds the settled text; `-` reads stdin.

The daemon writes the files of the proposal only once every conflicted file
has a settled text. The command refuses a text that still holds a
`<<<<<<< proposal` or `>>>>>>> disk` marker line.

## See also

- [[Help/TUI/Modes]] — the `propose` mode that creates proposals
- [[Help/CLI/diff]] — `cru diff comments proposal-<uuid>` prints a proposal's open comments
- [[Help/CLI/Index]] — every command
