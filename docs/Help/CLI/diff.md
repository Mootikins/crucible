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
- `-f json` prints each comment with all its fields.

In the web diff pane, each hunk has a header row with its patch range, for
example `@@ -14,7 +14,7 @@`. A click on the header hides the hunk or shows it
again. **Collapse all** in the toolbar hides every hunk, and **Expand all**
shows every hunk again. The chevron of a file hides the whole file.

In the web diff pane, a drag over the line numbers selects a range, and the
comment box opens under it. **Comment** stores the comment. It stays disabled
until the box has text. **Copy comments** copies the same quickfix list, and
**Send to chat** puts the reference `@path:start-end` into the composer.
**Resolve** on a stored comment marks it resolved, and the pane then leaves it
out of the open comments.

## See also

- [[Help/CLI/project]] — register a repository
- [[Help/CLI/Index]] — every command
