---
title: backlog
description: Development Backlog
tags:
  - help
---

# Development Backlog

Ideas and improvements to explore later.

## 2026-01-21 - Consolidate Agent Builder Repetition
Simplify repeated agent construction code in `crates/crucible-rig/src/agent.rs`.
Context: While implementing AgentComponents for model switching, noticed significant duplication in tool attachment patterns across `build_agent_with_tools`, `build_agent_with_kiln_tools`, `build_agent_with_model_size`, and `build_agent_from_components_generic`.

## 2026-01-27 - OIL Layout System Improvements (Oracle Analysis)

### P0: Horizontal sizing + Overlay positioning
- Add `width: Size` to `BoxNode` for horizontal sizing (unlocks diff panel, drawer, multi-column layout)
  - Partial 2026-07-10: boxes inside rows (incl. `fixed()`) now auto-size width to
    content instead of claiming the full row, so two-column `row([content, flex(1, …)])`
    layouts work; explicit fixed WIDTH is still unsupported
- Extend `OverlayAnchor` with `Positioned` variant (h/v anchors, width/height constraints)
- Add `OverlayLayer` (Float vs Modal) with input routing for permission prompts

### P0: ScrollableNode
- New `Node::Scrollable { child, offset, viewport_height }` for any floating content > viewport
- Render child fully → slice output lines by offset

### P1: Flexbox + constraints
- Wire `min_size`/`max_size` through to Taffy for drawer constraints
- ~~Add `flex-shrink` for narrow terminal graceful degradation~~ ✅ 2026-07-10: row text
  leaves shrink (`TextNode.no_shrink` opts out for badges), the renderer ellipsizes at
  the laid-out rect instead of bleeding, and spacers keep a 1-cell minimum

### P1: Lua scripting integration
- Add event callback IDs to NodeSpec for interactive plugin UIs
- Component registry: plugins register reusable components resolved before spec_to_node

### Tech debt: Layout engine consolidation
- Three layout computation paths: layout.rs, taffy_layout.rs, render.rs inline
- Consolidate before adding more features
- overlay compositing is line-based; partial-width overlays need character-level compositing

Context: Oracle consultation during oil-overlay-update plan execution

## 2026-07-15 - Second-brain workflow ideas (natural20.com LLM-wiki article)

Source: [Using Claude Code to set up a second brain](https://natural20.com/using-claude-code-to-setup-a-second-brain-aka-llm-wiki) — an agent-maintained wiki design. Mostly validates existing roadmap (precognition citations, kiln-digest-to-inbox); three ideas worth adopting:

- **Ripple ingest skill**: on ingesting a source note, the agent doesn't just file a summary — it updates the 5-15 connected entity/concept pages (backlinks + suggest_links give the fan-out). Ship as a `SKILL.md` + guide, not core code. Pairs with the proposals inbox so ripple edits stay reviewable.
- **`cru kiln lint`** (or a skill): kiln health check — orphan notes, broken wikilinks, missing frontmatter, stale [[deprecated]] markers; contradictions detection is the agent-powered tier. Cheap mechanical version first (parser already has all the data).
- **Second-brain kiln guide/template**: docs/Guides page (or `cru kiln init --template second-brain`) codifying the conventions that make agent maintenance work: `Raw/` immutable sources, `Inbox/` captures, entity/concept pages, deprecate-don't-delete, unverified-claim markers, answer-with-citations + separate graph knowledge from model knowledge (the citation half is the [[Web UI Feature Spec]] precognition-transparency item).
