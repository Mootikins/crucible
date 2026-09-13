---
title: CONTEXT
description: The glossary. One name per concept, and the names to avoid.
type: reference
status: living
updated: 2026-09-12
tags:
  - meta
  - glossary
---

# Crucible

A knowledge-grounded agent runtime. Notes, sessions and wikilinks form a knowledge graph that agents read from and write to. This glossary names the concepts the code and the docs share. It defines what a thing IS. It does not say how a thing works.

## Language

### Places

**Project**:
Where work output goes. A registered directory, the git root or the invocation directory.
_Avoid_: repo, workspace (for this meaning)

**Kiln**:
Where knowledge goes. A session attaches kilns as a flat set. A session is not stored in one.
_Avoid_: vault, knowledge base, notebook

**Workspace**:
One instance of a project directory, the root or a worktree. A runtime concept with no config file.
_Avoid_: project (for this meaning), checkout

**Runtimepath**:
The ordered set of directories the host searches for plugins and Lua modules. A plugin on the runtimepath is discovered.
_Avoid_: search path, plugin path, package.path

### Dispositions

**Review**:
The disposition of an agent's file edits. The composed diff in `review/`.
_Avoid_: proposal (for this meaning), approval

**Proposal**:
The disposition of a suggested knowledge note. Lives under a kiln's `proposals/`.
_Avoid_: review (for this meaning), suggestion

### Plugins

**Plugin**:
Code the operator installed. A directory on the runtimepath with an `init.luau` that returns a module.
_Avoid_: extension, addon, package

**Spec**:
The operator's list of plugins, written in Lua in `init.lua`. One entry per plugin.
_Avoid_: manifest, plugin list, config (for this meaning)

**Spec entry**:
One table in the spec that names a plugin and says where it comes from, whether it is enabled, and what options it gets.
_Avoid_: declaration, plugin config

**Fragment**:
A partial spec entry that a plugin ships beside its own code, that the shipped defaults provide, or that the installed manifest records. The operator's entry for the same name wins.
_Avoid_: metadata file, manifest, spec table

**Discovery**:
Reading every plugin's fragment on the runtimepath. Discovery runs no plugin code.
_Avoid_: scan, load (for this meaning)

**Activation**:
Running a plugin's module once and calling its entry's config. A plugin is active after activation and inactive before it. A disabled plugin is never activated.
_Avoid_: load, enable (for this meaning), setup (for this meaning)

**Source**:
Who defined a Lua registration: a plugin, the user's own Lua, the shipped defaults, or a socket eval.
_Avoid_: owner, origin, provenance (for this meaning)

**Intercept grant**:
A plugin's declared right to take a tool call over. The fragment declares it. The host records it. A plugin cannot grant itself one.
_Avoid_: capability, permission (for this meaning)

### Notes

**Note write**:
One change to a note's text sent to the daemon, online or offline. Either a whole write or an anchored edit.
_Avoid_: save, patch, put

**Whole write**:
A note write that replaces the note's full text.
_Avoid_: save, overwrite

**Anchored edit**:
A note write that changes named lines by their text, and is refused whole when a line moved.
_Avoid_: patch, diff, partial write

**Base**:
The hash of the note text a note write was made from. A write whose base is not the current hash is stale.
_Avoid_: version, etag, revision

**Conflict copy**:
A note written beside the original under a dated name, holding a stale write's text so the writing is not lost.
_Avoid_: backup, merge file

**Outbox**:
The ordered queue of note writes made while offline, replayed in order when the daemon answers again.
_Avoid_: queue, pending writes, sync log

**Kept kiln**:
A kiln this device mirrors for offline reading.
_Avoid_: cached kiln, offline kiln, downloaded kiln
