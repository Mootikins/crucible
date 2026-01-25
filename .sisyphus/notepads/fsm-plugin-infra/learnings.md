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
