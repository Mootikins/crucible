# AI Agent Guide for crucible-cli

> **TARGET LOCATION**: `crates/crucible-cli/AGENTS.md`
> 
> This draft should be moved to the CLI crate during plan execution.

---

## Critical Architecture Rule

> **⚠️ CLI/TUI IS A VIEW LAYER ONLY — NO DOMAIN LOGIC**

The CLI and TUI are **presentation layers**. They render state and forward user actions to the daemon. All actual domain logic lives in `crucible-daemon` and `crucible-core`.

### What This Means

```
┌─────────────────────────────────────────────────────────────┐
│  CORRECT Architecture                                       │
│                                                             │
│  CLI/TUI (crucible-cli)     Daemon (crucible-daemon)        │
│  ┌─────────────────────┐    ┌─────────────────────────────┐ │
│  │ • Render messages   │    │ • Session management        │ │
│  │ • Handle key events │───►│ • Handler registration      │ │
│  │ • Display popups    │    │ • Handler execution         │ │
│  │ • Show notifications│◄───│ • Event dispatch            │ │
│  │ • Format output     │    │ • Inject flow               │ │
│  └─────────────────────┘    │ • LLM communication         │ │
│        VIEW ONLY            │ • State persistence         │ │
│                             └─────────────────────────────┘ │
│                                   DOMAIN LOGIC              │
└─────────────────────────────────────────────────────────────┘
```

### DO NOT Put in CLI/TUI

- ❌ Handler registration or execution logic
- ❌ Session state beyond what's needed for display
- ❌ Business rules (e.g., "if todo incomplete, inject message")
- ❌ LLM message processing or transformation
- ❌ Event dispatch to Lua handlers
- ❌ Inject message flow logic

### DO Put in CLI/TUI

- ✅ Rendering components (OIL nodes, layout)
- ✅ Key event handling → RPC calls to daemon
- ✅ Display state (popup visible, scroll position, cursor)
- ✅ UI-only settings (theme, show_thinking toggle)
- ✅ Formatting output for display

### Why This Matters

1. **Multi-client consistency**: Multiple CLI instances connected to same session see same behavior
2. **Testability**: Domain logic in daemon can be tested without TUI
3. **Separation of concerns**: UI changes don't affect business logic
4. **Session-scoped state**: Handlers, injections, and state belong to session, not client

### Example: Handler Execution

**WRONG** (logic in CLI):
```rust
// In chat_runner.rs - DON'T DO THIS
if event.is_turn_complete() {
    let handlers = registry.get_handlers("turn:complete");
    for handler in handlers {
        let result = executor.execute(handler, event);
        if let Inject { content } = result {
            send_to_agent(content); // Business logic in CLI!
        }
    }
}
```

**CORRECT** (CLI just displays, daemon does logic):
```rust
// In chat_runner.rs - CLI just receives events and displays
match daemon_event {
    SessionEvent::InjectionSent { content, source } => {
        // Just display that injection happened
        self.notify(format!("Handler {} injected message", source));
    }
    SessionEvent::MessageReceived { message } => {
        // Just display the message
        self.append_message(message);
    }
}

// In agent_manager.rs (daemon) - All logic here
if event.is_turn_complete() {
    let handlers = session.get_handlers("turn:complete");
    for handler in handlers {
        match executor.execute(handler, event) {
            Inject { content } => {
                session.queue_injection(content);
                self.emit_event(InjectionSent { content, source: handler.name });
            }
        }
    }
}
```

### When In Doubt

Ask: "Would a different client (web UI, API, another TUI) need this same logic?"

- **YES** → Put in daemon/core
- **NO** → OK for CLI (it's truly presentation-only)

## Thin-Client Domain Dependencies

The CLI imports a **minimal set of domain crates** for display purposes only. These are intentional and do NOT violate the view-layer rule:

| Crate | What We Use | Why | Scope |
|-------|------------|-----|-------|
| `crucible-lua` | `LuaNode`, `FennelCompiler` types | Render Lua execution results in chat | Display only |
| `crucible-acp` | `humanize_tool_title()`, `get_known_agents()` | Format tool names and agent lists for display | Display only |
| `crucible-observe` | `SessionId`, `LogEvent` types | Render session metadata and event logs | Display only |

**Key principle:** We import *types and display utilities*, never *execution logic*. The daemon owns all business logic.


## File Organization

| Directory | Purpose |
|-----------|---------|
| `src/commands/` | CLI command implementations (parse args, call daemon, format output) |
| `src/tui/oil/` | TUI components and rendering |
| `src/tui/oil/components/` | Reusable UI components |
| `src/tui/oil/chat_app.rs` | Main TUI state (display state only!) |
| `src/tui/oil/chat_runner.rs` | Event loop (receives daemon events, updates display) |

## Testing

- **Unit tests**: Test rendering, key handling, display logic
- **Snapshot tests**: Verify visual output with insta
- **Integration tests**: Test CLI commands produce correct RPC calls
- **DO NOT**: Test business logic here — that's daemon's job
