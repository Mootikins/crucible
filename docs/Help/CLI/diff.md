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

The daemon refuses a root that is not a registered project, the workspace of a
session or a path inside a registered kiln. To register a repository, run
`cru project register`. The root must also be the top level of its repository.

In the TUI, `:diff [base]` shows the same diff full-screen. See
[[Help/TUI/Commands]].

## See also

- [[Help/CLI/project]] — register a repository
- [[Help/CLI/Index]] — every command
