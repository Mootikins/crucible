# Crucible Lua API Conventions

## Namespace: `cru`

**Use `cru` everywhere.** Not `crucible`. One name, one convention.

```lua
cru.vault.search("query")
cru.fs.read("file.txt")
cru.on("turn:complete", handler)
cru.fmt("Hello {name}", { name = "world" })
```

**Rationale**: Duplicate names (`cru` + `crucible`) cause divergence in understanding. Pick one, stick with it.

---

## Module Structure

```
cru
├── vault       -- Knowledge base operations
├── fs          -- File system operations  
├── shell       -- Shell command execution
├── http        -- HTTP requests
├── graph       -- Knowledge graph queries
├── mcp         -- MCP server integration
├── ask         -- User prompts/questions
├── popup       -- Popup UI
├── ui          -- Panel UI
├── oil         -- Terminal rendering (OIL components)
├── oq          -- JSON querying (jq-like)
├── fmt()       -- String formatting utility
├── on()        -- Event handler registration
├── once()      -- One-time event handler
├── off()       -- Remove event handler
└── state       -- Session state persistence
```

---

## String Formatting: `cru.fmt()`

```lua
-- Template string interpolation
cru.fmt("Hello {name}, you have {count} todos", {
  name = "Alice",
  count = 3
})
-- → "Hello Alice, you have 3 todos"

-- Missing keys preserved
cru.fmt("Hello {name}", {})
-- → "Hello {name}"
```

**Implementation** (~10 lines):
```lua
function cru.fmt(template, vars)
  return (template:gsub("{(%w+)}", function(key)
    return tostring(vars[key] or "{" .. key .. "}")
  end))
end
```

---

## Event System

### Registration API

```lua
-- Register handler for event pattern
local id = cru.on("turn:complete", function(ctx, event)
  -- Handle event
  return event  -- or nil (pass-through) or { cancel = true }
end, { priority = 10 })

-- One-time handler (auto-removes after first call)
cru.once("session:started", function(ctx, event)
  print("Session initialized")
end)

-- Remove handler
cru.off(id)
```

### Handler Return Conventions

```lua
-- Pass-through (no changes)
return nil

-- Transform event (modify and continue)
event.modified = true
return event

-- Cancel pipeline
return { cancel = true, reason = "Not allowed" }

-- Inject message into conversation (NEW)
return {
  inject = {
    position = "user_prefix",  -- prepend to next user message
    content = "Context: You have 3 incomplete todos.",
  }
}
```

---

## Event Types

### Mapping: Rust SessionEvent → Lua Events

| Rust SessionEvent | Lua Event | Bridged? | Priority |
|-------------------|-----------|----------|----------|
| `MessageReceived` | `message:received` | ❌ TODO | **MUST** |
| `PreLlmCall` | `message:before_send` | ❌ TODO | **MUST** |
| `AgentResponded` | `agent:responded` | ❌ TODO | **MUST** |
| `AgentThinking` | `agent:thinking` | ❌ TODO | SHOULD |
| `PreToolCall` | `tool:before_call` | ✅ Exists | **MUST** |
| `ToolCalled` | `tool:called` | ✅ Exists | SHOULD |
| `ToolCompleted` | `tool:completed` | ✅ Exists | **MUST** |
| `SessionStarted` | `session:started` | ❌ TODO | **MUST** |
| `SessionPaused` | `session:paused` | ❌ TODO | SHOULD |
| `SessionResumed` | `session:resumed` | ❌ TODO | SHOULD |
| `SessionEnded` | `session:ended` | ❌ TODO | **MUST** |
| `AwaitingInput` | `session:idle` | ❌ TODO | **MUST** |
| `SessionCompacted` | `session:compacted` | ❌ TODO | SHOULD |
| `TextDelta` | `streaming:delta` | ❌ TODO | SHOULD |
| `SubagentSpawned` | `task:spawned` | ❌ TODO | SHOULD |
| `SubagentCompleted` | `task:completed` | ❌ TODO | **MUST** |
| `SubagentFailed` | `task:failed` | ❌ TODO | **MUST** |
| `BashTaskCompleted` | `task:completed` | ❌ TODO | **MUST** |
| `BashTaskFailed` | `task:failed` | ❌ TODO | **MUST** |
| `InteractionRequested` | `interaction:requested` | ❌ TODO | SHOULD |
| `InteractionCompleted` | `interaction:completed` | ❌ TODO | SHOULD |
| `NoteParsed` | `note:parsed` | ❌ TODO | NICE |
| `NoteCreated` | `note:created` | ❌ TODO | NICE |
| `NoteModified` | `note:modified` | ❌ TODO | NICE |
| `FileChanged` | `file:changed` | ❌ TODO | NICE |
| *(new)* | `turn:complete` | ❌ TODO | **MUST** |
| *(new)* | `context:overflow` | ❌ TODO | SHOULD |

### Event Categories

#### Session Lifecycle (MUST-HAVE)
```lua
cru.on("session:started", fn)   -- Session initialized
cru.on("session:paused", fn)    -- Agent paused (user takes control)
cru.on("session:resumed", fn)   -- Agent resumes
cru.on("session:ended", fn)     -- Session cleanup
cru.on("session:idle", fn)      -- Awaiting user input (maps to AwaitingInput)
cru.on("turn:complete", fn)     -- Agent finished response (NEW - key for FSM)
```

#### Message Flow (MUST-HAVE)
```lua
cru.on("message:received", fn)      -- User sends message
cru.on("message:before_send", fn)   -- Before LLM call (inject context here)
cru.on("agent:responded", fn)       -- Agent completes response
cru.on("agent:thinking", fn)        -- Thinking block emitted
cru.on("streaming:delta", fn)       -- Each text chunk
```

#### Tool Execution (MUST-HAVE)
```lua
cru.on("tool:before_call", fn)  -- Cancellable, can modify args
cru.on("tool:called", fn)       -- Tool execution started
cru.on("tool:completed", fn)    -- Tool finished successfully
cru.on("tool:failed", fn)       -- Tool failed (maps to error in ToolCompleted)
```

#### Background Tasks (MUST-HAVE)
```lua
cru.on("task:spawned", fn)      -- Subagent or bash task started
cru.on("task:completed", fn)    -- Task finished
cru.on("task:failed", fn)       -- Task failed
```

#### Context Management (SHOULD-HAVE)
```lua
cru.on("context:overflow", fn)  -- Approaching context limit (NEW)
cru.on("session:compacted", fn) -- After compaction
```

#### User Interaction (SHOULD-HAVE)
```lua
cru.on("interaction:requested", fn)  -- Agent requests input (HIL gate)
cru.on("interaction:completed", fn)  -- User responded
```

---

## Implementation Priority

### Phase 1: Core FSM (Week 1)
**Goal**: Enable todo enforcer + ralph loop

```
Must implement:
1. Bridge `message_complete` → `turn:complete`
2. Bridge `AwaitingInput` → `session:idle`  
3. Add `{ inject }` return type
4. Fix cru.on() function storage
5. Standardize on `cru` namespace
```

### Phase 2: Full Message Flow (Week 2)
```
Add:
1. message:received (from MessageReceived)
2. message:before_send (from PreLlmCall)
3. agent:responded (from AgentResponded)
4. Session lifecycle events
```

### Phase 3: Advanced (Week 3+)
```
Add:
1. task:spawned/completed/failed
2. context:overflow (new)
3. streaming:delta
4. interaction:requested/completed
```

---

## State Persistence: `cru.state`

Simple key-value state scoped to session (pure Lua, no Rust needed):

```lua
-- Set state
cru.state.set("ralph_loop", {
  active = true,
  iteration = 5,
  max_iterations = 100,
})

-- Get state
local state = cru.state.get("ralph_loop")

-- Clear state
cru.state.clear("ralph_loop")
```

**Implementation** (JSON file in session dir):
```lua
local STATE_FILE = ".crucible/plugin-state.json"

function cru.state.set(key, value)
  local all = cru.state._load()
  all[key] = value
  cru.fs.write(STATE_FILE, cru.json.encode(all))
end

function cru.state.get(key)
  local all = cru.state._load()
  return all[key]
end

function cru.state._load()
  local content = cru.fs.read(STATE_FILE)
  if content then return cru.json.decode(content) end
  return {}
end
```

---

## Example: Todo Enforcer

```lua
-- plugins/crucible-agents/hooks/todo_enforcer.lua

--- Enforce todo completion
-- @handler event="turn:complete" priority=10
function todo_enforcer(ctx, event)
  -- Skip if continuation
  if ctx.is_continuation then return nil end
  
  -- Check for incomplete todos
  local response = event.response or ""
  if response:match("%- %[ %]") then
    return {
      inject = {
        position = "user_prefix",
        content = cru.fmt([[
<context>
You have incomplete tasks. Continue working.
Mark each task [x] when complete.
</context>
        ]], {}),
      }
    }
  end
  
  return nil  -- Pass through
end
```

---

## Example: Ralph Loop

```lua
-- plugins/crucible-agents/workflows/ralph_loop.lua

local STATE_KEY = "ralph_loop"

function M.start(prompt, opts)
  cru.state.set(STATE_KEY, {
    active = true,
    iteration = 0,
    max_iterations = opts.max_iterations or 100,
    completion_tag = opts.completion_tag or "DONE",
    prompt = prompt,
  })
end

--- Continue loop until completion
-- @handler event="turn:complete" priority=5
function M.on_turn_complete(ctx, event)
  local state = cru.state.get(STATE_KEY)
  if not state or not state.active then return nil end
  
  local response = event.response or ""
  
  -- Check for completion tag
  if response:match(state.completion_tag) then
    state.active = false
    cru.state.set(STATE_KEY, state)
    cru.toast("Ralph loop complete!")
    return nil
  end
  
  -- Check iteration limit
  state.iteration = state.iteration + 1
  if state.iteration >= state.max_iterations then
    state.active = false
    cru.state.set(STATE_KEY, state)
    cru.toast("Max iterations reached")
    return nil
  end
  
  cru.state.set(STATE_KEY, state)
  
  -- Continue working
  return {
    inject = {
      position = "user_prefix",
      content = cru.fmt([[
Continue. You have not indicated completion with '{tag}'.
Iteration {i}/{max}.
      ]], {
        tag = state.completion_tag,
        i = state.iteration,
        max = state.max_iterations,
      }),
    }
  }
end

return M
```

---

## TODO: Implementation Tasks

### Rust Changes (crucible-lua)
- [ ] Rename `crucible` global to `cru` (remove duplicate)
- [ ] Fix `cru.on()` to store Lua function reference
- [ ] Add `ScriptHandlerResult::Inject { message, position }`
- [ ] Bridge daemon events to Lua handler registry
- [ ] Implement `cru.fmt()` in Lua stdlib
- [ ] Implement `cru.state` module (pure Lua)

### Event Bridging (crucible-daemon + crucible-lua)
- [ ] `message_complete` → `turn:complete`
- [ ] `AwaitingInput` → `session:idle`
- [ ] `PreLlmCall` → `message:before_send`
- [ ] `MessageReceived` → `message:received`
- [ ] `AgentResponded` → `agent:responded`
- [ ] Session lifecycle events

### Documentation
- [ ] Update AGENTS.md with Lua API reference
- [ ] Add plugin development guide
- [ ] Document all event types and payloads
