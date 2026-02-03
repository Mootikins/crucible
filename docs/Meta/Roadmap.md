---
description: Phase-based development timeline for Crucible
type: roadmap
status: active
updated: 2026-01-30
tags:
  - meta
  - planning
  - vision
---

# Crucible Roadmap

> Feature details are tracked in [[Meta/Product]]. This document provides the phase-based timeline view.

## Vision

**"Neovim for agents+notes"** — extensible, open, documented

A knowledge management system where:
- AI agents have perfect context from your kiln
- Workflows are defined in markdown and executed by agents
- Everything is extensible via Lua scripting and hooks
- Power users get CLI, everyone else gets web/desktop UI eventually

## User Progression

| Phase | Users | Interface |
|-------|-------|-----------|
| Now | Power users, developers | CLI (chat-focused) |
| Next | Plugin creators, agent developers | CLI + Lua scripting |
| Later | Non-technical users | Web UI, Tauri desktop |

---

## Phase 0: Core Foundation (Current)

> See [[Meta/Product]] for detailed status of each feature.

**Focus**: Stability, error handling, session persistence, MCP completion.

Key areas: [[Meta/Product#AI Chat & Agents]], [[Meta/Product#MCP Integration]], [[Meta/Product#Configuration & Setup]], [[Meta/Product#Storage & Processing]]

---

## Phase 1: Extensibility Complete

> See [[Meta/Product]] for detailed status of each feature.

**Focus**: Complete Lua scripting API, internal agent system, TUI redesign.

Key areas: [[Meta/Product#Extensibility & Plugins]], [[Meta/Product#Terminal Interface (TUI)]], [[Meta/Product#AI Chat & Agents]]

---

## Phase 2: Workflow Automation

> See [[Meta/Product]] for detailed status of each feature.

**Focus**: Markdown-defined workflows, DAG execution, session learning.

Key areas: [[Meta/Product#Workflow Automation]]

---

## Phase 3: Polish & Rich Features

> See [[Meta/Product]] for detailed status of each feature.

**Focus**: Browser UI, Tauri desktop, rich rendering, note types.

Key areas: [[Meta/Product#Web & Desktop]], [[Meta/Product#Note-Taking & Authoring]]

---

## Phase 4: Scale & Collaborate

> See [[Meta/Product]] for detailed status of each feature.

**Focus**: Multi-device sync, federation, collaborative cognition.

Key areas: [[Meta/Product#Collaboration & Scale]]

---

## Backlog

| Item | Notes |
|------|-------|
| Session compaction with cache purge | When compacting, purge ViewportCache graduated_ids for pre-compaction content. Memory scales with model context length, not full session history. |
| Remove remaining unused deps | `cargo machete` shows unused deps in other crates (core, surrealdb, tools, etc.) |

---

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2024-12-13 | Cut GPUI desktop, keep Tauri | Web tech enables rich features (canvas, mermaid, latex) at low cost |
| 2024-12-13 | Keep ACP | Working, enables Cursor-specific models |
| 2024-12-13 | Event hooks = Tool + Note lifecycle | Focused scope vs. 317-task proposal |
| 2024-12-13 | Keep grammar crate | Integrate with Lua for constrained generation |
| 2024-12-13 | CLI is chat-focused | Other commands for testing, primary UX is conversation |
| 2025-01-23 | Oil UI DSL: Lua + Fennel | React-like functional UI, web deferred to Phase 3 |
| 2025-01-23 | Web is SolidJS, not Svelte | Corrected docs; SolidJS already in use |
| 2025-01-24 | Vim-style :set command | Runtime config overlay with modification tracking |
| 2025-01-24 | Model switching | Runtime model changes via :model command and daemon RPC |

---

## Archived / Cut

| Item | Reason |
|------|--------|
| `crucible-desktop` (GPUI) | Cut — using Tauri + web instead |
| `add-desktop-ui` OpenSpec | Archived — GPUI approach abandoned |
| `add-meta-systems` | Too ambitious (365 tasks), overlaps with focused Lua approach |
| `add-advanced-tool-architecture` | Overlaps with working MCP bridge |
| `add-quick-prompt-features` | Nice UX, not core — revisit in Phase 3 |
| `refactor-clustering-plugins` | Nice feature, not core |
| Ratatui TUI | Removed — migrated to oil-only TUI (2025-01-17) |

---

## Links

- [[Meta/Product]] — Feature map with status and documentation links
- [[Dev Kiln Architecture]] — System architecture
- [[TUI User Stories]] — Chat interface requirements
- [[Plugin User Stories]] — Extension system requirements
- [[Meta/Systems]] — System boundaries
- [[Help/Workflows/Workflow Syntax]] — Workflow syntax reference
- [[Help/Extending/Markdown Handlers]] — Handler syntax reference
- [[Help/Query/Query System]] — Query system reference
