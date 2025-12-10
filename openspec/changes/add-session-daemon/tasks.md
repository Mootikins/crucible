# Session Daemon Tasks

## Phase 1: Foundation (DB Concurrency)

- [ ] 1.1 Define socket protocol (JSON-RPC 2.0 message types)
- [ ] 1.2 Implement daemon server (`crucible-daemon` crate)
- [ ] 1.3 Implement auto-start lifecycle (first chat starts daemon)
- [ ] 1.4 Implement auto-stop lifecycle (last client triggers shutdown)
- [ ] 1.5 Add crash recovery (stale socket detection, PID file)
- [ ] 1.6 Proxy DB operations through daemon
- [ ] 1.7 Session registry (register, heartbeat, deregister)
- [ ] 1.8 Update `cru chat` to detect and connect to daemon

## Phase 2: Session Navigation

- [ ] 2.1 Status bar rendering (single line, session list + inbox badge)
- [ ] 2.2 `/sessions` command (list sessions with status)
- [ ] 2.3 `/goto <n>` command (switch by number)
- [ ] 2.4 `/next`, `/prev` commands (rotate sessions)
- [ ] 2.5 `/new` command (create session with optional worktree/agent flags)
- [ ] 2.6 Preserve session state on switch (input draft, scroll position)

## Phase 3: Inbox (HITL)

- [ ] 3.1 Inbox message schema in SurrealDB
- [ ] 3.2 Agent API: `ctx.notify()`, `ctx.request_decision()`, `ctx.request_approval()`
- [ ] 3.3 `/inbox` command (view messages, mark read)
- [ ] 3.4 Status bar inbox badge (unread count, real-time updates)
- [ ] 3.5 `cru inbox` CLI (works outside chat sessions)
- [ ] 3.6 Blocking behavior for decision/approval requests

## Phase 4: Context Stack

- [ ] 4.1 Context abstraction trait (backend-agnostic)
- [ ] 4.2 Stack operations: `pop(n)`, `checkpoint(name)`, `rollback(name)`, `reset()`
- [ ] 4.3 `replace_top(summary)` using LLM summarization
- [ ] 4.4 `/context` slash commands (pop, checkpoint, rollback, reset, summarize)
- [ ] 4.5 Error handlers (tool failure, approach failure, spiral detection)
- [ ] 4.6 Integration with internal agent system

## Phase 5: Workflow Integration

- [ ] 5.1 Rune API: `session::*` module (start, stop, list, send)
- [ ] 5.2 Rune API: `inbox::*` module (send, wait_for, list)
- [ ] 5.3 Rune API: `context::*` module (pop, checkpoint, rollback, reset)
- [ ] 5.4 Markdown workflow: retry loop syntax
- [ ] 5.5 Markdown workflow: context control directives
- [ ] 5.6 Retry primitive with context control

## Dependencies

```
Phase 1 (Foundation)
    │
    ├─→ Phase 2 (Navigation)
    │       │
    │       └─→ Phase 3 (Inbox)
    │
    └─→ Phase 4 (Context Stack)
            │
            └─→ Phase 5 (Workflows)
```

Phases 2-3 and Phase 4 can run in parallel after Phase 1.
