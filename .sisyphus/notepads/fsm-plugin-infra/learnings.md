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
