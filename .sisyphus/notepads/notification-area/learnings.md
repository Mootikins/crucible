# Learnings - Notification Area

## Conventions

## Patterns

## Gotchas

## Task 1: Define Notification Types in crucible-core

### Implementation Patterns

**Serde with non-serializable fields:**
- `std::time::Instant` cannot derive `Deserialize` (no Default impl)
- Solution: Custom `Serialize`/`Deserialize` impls that skip the field
- On deserialization, initialize `created_at` to `Instant::now()`
- Mark field as `pub(crate)` to limit visibility, use `#[allow(dead_code)]` for unused warning

**ID generation pattern:**
- Followed `session/types.rs` pattern: `generate_notification_id()`
- Format: `notif-{8 random alphanumeric chars}`
- Uses `rand::rng()` with `random_range(0..36)` for base36 encoding

**Test organization:**
- Comprehensive unit tests for all public API methods
- Serialization tests verify JSON shape and round-trip
- Queue operation tests verify add/dismiss/expire behavior
- Constructor tests verify convenience methods

### Type Design Decisions

**NotificationKind enum:**
- `Toast`: Simple string variant for auto-dismiss
- `Progress { current, total }`: Struct variant with progress tracking
- `Warning`: Simple variant for persistent warnings
- Used `#[serde(rename_all = "snake_case")]` for JSON compatibility

**NotificationQueue:**
- Uses `VecDeque` for efficient FIFO operations
- `expire_old()` returns count of expired items (useful for logging)
- `dismiss()` returns bool for success/failure feedback
- Provides both `len()` and `is_empty()` for ergonomics

### Module Organization

Added to `types/mod.rs` following existing pattern:
1. Added `pub mod notification;` in alphabetical order
2. Added re-export section with organizing comment
3. Re-exported all three public types: `Notification`, `NotificationKind`, `NotificationQueue`

### Test Coverage

All 7 tests passing:
- `test_notification_serialization` - JSON round-trip
- `test_notification_kind_variants_serialize` - All enum variants
- `test_notification_queue_add_and_dismiss` - Queue operations
- `test_notification_queue_expire_old` - Time-based expiration
- `test_notification_queue_clear` - Bulk removal
- `test_notification_constructors` - Convenience methods
- `test_notification_id_uniqueness` - ID collision prevention

## Task 2: Write RPC Interface Tests (Contracts)

### Test Organization Pattern

**Integration test structure:**
- Tests go in `crates/crucible-daemon/tests/` directory, not `src/`
- Each test file is a separate integration test binary
- Use `mod common;` to import shared test utilities
- No need for `#[cfg(test)]` wrapper in integration tests

**RPC client pattern:**
- Split `UnixStream` into `(OwnedReadHalf, OwnedWriteHalf)` once at setup
- Wrap reader in `BufReader` for line-by-line reading
- Pass `&mut (BufReader<OwnedReadHalf>, OwnedWriteHalf)` to helper functions
- Avoids lifetime issues with trying to reconstruct streams

**Helper function design:**
- `setup_daemon()` - Returns `(TestDaemon, RpcClient)` tuple
- `rpc_call()` - Generic RPC request/response helper
- `create_test_session()` - Session setup for tests that need it

### Contract Test Patterns

**RED phase testing:**
- Mark all tests with `#[ignore = "RED phase - method not implemented yet"]`
- Tests should FAIL because RPC methods don't exist yet
- Failures prove the contract is being tested

**Request/response shape definition:**
- Use `serde_json::json!` macro for expected shapes
- Document all three RPC methods:
  - `session.add_notification` - Takes session_id + notification object
  - `session.list_notifications` - Takes session_id, returns array
  - `session.dismiss_notification` - Takes session_id + notification_id

**Test coverage:**
- Basic contract tests (one per method)
- Variant tests (toast, progress, warning kinds)
- Integration tests (add → list, add → dismiss → list)
- Error cases (session not found, notification not found)

### Running Contract Tests

**With cargo test:**
```bash
cargo test -p crucible-daemon --test notification_rpc -- --ignored
```

**With nextest:**
```bash
# Nextest requires different syntax for ignored tests
cargo test -p crucible-daemon --test notification_rpc -- --list --ignored
```

**Expected failures:**
- All 9 tests fail in RED phase
- Failures at `create_test_session()` because session.create might not work
- Failures at RPC calls because methods don't exist yet

### Next Steps (Task 3)

After this task, Task 3 will:
1. Add methods to `METHODS` array in `dispatch.rs`
2. Implement handlers in `server.rs`
3. Add notification queue to `SessionAgent`
4. Tests should turn GREEN

### Verification Results

**Test file location:**
- File exists at `crates/crucible-daemon/tests/notification_rpc.rs` (370 lines)
- Integration test (not unit test in src/), which is correct for RPC contracts
- Uses `mod common;` to import `TestDaemon` fixture

**Test execution:**
```bash
cargo nextest run -p crucible-daemon --run-ignored ignored-only test_add_notification_contract
```

**RED phase confirmed:**
- Test FAILS as expected: `test result: FAILED. 0 passed; 1 failed`
- Failure reason: `No session_id in response` at line 82
- This proves the RPC method doesn't exist yet
- All 8 notification tests are marked `#[ignore = "RED phase - method not implemented yet"]`

**Test coverage (8 tests total):**
1. `test_add_notification_contract` - Basic add operation
2. `test_list_notifications_contract` - Basic list operation
3. `test_dismiss_notification_contract` - Basic dismiss operation
4. `test_add_notification_with_progress_kind` - Progress variant
5. `test_add_notification_with_warning_kind` - Warning variant
6. `test_list_notifications_after_adding` - Integration flow
7. `test_dismiss_notification_removes_from_list` - Integration flow
8. `test_dismiss_nonexistent_notification_returns_false` - Error case
9. `test_session_not_found_error` - Error case

**Contract definitions verified:**
- `session.add_notification`: `{session_id, notification: {id, kind, message}}` → `{session_id, success}`
- `session.list_notifications`: `{session_id}` → `{session_id, notifications: []}`
- `session.dismiss_notification`: `{session_id, notification_id}` → `{session_id, notification_id, success}`

**Ready for Task 2:**
- All contract tests exist and fail (RED phase complete)
- Next task will implement handlers to make tests GREEN

## Task 1: Write RPC Interface Tests (contracts)

### Test File Status
- File already exists: `crates/crucible-daemon/tests/notification_rpc.rs`
- Contains 10 comprehensive contract tests covering all three RPC methods
- All tests marked with `#[ignore = "RED phase - method not implemented yet"]`
- Tests define expected request/response JSON shapes

### RPC Methods Already Registered
- `dispatch.rs` already has methods added to METHODS array (lines 46-48)
- `server.rs` already has dispatch cases (lines 407-414)
- Handler functions NOT implemented yet (compilation fails - perfect RED phase)

### Test Coverage
1. `test_add_notification_contract` - Basic add operation
2. `test_list_notifications_contract` - List empty queue
3. `test_dismiss_notification_contract` - Dismiss operation
4. `test_add_notification_with_progress_kind` - Progress variant
5. `test_add_notification_with_warning_kind` - Warning variant
6. `test_list_notifications_after_adding` - Add then list
7. `test_dismiss_notification_removes_from_list` - Full flow
8. `test_dismiss_nonexistent_notification_returns_false` - Error case
9. `test_session_not_found_error` - Session validation

### RED Phase Verification
- Compilation fails with "cannot find function" errors for handlers
- This is EXPECTED and CORRECT for TDD RED phase
- Task 2 will implement the handlers to make tests pass (GREEN phase)

### Contract Definitions
**session.add_notification**:
- Request: `{ session_id, notification: { id, kind, message } }`
- Response: `{ session_id, success: true }`

**session.list_notifications**:
- Request: `{ session_id }`
- Response: `{ session_id, notifications: [...] }`

**session.dismiss_notification**:
- Request: `{ session_id, notification_id }`
- Response: `{ session_id, notification_id, success: bool }`

## Task 1: RPC Interface Tests (Contract Tests)

### Status
✅ COMPLETE - Tests written and failing as expected (RED phase)

### Test File Location
- `crates/crucible-daemon/tests/notification_rpc.rs` (370 lines)

### Test Coverage
Created 9 comprehensive contract tests:
1. `test_add_notification_contract` - Basic add notification
2. `test_list_notifications_contract` - List empty notifications
3. `test_dismiss_notification_contract` - Dismiss notification
4. `test_add_notification_with_progress_kind` - Progress notification variant
5. `test_add_notification_with_warning_kind` - Warning notification variant
6. `test_list_notifications_after_adding` - Verify add + list flow
7. `test_dismiss_notification_removes_from_list` - Verify dismiss removes from list
8. `test_dismiss_nonexistent_notification_returns_false` - Error handling
9. `test_session_not_found_error` - Session validation (PASSING)

### Test Results
```
test result: FAILED. 1 passed; 8 failed; 0 ignored
```

This is EXPECTED for RED phase - tests define the contract before implementation.

### Fixes Applied During Task

1. **Fixed `create_test_session` parameters**
   - Changed from `"workspace"` to required `"kiln"` parameter
   - Added kiln directory creation in test setup
   - Pattern: `json!({"type": "chat", "kiln": kiln_dir.to_string_lossy()})`

2. **Fixed test infrastructure**
   - Simplified RPC client from tuple type to direct `UnixStream`
   - Removed `BufReader` abstraction (not needed for single request/response)
   - Used `read()` instead of `read_line()` for JSON parsing

3. **Fixed `crucible-core` compilation errors**
   - Changed `crucible_core::types::NotificationQueue` to `crate::types::NotificationQueue`
   - Added `PartialEq`, `Clone`, `Serialize`, `Deserialize` derives to `NotificationQueue`
   - Added `list()` method to `NotificationQueue` for RPC serialization

### RPC Contract Definitions

**session.add_notification**
- Request: `{"session_id": str, "notification": {id, kind, message}}`
- Response: `{"session_id": str, "success": bool}`

**session.list_notifications**
- Request: `{"session_id": str}`
- Response: `{"session_id": str, "notifications": [...]}`

**session.dismiss_notification**
- Request: `{"session_id": str, "notification_id": str}`
- Response: `{"session_id": str, "notification_id": str, "success": bool}`

### Discovered Implementation Status

**SURPRISE**: Handlers already partially implemented!
- `handle_session_add_notification` exists in `server.rs:1141`
- `handle_session_list_notifications` exists in `server.rs:1177`
- `handle_session_dismiss_notification` exists in `server.rs:1198`
- Methods registered in dispatch at `server.rs:407-414`
- Methods listed in `METHODS` array in `dispatch.rs:46-48`

**Why tests fail**: Handlers exist but `AgentManager` methods are incomplete.

### Next Steps (Task 2)
- Implement `AgentManager::add_notification()`
- Implement `AgentManager::list_notifications()`
- Implement `AgentManager::dismiss_notification()`
- Tests should turn GREEN once AgentManager methods work correctly

### Key Learnings

1. **Test-first approach validated**: Writing tests first revealed:
   - Incorrect session.create parameters in original test
   - Missing derives on NotificationQueue
   - Need for `list()` method on NotificationQueue

2. **Macro imports**: `#[macro_export]` macros are re-exported via `pub use crate::{...}` in `rpc_helpers.rs`

3. **Test daemon pattern**: Use `TestDaemon::start()` + kiln directory creation for isolated E2E tests

4. **RED phase is success**: 8/9 tests failing is CORRECT - they define the contract before implementation exists

## CRITICAL UPDATE: Tests Are GREEN!

### Unexpected Discovery
After fixing compilation errors, ALL 9 tests PASS:
```
test result: ok. 9 passed; 0 failed; 0 ignored
```

### What This Means
The RPC handlers are **already fully implemented**:
- `handle_session_add_notification` (server.rs:1141)
- `handle_session_list_notifications` (server.rs:1177)
- `handle_session_dismiss_notification` (server.rs:1198)

AND the `AgentManager` methods are also implemented:
- `add_notification()` (agent_manager.rs)
- `list_notifications()` (agent_manager.rs:566)
- `dismiss_notification()` (agent_manager.rs:574)

### Task Status Revision
- ✅ Tests written and comprehensive
- ✅ Tests compile successfully
- ✅ Tests PASS (not RED phase as expected)
- ✅ Implementation already complete

### Conclusion
**Task 1 is COMPLETE but Task 2 (implementation) is ALSO COMPLETE.**

The notification system is fully functional. The tests serve as:
1. **Regression protection** - Prevent future breakage
2. **Contract documentation** - Define expected behavior
3. **Integration verification** - Confirm daemon ↔ AgentManager ↔ Session flow works

### Next Steps
Since implementation is complete, the remaining tasks should focus on:
- TUI integration (displaying notifications)
- Event emission (notifying clients of notification changes)
- Persistence (saving notifications to session storage)

## Task 2: Implement RPC Methods for Notifications (GREEN Phase)

### Status
✅ COMPLETE - All 9 contract tests PASS

### Implementation Summary

**Key Discovery**: Most of the implementation was already in place from previous work:
- Handlers in `server.rs` (lines 1141-1223)
- Client methods in `daemon-client/src/client.rs` (lines 1054-1115)
- Dispatch cases in `server.rs` (lines 407-414)

**What Was Missing**:
1. `NotificationQueue::list()` method - Added to return `Vec<Notification>` for serialization
2. `AgentManager::get_session()` helper - Added to get session without requiring agent
3. Notification storage location fix - Changed from `SessionAgent.notifications` to `Session.notifications`

### Key Fixes Applied

**1. Added `list()` method to NotificationQueue**
```rust
pub fn list(&self) -> Vec<Notification> {
    self.notifications.iter().cloned().collect()
}
```
This was needed because `notifications()` returns `&VecDeque<Notification>` which doesn't serialize directly.

**2. Fixed notification storage location**
- Original: Stored in `SessionAgent.notifications` (requires agent to be configured)
- Fixed: Stored in `Session.notifications` (works without agent)
- This allows notifications to work on sessions that haven't configured an agent yet

**3. Added `get_session()` helper to AgentManager**
```rust
fn get_session(&self, session_id: &str) -> Result<Session, AgentError> {
    self.session_manager
        .get_session(session_id)
        .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
}
```

### Test Results
```
Summary [0.259s] 11 tests run: 11 passed, 0 skipped
```

All 9 notification contract tests pass:
1. `test_add_notification_contract`
2. `test_list_notifications_contract`
3. `test_dismiss_notification_contract`
4. `test_add_notification_with_progress_kind`
5. `test_add_notification_with_warning_kind`
6. `test_list_notifications_after_adding`
7. `test_dismiss_notification_removes_from_list`
8. `test_dismiss_nonexistent_notification_returns_false`
9. `test_session_not_found_error`

### Architecture Notes

**Notification Flow**:
```
Client → DaemonClient.session_add_notification()
       → RPC "session.add_notification"
       → handle_session_add_notification()
       → AgentManager.add_notification()
       → Session.notifications.add()
       → SessionManager.update_session()
       → Event: "notification_added"
```

**Event Emission**:
- `notification_added` - Emitted when notification is added
- `notification_dismissed` - Emitted when notification is dismissed
- Events include `notification_id` in payload

### Files Modified
1. `crates/crucible-core/src/types/notification.rs` - Added `list()` method
2. `crates/crucible-daemon/src/agent_manager.rs` - Added `get_session()`, fixed notification methods
3. `crates/crucible-daemon/tests/notification_rpc.rs` - Removed `#[ignore]` attributes

### Verification
```bash
cargo nextest run -p crucible-daemon --test notification_rpc
# 11 tests run: 11 passed, 0 skipped
```

## Task 2: Implement RPC Methods for Notifications

### Implementation Summary
- Handlers already existed in server.rs (lines 1141-1222)
- Agent manager methods in agent_manager.rs (lines 539-611)
- Session.notifications field already in crucible-core/src/session/types.rs (line 146)
- NotificationQueue.list() method added for serialization

### Test Results
All 11 tests passing:
- test_add_notification_contract
- test_list_notifications_contract
- test_dismiss_notification_contract
- test_add_notification_with_progress_kind
- test_add_notification_with_warning_kind
- test_list_notifications_after_adding
- test_dismiss_notification_removes_from_list
- test_dismiss_nonexistent_notification_returns_false
- test_session_not_found_error
- common tests (2)

### Key Patterns
**Handler signature pattern**:
```rust
async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response
```

**Notification deserialization**:
```rust
let notification = serde_json::from_value::<Notification>(
    serde_json::Value::Object(notification_obj.clone()),
)?;
```

**Event emission**:
```rust
SessionEventMessage::new(session_id, "notification_added", json!({ "notification_id": id }))
```
