---
title: Architecture
description: Current architecture entry points, focused designs, and clearly dated audit history
tags: [meta, architecture]
---

# Architecture

Start with [[Meta/Product]] for goals, behavior and current proof status.
[[Meta/CONTEXT]] defines the vocabulary. The repository agent guide records
the implementation boundaries and required workflow.

## Current entry points

| Question | Read |
| --- | --- |
| Which subsystem owns the change? | Systems and Type Flows; verify the current types and callers |
| How do configuration and plugins start? | [[Config Boot]], [[State Stores]], [[Meta/Plugin Conventions]] |
| What is the plugin data/render contract? | The Plugin Contract, Plugin API Plan |
| Where does a new tool, provider, client or RPC land? | [[Consolidation Plan#Extension seams]] |
| How does the web window manager work, and where does a layout feature go? | [[Web Windowing]] |
| What proves the recent daemon-first work? | 2026-09-15 Architecture Follow-ups and [[Meta/Product]] |
| What does a user do? | The relevant note under `docs/Help/`, rather than an implementation report |

Focused boundary notes: Storage Schema,
Filesystem Containment, Bash Permission Layers
and Workspace and Runtime Targets.

## Designs, not implementation promises

[[Mobile Shell]] is a design draft. It includes implemented pieces and proposed
work; check [[Meta/Product]] and the relevant Help note before treating an
individual section as shipped. Plugin Web Delivery records
the chosen third-party web isolation design and its implementation trigger.
Canvas and Oil in Documents preserve their
design context.

## Historical audits

[[Expected]], [[Actual]] and [[Gaps]] began as the **2026-08-22** comparison at
`7053bcfe7`, with later dated amendments. They are evidence of that review,
not current normative architecture or an active defect queue. Their source
paths and line numbers belong to those revisions.

[[Consolidation Plan]] retains the resulting decisions and extension seams;
completed per-symbol inventories live in git history.
2026-09-14 Code Reduction Review records the subsequent
reduction outcomes. Use those lessons, but reproduce an old finding before
promoting it to current work.
