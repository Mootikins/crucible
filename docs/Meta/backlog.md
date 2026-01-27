# Development Backlog

Ideas and improvements to explore later.

## 2026-01-21 - Consolidate Agent Builder Repetition
Simplify repeated agent construction code in `crates/crucible-rig/src/agent.rs`.
Context: While implementing AgentComponents for model switching, noticed significant duplication in tool attachment patterns across `build_agent_with_tools`, `build_agent_with_kiln_tools`, `build_agent_with_model_size`, and `build_agent_from_components_generic`.

## 2026-01-27 - OIL Layout System Improvements (Oracle Analysis)

### P0: Horizontal sizing + Overlay positioning
- Add `width: Size` to `BoxNode` for horizontal sizing (unlocks diff panel, drawer, multi-column layout)
- Extend `OverlayAnchor` with `Positioned` variant (h/v anchors, width/height constraints)
- Add `OverlayLayer` (Float vs Modal) with input routing for permission prompts

### P0: ScrollableNode
- New `Node::Scrollable { child, offset, viewport_height }` for any floating content > viewport
- Render child fully → slice output lines by offset

### P1: Flexbox + constraints
- Wire `min_size`/`max_size` through to Taffy for drawer constraints
- Add `flex-shrink` for narrow terminal graceful degradation

### P1: Lua scripting integration
- Add event callback IDs to NodeSpec for interactive plugin UIs
- Component registry: plugins register reusable components resolved before spec_to_node

### Tech debt: Layout engine consolidation
- Three layout computation paths: layout.rs, taffy_layout.rs, render.rs inline
- Consolidate before adding more features
- overlay compositing is line-based; partial-width overlays need character-level compositing

Context: Oracle consultation during oil-overlay-update plan execution
