# Decisions - Config Hierarchy Implementation

## Architectural Choices

### Hook Storage Location
- **Decision**: Store hooks in `LuaExecutor` (not `SessionManager`)
- **Rationale**: Hooks are global configuration, not session-specific. SessionManager is per-session.
- **Source**: Metis gap analysis

### Hook Timing
- **Decision**: Fire hooks after session creation, before first message
- **Rationale**: Session must be bound to RPC before hooks can configure it
- **Source**: Plan design

### Error Isolation
- **Decision**: Hook failures logged but don't block session creation
- **Rationale**: User config errors shouldn't prevent chat from working
- **Source**: Plan guardrails
