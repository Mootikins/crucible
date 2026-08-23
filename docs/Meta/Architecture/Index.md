---
title: Architecture
description: Entry point for the architecture docs. Expected, actual, the gaps between them, and the consolidation plan.
tags: [meta, architecture]
---

# Architecture

Four documents, written on 2026-08-22 at commit `7053bcfe7`.

| Doc | What it answers |
|---|---|
| [[Expected]] | What shape the product docs imply, with no code read. Section 10 lists where two independent drafts disagreed. |
| [[Actual]] | What the code is. Seams, types, traits, duplicates, dead code. Every claim cites `file:line`. |
| [[Gaps]] | Where Expected and Actual differ, one row each, with a verdict on which side is wrong. |
| [[Consolidation Plan]] | The dead code and duplicates to remove, in four tiers by evidence and risk. |

To find a seam's owner, read [[Actual]] section 3. To add a tool, provider,
client, hook stage, storage backend or RPC method, read [[Consolidation Plan]]
section 6. For the older analyses, see [[Systems]], [[Type Flows]],
[[Storage Schema]], [[Canvas]], [[Filesystem Containment]], [[Bash Permission Layers]],
[[Workspace and Runtime Targets]] and [[Fennel for Plugins]].
