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
