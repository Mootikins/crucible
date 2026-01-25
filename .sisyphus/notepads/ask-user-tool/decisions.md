# Decisions - ask_user Tool

## Tool Design

**Decision**: Dedicated `AskUserTool` struct (opt-in per tool)  
**Rationale**: Clean separation, explicit dependency injection, easy to test  
**Alternatives Rejected**: 
- Extend ExecutionContext (bloats all tools)
- Trait-based capability (over-engineering for v1)

## Async Strategy

**Decision**: Use `tokio::sync::oneshot` with `rx.await`  
**Rationale**: Rig tools run async - `blocking_recv()` would deadlock  
**Source**: Metis review, critical finding

## Scope Boundaries

**In Scope**:
- Simple `AskRequest` (single question)
- Backend tool infrastructure
- UI redesign with multi-select

**Out of Scope (v1)**:
- `AskBatch` support (future enhancement)
- Timeout handling (future)
- Permission modal changes (separate task)

## Wiring Strategy

**Decision**: Share `InteractionRegistry` per-session  
**Rationale**: Multiple tools in same session need same registry  
**Pattern**: Create in daemon agent factory, pass via `WorkspaceContext`

---

_Updated: 2026-01-25T22:12:13.505Z_
