# Bug #5: Watch Mode Not Detecting Changes - Implementation Plan

## Problem Statement

**Symptom**: Watch mode starts successfully but never triggers reprocessing when files change.

**Evidence from Manual Testing** (Test Suite 4):
- Test 4.1 (Start watch mode): ✅ PASS - Watch starts and processes initial files
- Test 4.2 (Detect file changes): ❌ FAIL - No reprocessing when file modified
- Manual test showed: File was modified but watch didn't detect/trigger reprocessing

**Log Evidence** (from /tmp/watch-test.log line 30):
```
📝 Change detected: /tmp/test-kiln-1764026375/invalid.md
   ✓ Reprocessed successfully
```
This shows the watcher CAN detect changes, but something prevents it from working reliably.

## Root Cause Analysis

From BUG_FIX_STATUS.md investigation:

1. **Event sender setup has Arc/mutability conflict**
   - Factory creates `Arc<dyn FileWatcher>`
   - Process command needs mutable access to call `set_event_sender()`
   - Can't get mutable reference from Arc

2. **Event sender never properly connected**
   - `set_event_sender()` method requires `&mut self`
   - Arc wrapper prevents getting mutable reference
   - Event channel never gets wired to the watcher

3. **Watch handle discarded**
   - Watch handle prefixed with underscore: `_watch_handle`
   - Rust drops it immediately
   - Watcher stops before events can be processed

4. **No shutdown mechanism**
   - Watch runs indefinitely with no clean shutdown
   - Can't gracefully stop watch when user presses Ctrl+C

## Affected Files

### Primary Files to Modify:
1. **`/home/moot/crucible-fix-process-pipeline/crates/crucible-watch/src/traits.rs`**
   - Lines ~15-30: `FileWatcher` trait definition
   - Change: Add event sender parameter to `watch()` method
   - Remove: `set_event_sender()` method

2. **`/home/moot/crucible-fix-process-pipeline/crates/crucible-watch/src/manager.rs`**
   - Lines ~50-100: `WatchManager` implementation
   - Change: Update `watch()` to accept `mpsc::Sender<FileEvent>`
   - Change: Store sender in watch task, remove from struct

3. **`/home/moot/crucible-fix-process-pipeline/crates/crucible-cli/src/commands/process.rs`**
   - Lines 152-242: Watch mode setup
   - Change: Create event channel and pass sender to `watch()`
   - Change: Keep watch handle alive (remove underscore)
   - Change: Add Ctrl+C handler for graceful shutdown

4. **`/home/moot/crucible-fix-process-pipeline/crates/crucible-cli/src/factories/watch.rs`**
   - New file or existing factory
   - Change: Remove `set_event_sender()` call from factory

### Test Files to Create/Modify:
1. **`/home/moot/crucible-fix-process-pipeline/crates/crucible-watch/tests/watch_integration_test.rs`**
   - New failing test: Watch should detect file changes
   - Verify: Events sent through channel when file modified

## Implementation Plan (TDD Approach)

### Phase 1: RED - Write Failing Test

**File**: `crates/crucible-watch/tests/watch_integration_test.rs`

```rust
#[tokio::test]
async fn test_watch_detects_file_changes() {
    // Setup temp directory with test file
    let temp_dir = create_temp_test_dir();
    let test_file = temp_dir.path().join("test.md");
    fs::write(&test_file, "initial content").await.unwrap();

    // Create event channel
    let (tx, mut rx) = mpsc::channel(100);

    // Create watcher and start watching
    let watcher = WatchManager::new();
    let handle = watcher.watch(temp_dir.path(), tx).await.unwrap();

    // Give watcher time to initialize
    sleep(Duration::from_millis(200)).await;

    // Modify the file
    fs::write(&test_file, "modified content").await.unwrap();

    // Wait for event with timeout
    let event = timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Should receive event within timeout")
        .expect("Channel should not be closed");

    // Verify event
    assert!(matches!(event, FileEvent::Modified(_)));
    assert_eq!(event.path(), &test_file);

    // Cleanup
    drop(handle);
}
```

**Expected Result**: Test FAILS because current API doesn't accept event sender in `watch()`

### Phase 2: GREEN - Fix the API

#### Step 2.1: Update FileWatcher Trait

**File**: `crates/crucible-watch/src/traits.rs`

```rust
#[async_trait]
pub trait FileWatcher: Send + Sync {
    /// Start watching a directory for changes
    ///
    /// # Arguments
    /// * `path` - Directory to watch
    /// * `event_sender` - Channel to send file events
    ///
    /// # Returns
    /// A handle that keeps the watcher alive. Drop to stop watching.
    async fn watch(
        &self,
        path: impl AsRef<Path> + Send,
        event_sender: mpsc::Sender<FileEvent>,
    ) -> Result<WatchHandle>;

    // REMOVED: fn set_event_sender(&mut self, sender: mpsc::Sender<FileEvent>);
}
```

#### Step 2.2: Update WatchManager Implementation

**File**: `crates/crucible-watch/src/manager.rs`

```rust
impl WatchManager {
    pub async fn watch(
        &self,
        path: impl AsRef<Path> + Send,
        event_sender: mpsc::Sender<FileEvent>,
    ) -> Result<WatchHandle> {
        let path = path.as_ref().to_path_buf();

        // Create watcher
        let (tx, mut rx) = mpsc::channel(100);
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        })?;

        // Start watching
        watcher.watch(&path, RecursiveMode::Recursive)?;

        // Spawn event processing task
        let task = tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                // Convert notify event to FileEvent
                let file_event = convert_event(event);

                // Send to pipeline
                if event_sender.send(file_event).await.is_err() {
                    break; // Channel closed, stop watching
                }
            }
        });

        Ok(WatchHandle {
            _watcher: watcher,
            _task: task,
        })
    }
}

pub struct WatchHandle {
    _watcher: Box<dyn notify::Watcher>,
    _task: JoinHandle<()>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        // Watcher and task automatically cleaned up
    }
}
```

#### Step 2.3: Update Process Command

**File**: `crates/crucible-cli/src/commands/process.rs` (lines 152-242)

```rust
// Watch mode
if watch {
    output::info("Starting watch mode");

    // Create event channel for watch events
    let (event_tx, mut event_rx) = mpsc::channel::<FileEvent>(100);

    // Start watching - pass sender directly to watch()
    let watch_handle = file_watcher
        .watch(&path_to_process, event_tx)
        .await
        .context("Failed to start file watcher")?;

    output::success(&format!("Watch started on: {}", path_to_process.display()));
    output::info("Watching for changes (Press Ctrl+C to stop)...");

    // Setup Ctrl+C handler
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        shutdown_clone.store(true, Ordering::SeqCst);
    });

    // Event loop
    while !shutdown.load(Ordering::SeqCst) {
        tokio::select! {
            Some(event) = event_rx.recv() => {
                match event {
                    FileEvent::Modified(file_path) | FileEvent::Created(file_path) => {
                        output::info(&format!("Change detected: {}", file_path.display()));

                        // Reprocess the file
                        match pipeline.process_note(&file_path).await {
                            Ok(_) => output::success("Reprocessed successfully"),
                            Err(e) => output::error(&format!("Reprocess failed: {}", e)),
                        }
                    }
                    FileEvent::Deleted(file_path) => {
                        // Handle deletion if needed
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Check shutdown periodically
            }
        }
    }

    // Keep handle alive until shutdown
    drop(watch_handle);
    output::info("Watch stopped");
}
```

### Phase 3: VERIFY - Test Everything

#### Verification Checklist:

1. **Unit Test**: Run `test_watch_detects_file_changes` - should PASS
2. **Manual Test 4.2**: Modify file in watch mode - should reprocess
3. **Integration**: Full watch mode test suite (Tests 4.1-4.5)
4. **Edge Cases**:
   - Multiple rapid changes (debouncing)
   - Large files
   - Binary files
   - Deleted files
   - Created files

## API Design Notes

### Why Remove `set_event_sender()`?

**Problem with old API**:
```rust
// Factory creates immutable Arc
let watcher: Arc<dyn FileWatcher> = Arc::new(WatchManager::new());

// Can't call mutable method through Arc
watcher.set_event_sender(sender); // ❌ COMPILE ERROR
```

**Solution with new API**:
```rust
// Factory creates immutable Arc
let watcher: Arc<dyn FileWatcher> = Arc::new(WatchManager::new());

// Pass sender when starting watch (no mutation needed)
let handle = watcher.watch(&path, sender).await?; // ✅ WORKS
```

### Handle Lifetime Management

The `WatchHandle` struct owns both the watcher and the event processing task. When dropped:
1. Watcher stops receiving file system events
2. Task completes naturally when channel closes
3. Clean shutdown guaranteed

## Testing Strategy

### Test Coverage:

1. **Unit Tests** (crucible-watch):
   - ✅ Test watch detects file modifications
   - ✅ Test watch detects file creations
   - ✅ Test watch handles deleted files
   - ✅ Test handle drop stops watching

2. **Integration Tests** (crucible-cli):
   - ✅ Test process command watch mode
   - ✅ Test Ctrl+C graceful shutdown
   - ✅ Test multiple file changes
   - ✅ Test reprocessing during watch

3. **Manual Tests**:
   - ✅ Run full Test Suite 4 (5 tests)
   - ✅ Verify no regressions in Suites 1-3

## Migration Path

This is a **breaking change** to the `FileWatcher` trait. However, since this is internal API used only by `crucible-cli`, the impact is minimal.

**Files that need updates**:
- ✅ `crucible-watch/src/manager.rs` - Implementation
- ✅ `crucible-watch/src/traits.rs` - Trait definition
- ✅ `crucible-cli/src/commands/process.rs` - Usage
- ✅ `crucible-cli/src/factories/watch.rs` - Factory (if exists)

## Success Criteria

- [ ] All watch unit tests passing
- [ ] Manual Test 4.2 passing (file change detected)
- [ ] Watch mode works end-to-end
- [ ] Ctrl+C shuts down cleanly
- [ ] No memory leaks (handle dropped properly)
- [ ] Manual Test Suite 4 all passing (5/5 tests)

## Estimated Effort

- **RED Phase**: 30 minutes (write failing test)
- **GREEN Phase**: 1.5 hours (refactor API, update implementations)
- **VERIFY Phase**: 30 minutes (run all tests)
- **Total**: ~2.5 hours

## Notes for Implementation

- Keep changes minimal and focused
- Follow existing error handling patterns
- Use `tokio::select!` for clean shutdown
- Add debug logging for troubleshooting
- Verify handle is NOT prefixed with underscore
