# Development Backlog

Ideas and improvements to explore later.

## 2026-01-21 - Consolidate Agent Builder Repetition
Simplify repeated agent construction code in `crates/crucible-rig/src/agent.rs`.
Context: While implementing AgentComponents for model switching, noticed significant duplication in tool attachment patterns across `build_agent_with_tools`, `build_agent_with_kiln_tools`, `build_agent_with_model_size`, and `build_agent_from_components_generic`.
