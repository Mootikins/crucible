# Minimal Gaps for FSM-Driven Agent Discipline

## Goal

Implement OMO-like workflow discipline (Ralph loop, todo enforcer) with **minimal changes** to Crucible. Focus on what's needed to dogfood the FSM pattern.

---

## What Crucible Already Has

| Feature | Status | Location |
|---------|--------|----------|
| `@handler event="..." pattern="..." priority=...` | ✅ Works | `crucible-lua/src/annotations.rs` |
| Handler chain (transform/pass-through/cancel) | ✅ Works | `crucible-lua/src/handlers.rs` |
| `PluginManager.register_handler()` | ✅ Works | `crucible-lua/src/lifecycle.rs:608` |
| `crucible.on(event, fn)` | ⚠️ Incomplete | Stores metadata only, not function |
| `message_complete` event | ✅ Emitted | `crucible-daemon/src/agent_manager.rs:317` |
| `AwaitingInput` event | ✅ Defined | `crucible-core/src/events/session_event.rs:163` |
| File read/write from Lua | ✅ Works | `crucible-lua/src/fs.rs` |

---

## What's Missing (Prioritized)

### P0: Critical for Basic FSM

#### 1. **Bridge `message_complete` to Lua Handlers**

**Problem**: Daemon emits `message_complete` but it doesn't reach Lua handlers.

**Solution**: Add event bridging in session/chat flow.

```rust
// In crucible-daemon or crucible-cli chat flow
// After message_complete is emitted:
let event = SessionEvent::Custom {
    name: "turn:complete".to_string(),
    payload: json!({ "session_id": session_id, "response": response }),
};
handlers.dispatch(event);
```

**Effort**: ~2 hours

---

#### 2. **Handler Return: `{ inject = "..." }`**

**Problem**: Handlers can transform/cancel, but can't inject a follow-up message.

**Current**:
```lua
-- @handler event="turn:complete"
function on_turn_complete(ctx, event)
  return { cancel = true, reason = "stop" }  -- Works
  -- But no way to say "inject this message and continue"
end
```

**Needed**:
```lua
-- @handler event="turn:complete"
function todo_enforcer(ctx, event)
  local has_todos = check_for_incomplete_todos()
  if has_todos then
    return {
      inject = "Continue working. Mark tasks complete when done.",
      -- Agent receives this as next user message
    }
  end
end
```

**Implementation**:
```rust
// In handlers.rs - ScriptHandlerResult
pub enum ScriptHandlerResult {
    Transform(JsonValue),
    PassThrough,
    Cancel { reason: String },
    Inject { message: String },  // NEW
}

// In handler chain execution
if let ScriptHandlerResult::Inject { message } = result {
    // Queue message to be sent to agent
    ctx.inject_message(message);
}
```

**Effort**: ~4 hours

---

#### 3. **Fix `crucible.on()` to Store Function**

**Problem**: `crucible.on()` doesn't store the Lua function, only metadata.

**Current** (`handlers.rs:572`):
```rust
let on_fn = lua.create_function(move |_lua, (event_type, _handler): (String, Function)| {
    // handler is ignored! Only event_type stored
    guard.push(RuntimeHandler {
        event_type: event_type.clone(),
        name: name.clone(),
        priority: 100,
    });
    Ok(())
})?;
```

**Fix**: Store function reference in registry.

```rust
// Add function storage
pub struct RuntimeHandler {
    pub event_type: String,
    pub name: String,
    pub priority: i32,
    pub function_key: mlua::RegistryKey,  // NEW: Store in Lua registry
}

// In on_fn creation:
let key = lua.create_registry_value(handler)?;  // Store function
guard.push(RuntimeHandler {
    event_type,
    name,
    priority: 100,
    function_key: key,
});
```

**Effort**: ~2 hours

---

### P1: Needed for Full FSM

#### 4. **Session Context in Handlers**

**Problem**: Handlers get event but not session context (transcript, todos, state).

**Needed**:
```lua
-- @handler event="turn:complete"
function ralph_loop(ctx, event)
  -- Need access to:
  local transcript = ctx.session.transcript  -- Full conversation
  local todos = ctx.session.todos            -- Parsed todo items
  local state = ctx.session.state            -- Custom state storage
end
```

**Implementation**: Enrich `ctx` table passed to handlers.

```rust
fn build_handler_context(session: &Session) -> Table {
    let ctx = lua.create_table()?;
    
    ctx.set("session", {
        let t = lua.create_table()?;
        t.set("id", session.id())?;
        t.set("transcript", session.transcript())?;
        t.set("message_count", session.message_count())?;
        // Lazy-load expensive fields
        t
    })?;
    
    ctx
}
```

**Effort**: ~4 hours

---

#### 5. **State Persistence API**

**Problem**: Ralph loop needs to persist state across turns (iteration count, active flag).

**Needed**:
```lua
-- Simple key-value state scoped to session
crucible.state.set("ralph_loop", { active = true, iteration = 5 })
local state = crucible.state.get("ralph_loop")
```

**Implementation**: JSON file in session directory.

```lua
-- Can implement in pure Lua using crucible.fs!
local state_file = ".crucible/state.json"

function M.set(key, value)
  local state = M.load_all()
  state[key] = value
  crucible.fs.write(state_file, json.encode(state))
end
```

**Effort**: ~1 hour (Lua only, no Rust needed)

---

### P2: Nice to Have

#### 6. **Model/Agent Switching from Handlers**

**Problem**: Can't switch model mid-conversation from Lua.

**Needed**:
```lua
return {
  inject = "Think carefully about this...",
  model = "openai/o1",  -- Switch for this message
}
```

**Effort**: ~4 hours (needs daemon RPC integration)

---

#### 7. **Background Task Spawning**

**Problem**: Can't spawn parallel agent tasks from Lua.

**Needed**:
```lua
crucible.background.spawn({
  agent = "explore",
  prompt = "Find all usages of AuthService",
  on_complete = function(result) ... end,
})
```

**Effort**: ~8 hours (significant daemon work)

---

## Minimal Implementation Order

```
Week 1: Core FSM (P0)
├── Day 1-2: Bridge message_complete → Lua handlers
├── Day 3: Add { inject = "..." } handler return
└── Day 4: Fix crucible.on() function storage

Week 2: Full FSM (P1)
├── Day 1-2: Session context in handlers
└── Day 3: State persistence (Lua-only)

Result: Can implement Ralph loop + todo enforcer in Lua
```

---

## Proof of Concept: Todo Enforcer

With P0 complete, this becomes possible:

```lua
-- plugins/crucible-agents/hooks/todo_enforcer.lua

-- @handler event="turn:complete" priority=10
function todo_enforcer(ctx, event)
  -- Skip if this was an injected continuation
  if ctx.is_continuation then return nil end
  
  -- Check for incomplete todos in response
  local response = event.response or ""
  if response:match("%- %[ %]") then
    -- Found incomplete checkbox - continue working
    return {
      inject = [[
        You have incomplete tasks. Continue working.
        Mark each task [x] when complete.
        Do not stop until all tasks are done.
      ]]
    }
  end
  
  -- No todos, allow natural completion
  return nil
end
```

---

## Proof of Concept: Ralph Loop

```lua
-- plugins/crucible-agents/workflows/ralph_loop.lua
local json = require("cjson")
local STATE_FILE = ".sisyphus/ralph-loop.json"

local function read_state()
  local content = crucible.fs.read(STATE_FILE)
  if content then return json.decode(content) end
  return nil
end

local function write_state(state)
  crucible.fs.write(STATE_FILE, json.encode(state))
end

-- Start ralph loop (called from slash command)
function M.start(prompt, opts)
  write_state({
    active = true,
    iteration = 0,
    max_iterations = opts.max_iterations or 100,
    completion_tag = opts.completion_tag or "DONE",
    prompt = prompt,
  })
end

-- @handler event="turn:complete" priority=5
function M.on_turn_complete(ctx, event)
  local state = read_state()
  if not state or not state.active then return nil end
  
  -- Check for completion tag
  local response = event.response or ""
  if response:match(state.completion_tag) then
    state.active = false
    write_state(state)
    crucible.toast("Ralph loop complete!")
    return nil
  end
  
  -- Check iteration limit
  state.iteration = state.iteration + 1
  if state.iteration >= state.max_iterations then
    state.active = false
    write_state(state)
    crucible.toast("Ralph loop: max iterations reached")
    return nil
  end
  
  write_state(state)
  
  -- Continue working
  return {
    inject = "Continue. You have not indicated completion with '" 
             .. state.completion_tag .. "'. Iteration " 
             .. state.iteration .. "/" .. state.max_iterations
  }
end

return M
```

---

## Architecture Insight

The key insight is that **FSM discipline doesn't require complex Rust machinery**. With three small changes:

1. Event bridging (Rust)
2. Inject return type (Rust)
3. Function storage fix (Rust)

...everything else can be pure Lua:
- State persistence → JSON files
- Todo parsing → Pattern matching
- Agent personalities → System prompt injection
- Category routing → Lookup table

This is the **game engine pattern**: Rust provides primitives, Lua defines behavior.
