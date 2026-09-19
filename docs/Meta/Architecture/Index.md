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
| Which subsystem owns the change? | The ownership table in the repository agent guide; then verify the current types and callers |
| How do configuration and plugins start? | [[Config Boot]], [[State Stores]], [[Meta/Plugin Conventions]] |
| What is the plugin data/render contract? | [[Meta/Plugin Conventions]], [[Meta/Plugin User Stories]] |
| Where does a new tool, provider, client or RPC land? | [[Consolidation Plan#Extension seams]] |
| How does the web window manager work, and where does a layout feature go? | [[Web Windowing]] |
| What does a user do? | The relevant note under `docs/Help/`, rather than an implementation report |

Deeper per-subsystem notes — the systems inventory and type flows, the storage
schema, filesystem containment, the bash permission layers, and workspace and
runtime targets — are **working notes, not repository content**. They cite line
numbers that move, so they deliberately live outside this kiln, under
`docs/Meta/Analysis/`, and are absent from a clone. Reproduce what one claims
against the current code before acting on it.

## Designs, not implementation promises

[[Mobile Shell]] is a design draft. It includes implemented pieces and proposed
work; check [[Meta/Product]] and the relevant Help note before treating an
individual section as shipped. The chosen third-party web isolation design and
the canvas and Oil-in-documents rendering designs are working notes in the same
untracked tree.

## Historical audits

[[Expected]], [[Actual]] and [[Gaps]] began as the **2026-08-22** comparison at
`7053bcfe7`, with later dated amendments. They are evidence of that review,
not current normative architecture or an active defect queue. Their source
paths and line numbers belong to those revisions.

[[Consolidation Plan]] retains the resulting decisions and extension seams;
completed per-symbol inventories live in git history. Later reduction reviews
are working notes too: use their lessons, but reproduce an old finding before
promoting it to current work.