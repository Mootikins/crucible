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
cru kiln list
cru kiln forget <NAME>
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

### Where the registration goes

The command sends the registration to the daemon. The daemon writes it to
`<data_home>/kilns.json`, not to your config file. The reply names the file it wrote.

Two layers hold kiln names. Your config file holds a `[kilns]` table that you edit.
The state file holds the registrations that commands make. The config layer out-ranks
the state layer. Therefore the daemon refuses a name that the config already gives to a
different directory: the state entry would never be used.

The daemon is the only writer of the state file, and it is also the process that
resolves the name. A registration takes effect immediately. You do not have to restart
the daemon.

### Why the command exists

Two daemon refusals name `cru kiln register` as the remedy: `session.create` telling a
caller that kilns are addressed by the name of a `[kilns]` entry, and the registry
telling a user that every disambiguation of a derived name is taken. An error that names
a command which does not exist is worse than one that names nothing.

## list

Show every kiln name Crucible knows, and which layer owns it.

```bash
cru kiln list
```

```
*notes    /home/u/vault/notes   config
 docs     /home/u/docs          config (also registered)
 work     /w/notes              registered
 archive  /home/u/archive       config (shadows registered /old/archive)
 scratch  /p/kiln               discovered
 gone     /home/u/gone          registered (missing)
```

The `origin` column says which layer owns the name.

| Origin | Meaning |
|--------|---------|
| `config` | You declared it in your config file |
| `registered` | A command wrote it into the daemon's state file |
| `discovered` | Something opened the directory by path. It is **not** a kiln that a session can name |

The qualifiers in brackets say what is unusual about the entry.

| Qualifier | Meaning |
|-----------|---------|
| `also registered` | Both layers hold the name, and both point at the same directory. One registration, written down twice |
| `shadows registered <PATH>` | Both layers hold the name, and they disagree. The config wins. The registration does nothing until you forget it |
| `missing` | The directory is gone |
| `lazy` | Crucible does not open or index this kiln until a session asks for it by name |

`*` marks the default kiln.

## forget

Remove one registration from the daemon's state file.

```bash
cru kiln forget work
```

Deleting a kiln from your config does **not** remove a registration. The daemon cannot
tell a line you deleted from a branch that did not run, so absence never deletes. This
command is the removal.

The daemon refuses a name that your config declares, and the refusal names the file to
edit: there is nothing in the state file to forget. The daemon forgets a name that
**both** layers hold, which clears the conflict that `cru kiln list` shows as `shadows`.

The removal takes effect at the next daemon start. A running daemon keeps resolving the
name, because a removal changes what an already-stored session reference means.

## Where names come from otherwise

You do not have to register a kiln by hand. `cru acp --kiln <path>` registers an
unregistered directory under a name derived from its basename, and a bare `cru acp`
inside a kiln does the same for whatever it discovers — see [[Help/CLI/acp]]. Use
`cru kiln register` when you want the name to be something other than the basename, or
when a derived name has already been taken.

**The daemon derives the name, not the CLI.** The derivation depends on what is already
registered: the first `notes` is `notes`, the second is `notes-2`. A registration
derived anywhere else would be derived against a different set of names.

These registrations are marked `auto` in the state file, which records that Crucible
chose the name and you did not.

## See also

- [[Help/CLI/acp]] — `--kiln` accepts either a registered name or a directory
- [[Help/CLI/project]] — the same three verbs over the project registry
- [[Help/Core/Sessions]] — how a session's attached kilns are stored
