# Architectural Decisions - Interaction Primitives

## [2026-01-25] Modal Structure

**Decision**: Use dedicated `InteractionModalState` separate from `ShellModal`

**Rationale**:
- Different lifecycle (user-driven vs process-driven)
- Different state (selection vs output buffering)
- Different key handling (structured choices vs scroll)

**Trade-off**: Slight code duplication for modal lifecycle, but cleaner separation of concerns.

## [2026-01-25] Key Handling Dispatch

**Decision**: Refactor `handle_interaction_key()` to dispatch by request type

**Current**: Only handles `Permission` (early returns for other types)

**Target**: 
```rust
match &modal.request {
    InteractionRequest::Ask(ask) => self.handle_ask_key(key, ask, request_id),
    InteractionRequest::Permission(perm) => self.handle_perm_key(key, perm, request_id),
    _ => Action::Continue,
}
```

**Rationale**: Each interaction type has different key bindings and state transitions.

## [2026-01-25] Response Flow

**Decision**: Use `ChatAppMsg::CloseInteraction` with embedded response

**Flow**:
1. User action → build `InteractionResponse`
2. Call `self.close_interaction()` to clear modal state
3. Return `Action::Send(ChatAppMsg::CloseInteraction { request_id, response })`
4. Message handler calls RPC to send response to daemon

**Why not direct RPC call**: Keep view layer pure - message passing for testability.
