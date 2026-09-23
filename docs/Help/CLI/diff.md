---
title: "cru diff"
description: Show a diffset that the daemon computes
tags:
  - reference
  - cli
---

# cru diff

Show a **diffset**: a set of file changes that the daemon computes. The daemon
finds the files and counts the lines. The command reads the texts and prints them.

## Synopsis

```
cru diff branch [--base REF] [--head REF] [--root PATH] [--stat] [-f text|json]
cru diff comments <DIFFSET> [--base REF] [--head REF] [--root PATH] [-f quickfix|json]
```

## branch

Show the changes of a branch since its merge base with a base branch.

```bash
cru diff branch
cru diff branch --base develop
cru diff branch --head HEAD
cru diff branch --stat
```

- `--base` names the base branch. Without it, the daemon uses `origin/HEAD`, then
  `main`, then `master`.
- `--head` names the new side. Without it, the new side is the working tree.
- `--root` names the repository. Without it, the command uses the git top level
  above the working directory.
- `--stat` prints only the file list and the line counts.
- `-f json` prints the diffset and the texts of its files.

The output shows each added, modified, deleted and renamed file. A binary file
and a file larger than 1 MiB show a line that says so. On a terminal, the output
has colors and uses the side-by-side layout when the terminal is wide. In a pipe,
the output is plain unified text.

A diffset can leave out a root that the daemon cannot read. The output then
prints one `warning:` line for each root, with the reason, below the summary.
`-f json` gives the roots in `diffset.unreadable_roots`. Only a session record
leaves out a root, so the list of a branch diff is empty.

The daemon refuses a root that is not a registered project, the workspace of a
session or a path inside a registered kiln. To register a repository, run
`cru project register`. The root must also be the top level of its repository.

In the TUI, `:diff [base]` shows the same diff full-screen. See
[[Help/TUI/Commands]].

## comments

Print the open comments of a diffset.

```bash
cru diff comments session-<id>
cru diff comments branch --base develop
cru diff comments proposal-<uuid> -f json
vim -q <(cru diff comments session-<id>)
```

- The diffset is `session-<id>` for the record of a session, `proposal-<uuid>`
  for a proposal, or `branch` for the branch diff that `--root`, `--base` and
  `--head` name. A `branch-<hex>` id also works when those flags give the same id.
- The default format is `quickfix`. A comment on one line prints as
  `path:line: text`. A comment on a range prints as `path:start: [start-end] text`.
  A second line of comment text has an indent of two spaces.
- Vim reads the quickfix form with its default `errorformat`. The path is
  relative to the root of the comment, so run Vim in that root.
- `-f json` prints each comment with all its fields, with its `id`. Name that
  id in a chat message with `@comment:<id>`: the daemon finds the comment and
  gives the agent the file, the range, the text of the comment and the diff at
  that range. It refuses a message that names an unknown or a resolved comment.
  This is how the TUI attaches a comment, because the TUI has no comment box.

In the web diff pane, each hunk has a header row with its patch range, for
example `@@ -14,7 +14,7 @@`. A click on the header hides the hunk or shows it
again. **Collapse all** in the toolbar hides every hunk, and **Expand all**
shows every hunk again. The chevron of a file hides the whole file.

In the web diff pane, a drag over the line numbers selects a range, and the
comment box opens under it. A drag over the text opens the same box: the range
takes the whole lines at the two ends of the drag, and a wrapped line counts as
one line. A removed row at an end of the drag brings its chunk into the range.
During the drag, only the tint of the lines shows the range. A click opens no
box, and **Cancel** leaves the text selected, so that you can copy it.
**Comment** stores the comment and attaches it to
the chat that the pane header names. It stays disabled until the box has text.
The composer of that chat then shows a chip, for example `server.rs L17–19`,
and the next message carries the comment to the agent. A pane with no chat
stores the comment and says that no chat takes it. **Copy comments** copies the
same quickfix list. **Resolve** on a stored comment marks it resolved, and the
pane then leaves it out of the open comments.

The chip and the stored comment are one thing. The `×` on a chip deletes the
comment, so the comment also leaves the diff pane; resolve keeps a settled
remark, delete says that the author never wrote it. **Attach** on a stored
comment with no chip puts the chip back. A comment that a message already
carried is the exception: the agent has it, so a later `×` only drops the
chip.

## See also

- [[Help/CLI/project]] — register a repository
- [[Help/CLI/Index]] — every command
