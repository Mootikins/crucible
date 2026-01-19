# Event-Driven Architecture

> Design document for Crucible's event-driven core architecture.
> Status: Draft (not committed)

## Overview

Everything in Crucible is async. Rather than explicit procedural pipelines, events become the core orchestration mechanism. Pipelines are just linear event chains; loops (like chat/agent flows) are event chains with cycles.

## Core Model

```
Event → Handler(s) → Event(s) → Handler(s) → ...
```

- **Events** are typed, immutable data
- **Handlers** are stateless async transforms
- **Reactor** dispatches events to handlers via pre-compiled graph
- **Pipelines** are declarative definitions of event flow, not procedural code

## Two-Phase Execution

### Compile Phase (startup)

1. **Discovery**: Scan for handlers (Rust, Lua, MCP)
2. **Topo-sort**: Resolve priorities and dependencies
3. **Build**: Construct immutable `ExecutionGraph`

Output: Frozen execution graph that can be inspected/dumped for debugging.

### Run Phase (event loop)

Reactor follows the pre-built graph:
```
event → lookup handlers → run in order → emit next
```

No sorting, no discovery at runtime. Just execute.

**Benefits:**
- Runtime is dead simple
- Errors surface at startup, not mid-execution
- Graph can be validated at compile time (cycles, missing handlers, type mismatches)
- Predictable performance

**Cost:**
- Hot reload requires recompile (acceptable tradeoff)

## Event Naming Convention

Following Neovim/Claude Code hook patterns:

- **Pre-events**: Interception points. Handlers can modify input, cancel, inject.
- **Result events**: Implicit "post". Observation points for logging, triggering next step.

Example:
```
PreParse → [handlers can modify/cancel] → (parse runs) → NoteParsed
PreLLMCall → [handlers can modify prompt] → (call runs) → LLMComplete
```

## Core Event Types (~20)

### Parse Flow

```rust
FileChanged { path: PathBuf, kind: ChangeKind }
// kind: Create | Modify | Delete

PreParse { path: PathBuf }
// Intercept: skip file, modify path

NoteParsed { path: PathBuf, note: ParsedNote }
// Result of parsing
```

### Enrichment Flow

```rust
PreEnrich { path: PathBuf, note: ParsedNote }
// Intercept: skip enrichment, modify note

NoteEnriched { path: PathBuf, note: EnrichedNote }
// Result: note with embeddings + inferred relations

NoteStored { path: PathBuf, record_id: RecordId }
// Result: persisted to storage
```

### Chat Flow

```rust
UserMessage { content: String, session_id: SessionId }
// Trigger: user typed something

PreCompose { content: String, context: Vec<ContextItem> }
// Intercept: modify prompt, inject context

PromptReady { prompt: String }
// Result: composed prompt ready for LLM

PreLLMCall { prompt: String, model: String }
// Intercept: modify prompt, reroute to different model

LLMChunk { delta: String }
// Stream: partial response (for streaming display)

LLMComplete { response: String, usage: Usage }
// Result: full response received

PreToolCall { name: String, params: Value, format: ToolFormat }
// Intercept: normalize XML→JSON, cancel, modify params
// format: Json | Xml (for handling small model quirks)

ToolResult { name: String, result: Value, success: bool }
// Result: tool execution complete

TurnComplete { response: String }
// Result: full turn done, ready for display/storage
```

### Session

```rust
SessionStart { id: SessionId, mode: Mode }
// Session began

SessionEnd { id: SessionId }
// Session ended

ModeChange { from: Mode, to: Mode }
// Mode switched (plan/act/auto)
```

### System

```rust
Startup { }
// Application started

Shutdown { }
// Application shutting down

Error { source: String, message: String }
// Error occurred (for logging/alerting)
```

## Flow Diagrams

### Parse Pipeline (linear)

```
FileChanged
    ↓
PreParse → [handlers: validate, filter, log]
    ↓
    (parser runs)
    ↓
NoteParsed → [handlers: enrich trigger, log]
    ↓
PreEnrich → [handlers: skip if cached, modify]
    ↓
    (enrichment runs)
    ↓
NoteEnriched → [handlers: store trigger, log]
    ↓
    (storage runs)
    ↓
NoteStored → [handlers: index, notify]
```

### Chat Pipeline (looping)

```
UserMessage
    ↓
PreCompose → [handlers: add context, templates]
    ↓
PromptReady
    ↓
PreLLMCall → [handlers: modify prompt, select model]
    ↓
    (LLM call runs, may stream LLMChunk events)
    ↓
LLMComplete
    ↓
    (parse response for tool calls)
    ↓
┌─► PreToolCall → [handlers: normalize format, validate]
│       ↓
│       (tool executes)
│       ↓
│   ToolResult
│       ↓
│       (if more tool calls, loop back)
└───────┘
    ↓
TurnComplete → [handlers: log, display, store to session]
    ↓
    (if conversation continues, back to UserMessage)
```

## Handler Registration

Handlers declare:
- **Event type** they handle
- **Priority** (higher runs first within same event)
- **Dependencies** (other handlers that must run before)

```rust
#[handler(event = "PreToolCall", priority = 100)]
async fn normalize_xml_tools(ctx: &mut Context, event: PreToolCall) -> HandlerResult {
    if event.format == ToolFormat::Xml {
        // Convert XML to JSON format
        let normalized = xml_to_json(&event.params)?;
        ctx.emit(PreToolCall {
            params: normalized,
            format: ToolFormat::Json,
            ..event
        });
        return Ok(Handled::Replaced);
    }
    Ok(Handled::Continue)
}
```

## Extension Points

### Script Handlers (Lua)

Scripts register handlers at compile phase:

```lua
-- Lua handler
crucible.on("PreCompose", function(event)
    -- Add custom context
    event.context:add(load_custom_context())
    return event
end)
```

### MCP Tool Integration

MCP servers provide tools. Tool calls flow through `PreToolCall` → `ToolResult` like any other tool.

### Custom Providers

LLM providers implement the chat interface. `PreLLMCall` handler can reroute to different providers based on model name or other criteria.

## Comparison to Current Architecture

| Aspect | Current | Event-Driven |
|--------|---------|--------------|
| Parse flow | Procedural pipeline | Linear event chain |
| Chat flow | Complex state machine | Event loop with cycles |
| Extension | Ad-hoc hooks | Uniform handler registration |
| Ordering | Implicit in code | Explicit via priority/deps |
| Debug | Step through code | Inspect compiled graph |

## Migration Path

1. Define event types in `crucible-core::events`
2. Update existing `Handler` trait to match this model
3. Convert parse pipeline to emit events at each stage
4. Convert chat flow to emit events
5. Move extension points to handler registration

## Open Questions

1. **Event storage**: Should events be persisted for replay/debugging?
2. **Event versioning**: How to handle schema changes?
3. **Cancellation**: How does a handler cancel an event chain?
4. **Error propagation**: Does `Error` event stop the chain or just log?

---

*Document created during architecture brainstorm session, 2025-12-29*
