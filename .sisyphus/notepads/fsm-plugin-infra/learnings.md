# FSM Plugin Infrastructure - Learnings

## Task 1: RuntimeHandler Function Storage

### Problem
The `crucible.on()` API ignores the function parameter (prefixed with `_handler`) and only stores metadata in `RuntimeHandler` struct. This means handlers can't actually be executed.

### Solution Approach
Since `RuntimeHandler` derives `Clone` and `Debug`, we can't add `RegistryKey` directly (it doesn't implement these traits). Instead:

1. Add a companion `HashMap<String, RegistryKey>` to `LuaScriptHandlerRegistry`
2. Store function in Lua registry via `lua.create_registry_value(handler)`
3. Map handler name → registry key
4. Remove underscore from `_handler` parameter

### Test Strategy (TDD)
1. Write failing test that registers a handler and verifies function is stored
2. Implement the fix
3. Verify test passes

### Files Modified
- `crates/crucible-lua/src/handlers.rs`

## Implementation Complete

### Changes Made
1. Added `RegistryKey` import from mlua
2. Added `HashMap<String, RegistryKey>` field to `LuaScriptHandlerRegistry` to store function references
3. Updated `register_crucible_on_api` to accept `handler_functions` parameter
4. Removed underscore from `_handler` parameter (now `handler`)
5. Implemented function storage via `lua.create_registry_value(handler)`
6. Updated all struct initializations in `new()` and `discover()` methods
7. Updated all test calls to pass the new parameter

### Test Results
- Test `runtime_handler_stores_function_reference` passes
- All 36 handler tests pass
- Function is successfully stored in registry and retrievable via `lua.registry_value(key)`

### Key Implementation Details
- Functions are stored in Lua registry to avoid lifetime issues
- HashMap maps handler name (e.g., "runtime_handler_0") to RegistryKey
- Both `runtime_handlers` Vec and `handler_functions` HashMap are wrapped in Arc<Mutex<>> for thread safety
- The companion HashMap approach works because RuntimeHandler derives Clone/Debug, but RegistryKey doesn't

### Next Steps
Task 2 will implement the execution method to retrieve and call stored functions.

## Task 2: Runtime Handler Execution

### Problem
Runtime-registered handlers (via `crucible.on()`) had their functions stored in the registry (Task 1), but there was no way to execute them. The `LuaScriptHandlerRegistry` needed an `execute_runtime_handler()` method.

### Solution Approach
Implemented `execute_runtime_handler()` method that:
1. Retrieves the stored function from `handler_functions` HashMap using the handler name
2. Creates an empty context table (following pattern from `LuaScriptHandler::execute()`)
3. Converts the event to Lua table using `session_event_to_lua()`
4. Calls the handler function with (ctx, event) parameters
5. Parses the result using `interpret_handler_result()`

### Implementation Details
- Method signature: `pub fn execute_runtime_handler(&self, lua: &Lua, name: &str, event: &SessionEvent) -> LuaResult<ScriptHandlerResult>`
- Returns `LuaError::RuntimeError` if handler name not found
- Reuses existing helper functions to avoid duplication:
  - `session_event_to_lua()` for event conversion
  - `interpret_handler_result()` for result parsing
- Follows the exact pattern from `LuaScriptHandler::execute()` (lines 167-215)

### Test Results
All 3 new tests pass:
- `execute_runtime_handler_receives_event` - Verifies handler receives event with correct fields
- `execute_runtime_handler_returns_cancel` - Verifies `{cancel=true, reason="..."}` produces `ScriptHandlerResult::Cancel`
- `execute_runtime_handler_not_found` - Verifies error when handler name doesn't exist

All 39 handler tests pass (36 existing + 3 new).

### Key Learnings
- `interpret_handler_result()` returns `Ok(ScriptHandlerResult::Cancel)`, not an error
- Empty context table is correct (ctx is just metadata container)
- Handler name lookup must be done with lock on `handler_functions` HashMap
- Method is non-blocking and doesn't require mutable self

### Next Steps
Task 2.5 (event dispatch) will use this method to execute runtime handlers when events are fired.

## Task 2.5: Event Dispatch Loop

### Problem
After `message_complete` event fires in the daemon, we need to dispatch `turn:complete` events to session-scoped Lua handlers registered via `crucible.on()`.

### Solution Approach
1. Added `runtime_handlers_for(event_type: &str)` method to `LuaScriptHandlerRegistry`
   - Returns handlers matching the event type, sorted by priority (lower = earlier)
   - Thread-safe via Arc<Mutex<>>

2. Added session-scoped Lua state to `AgentManager`:
   - `SessionLuaState` struct holds `Lua` instance and `LuaScriptHandlerRegistry`
   - `lua_states: DashMap<String, Arc<Mutex<SessionLuaState>>>` for session isolation
   - `get_or_create_lua_state()` creates Lua state with `crucible.on()` API registered

3. Modified `execute_agent_stream()` to dispatch handlers:
   - Added `lua_state` parameter
   - After `message_complete` event, calls `dispatch_turn_complete_handlers()`
   - Creates `SessionEvent::Custom { name: "turn:complete", payload }` with session/message info
   - Executes each handler in priority order
   - Logs results (debug) and errors (error) but continues chain on failure

### Implementation Details
- Added `handler_functions()` getter to `LuaScriptHandlerRegistry` (was private)
- Added `crucible-lua` and `mlua` dependencies to `crucible-daemon`
- Used re-exports from `crucible_lua` (not private `handlers` module)

### Test Results
All 6 new tests pass:
- `handler_executes_when_event_fires` - Basic handler execution
- `multiple_handlers_run_in_priority_order` - Priority ordering verified
- `handler_errors_dont_break_chain` - Error isolation confirmed
- `handlers_are_session_scoped` - Different sessions have different handlers
- `handler_receives_event_payload` - Event data passed correctly
- `handler_can_return_cancel` - Cancel result works

All 2 `runtime_handlers_for` tests pass in crucible-lua.

### Key Learnings
- Session-scoped Lua state requires `Arc<Mutex<>>` for async compatibility
- `mlua::Lua` is not `Send` by default, but works in async context with proper locking
- Handler dispatch should be fire-and-forget (log results, don't block on them)
- Error isolation is critical - one handler failure shouldn't break the chain

### Files Modified
- `crates/crucible-lua/src/handlers.rs` - Added `runtime_handlers_for()` and `handler_functions()` methods
- `crates/crucible-daemon/src/agent_manager.rs` - Added session Lua state and dispatch loop
- `crates/crucible-daemon/Cargo.toml` - Added `crucible-lua` and `mlua` dependencies

### Next Steps
Task 3.5 will process `Inject` results from handlers to inject messages into the conversation.

## Task 3: ScriptHandlerResult::Inject Variant

### Problem
Handlers needed a way to inject follow-up messages into the conversation. The `ScriptHandlerResult` enum only had Transform, PassThrough, and Cancel variants. We needed to add an Inject variant that handlers could return via `{inject={content="...", position="..."}}` convention.

### Solution Approach
1. Added `Inject` variant to `ScriptHandlerResult` enum with:
   - `content: String` - The message content to inject
   - `position: String` - Where to inject: "user_prefix" (default) or "user_suffix"

2. Updated `interpret_handler_result()` to check for inject convention BEFORE cancel check:
   - Parses `{inject={content="...", position="..."}}` table structure
   - Defaults position to "user_prefix" if not specified
   - Returns early with `ScriptHandlerResult::Inject { content, position }`

3. Added placeholder handling in `execute()` and `execute_json()` methods:
   - Both return `Ok(None)` for Inject results (actual injection happens in daemon, Task 3.5)
   - Added debug logging to track Inject results

### Implementation Details
- Inject check happens FIRST in the table match arm (before cancel check)
- This ensures `{inject={...}, cancel=true}` is treated as Inject, not Cancel
- Position defaults to "user_prefix" via `unwrap_or_else(|_| "user_prefix".to_string())`
- Both `execute()` and `execute_json()` handle Inject by returning None (fire-and-forget)

### Test Results
All 6 new tests pass:
- `test_interpret_handler_result_inject_with_default_position` - Parses inject with default position
- `test_interpret_handler_result_inject_with_custom_position` - Parses inject with custom position
- `test_inject_takes_precedence_over_transform` - Inject checked before Transform
- `test_inject_checked_before_cancel` - Inject checked before Cancel
- `test_handler_returns_inject_with_default_position` - Handler execution with default position
- `test_handler_returns_inject_with_custom_position` - Handler execution with custom position

All 47 handler tests pass (41 existing + 6 new).

### Key Learnings
- Parsing order matters: inject must be checked BEFORE cancel to avoid conflicts
- Default position "user_prefix" is sensible (prepend to user's next message)
- Placeholder handling (returning None) is correct - actual injection is daemon responsibility
- The convention `{inject={content="...", position="..."}}` is clean and Lua-idiomatic

### Files Modified
- `crates/crucible-lua/src/handlers.rs` - Added Inject variant, updated parsing, added tests

### Next Steps
Task 3.5 will implement the actual message injection in the daemon's `execute_agent_stream()` function.
## Task 3.5: Inject Message Flow in Daemon

### Problem
When handlers return `ScriptHandlerResult::Inject`, the daemon needed to:
1. Collect the injection from dispatch
2. Auto-send the injected content to the LLM after response completes
3. Mark injected messages with `is_continuation: true` to prevent infinite loops
4. Emit events for CLI feedback

### Solution Approach
1. Modified `dispatch_turn_complete_handlers()` to return `Option<(String, String)>`:
   - Returns `(content, position)` tuple when handler returns Inject
   - Last inject wins (no queue buildup) - subsequent handlers overwrite previous
   - Returns `None` if no handler returns Inject

2. Added `is_continuation: bool` parameter to both functions:
   - `execute_agent_stream()` - tracks whether this is an injected message
   - `dispatch_turn_complete_handlers()` - passes flag to handlers via event payload
   - Handlers can check `event.payload.is_continuation` to skip injection on continuations

3. Recursive injection handling in `execute_agent_stream()`:
   - After dispatch returns injection, emits `injection_pending` event
   - Drops stream and agent guard to release locks
   - Recursively calls `execute_agent_stream` with `is_continuation=true`
   - Uses `Box::pin()` for async recursion

### Implementation Details
- Event payload includes `is_continuation` flag for handlers to check
- `injection_pending` event emitted with content, position, and is_continuation
- New message_id generated for injected message (`msg-{uuid}`)
- Accumulated response cleared before injection to track new response

### Test Results
All 5 new tests pass:
- `handler_returns_inject_collected_by_dispatch` - Basic inject collection
- `second_inject_replaces_first` - Last inject wins behavior
- `inject_includes_position` - Position field preserved
- `continuation_flag_passed_to_handlers` - is_continuation flag works
- `no_inject_when_handler_returns_nil` - No injection on nil return

All 11 event_dispatch tests pass (6 existing + 5 new).

### Key Learnings
- Async recursion requires `Box::pin()` for the recursive call
- Must drop stream and agent_guard before recursive call to avoid deadlock
- `is_continuation` flag is critical for infinite loop prevention
- Handlers should check `if event.payload.is_continuation then return nil end`
- Event emission pattern: `SessionEventMessage::new(session_id, "event_name", json!({...}))`

### Files Modified
- `crates/crucible-daemon/src/agent_manager.rs`:
  - `dispatch_turn_complete_handlers()` - returns `Option<(String, String)>`, accepts `is_continuation`
  - `execute_agent_stream()` - accepts `is_continuation`, handles injection recursively
  - Added 5 new tests in `event_dispatch` module

### Infinite Loop Prevention Pattern
Handlers should implement this pattern:
```lua
crucible.on("turn:complete", function(ctx, event)
    if event.payload.is_continuation then
        return nil  -- Skip injection on continuation
    end
    return { inject = { content = "Continue" } }
end)
```

### Next Steps
- Task 4: `cru.fmt()` utility (can be done in parallel)
- Task 5: E2E integration test (depends on this task)
