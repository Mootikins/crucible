---
title: "cru kiln"
description: Manage the kilns Crucible knows about
tags:
  - reference
  - cli
---

# cru kiln

Manage the kilns Crucible knows about.

A kiln is addressed everywhere else in Crucible by the **name** of its `[kilns]` entry,
never by its path — that is what keeps your directory layout out of session metadata,
plugin payloads, and the agent's prompt. This is where a directory gets a name.

## Synopsis

```
cru kiln register <NAME> <PATH>
```

## register

Give a directory a name, so sessions can attach it by that name.

```bash
cru kiln register notes ~/vault/notes
```

| Argument | Description |
|----------|-------------|
| `<NAME>` | Name to register the kiln under — lower-case `[a-z0-9._-]`, at most 64 characters, not starting with a dot |
| `<PATH>` | Directory to register. Must be an absolute path, or one that resolves to a directory |

### What it refuses, and why

**Re-pointing an existing name is refused.** Registering a name that is already taken,
against a different directory, is an error rather than an update. Sessions that already
stored that name would silently start opening a different corpus — the failure would be
invisible at the point it mattered, so it is refused at the point it is cheap.

**Names are case-folded.** `cru kiln register Notes ~/vault/notes` after `notes` is
already registered is refused as a duplicate rather than creating a second kiln that
differs only in case.

**Registering the same name and path twice is a no-op**, so the command is safe to run
from a setup script.

### Why the command exists

Two daemon refusals name `cru kiln register` as the remedy: `session.create` telling a
caller that kilns are addressed by the name of a `[kilns]` entry, and the registry
telling a user that every disambiguation of a derived name is taken. An error that names
a command which does not exist is worse than one that names nothing.

## Where names come from otherwise

You do not have to register a kiln by hand. `cru acp --kiln <path>` registers an
unregistered directory under a name derived from its basename, and a bare `cru acp`
inside a kiln does the same for whatever it discovers — see [[Help/CLI/acp]]. Use
`cru kiln register` when you want the name to be something other than the basename, or
when a derived name has already been taken.

## See also

- [[Help/CLI/acp]] — `--kiln` accepts either a registered name or a directory
- [[Help/Core/Sessions]] — how a session's attached kilns are stored
