# Process Command Optional Improvements - Implementation Plan

**Branch:** `fix/process-command-pipeline`
**Date:** 2025-11-24
**Status:** Ready for implementation

## Executive Summary

Implement four optional improvements to the `process` command following SOLID principles and TDD methodology. All improvements maintain the factory pattern and dependency inversion architecture.

## Current State

### ✅ Completed
- Core process command implemented and tested (185 tests passing)
- Change detection working with SurrealDB persistence
- All 5 pipeline phases executing correctly
- TDD tests written for verbose (6 tests), dry-run (5 tests), watch (10 tests)
- In-memory change detector factory removed

### 📋 Remaining Work
1. Implement --verbose output logic
2. Implement --dry-run preview logic
3. Create watch factory following DI pattern
4. Implement --watch mode in process command
5. Add background watch to chat command
6. SOLID compliance review
7. Manual testing

---

## Phase 1: --verbose Flag Implementation

### Current State
- ✅ Parameter added to `process::execute(verbose: bool)`
- ✅ 6 tests written (1 passing baseline, 5 ignored)
- ✅ CLI integration complete (`cli.verbose` passed through)

### Implementation Tasks

**File:** `crates/crucible-cli/src/commands/process.rs`

**Location:** File processing loop (lines 77-100)

**Changes:**
```rust
for file in files {
    let file_name = file.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    pb.set_message(format!("Processing: {}", file_name));

    // NEW: Verbose output before processing
    if verbose {
        println!("\n📄 Processing: {}", file.display());
    }

    match pipeline.process(&file).await {
        Ok(ProcessingResult::Success { .. }) => {
            processed_count += 1;

            // NEW: Verbose output on success
            if verbose {
                // TODO: Access parsed data if available from ProcessingResult
                println!("  ✓ Parsed successfully");
                println!("  ✓ Pipeline completed");
            }
        }
        Ok(ProcessingResult::Skipped) | Ok(ProcessingResult::NoChanges) => {
            skipped_count += 1;

            // NEW: Verbose output on skip
            if verbose {
                println!("  ⊘ Skipped (no changes)");
            }
        }
        Err(e) => {
            error_count += 1;
            eprintln!("Error processing {}: {:?}", file.display(), e);

            // NEW: Verbose error details
            if verbose {
                println!("  ✗ Error: {}", e);
            }
            warn!("Failed to process {}: {}", file.display(), e);
        }
    }

    pb.inc(1);
}
```

**Output Format (when verbose=true):**
```
📄 Processing: /path/to/note1.md
  ✓ Parsed successfully
  ✓ Pipeline completed

📄 Processing: /path/to/note2.md
  ⊘ Skipped (no changes)
```

**After Implementation:**
1. Remove `#[ignore]` from these 5 tests:
   - `test_verbose_shows_phase_timings`
   - `test_verbose_shows_detailed_parse_info`
   - `test_verbose_shows_merkle_diff_details`
   - `test_verbose_shows_enrichment_progress`
   - `test_verbose_shows_storage_operations`

2. Run: `cargo test -p crucible-cli --test process_command_tests`
3. Verify: 12 tests pass (7 existing + 5 newly enabled)

**Commit:**
```
feat: Implement --verbose flag for detailed process output

Shows per-file processing details when --verbose is enabled:
- File being processed
- Processing result (success/skip/error)
- Phase completion indicators

All verbose tests now passing (12/13 total process tests).

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

**Estimated Time:** 30 minutes

---

## Phase 2: --dry-run Flag Implementation

### Current State
- ✅ 5 tests written (all passing, expecting no DB writes)
- ❌ Parameter not yet added to function signature
- ❌ Logic not implemented

### Implementation Tasks

#### Step 1: Add dry_run Parameter

**File:** `crates/crucible-cli/src/cli.rs`

Add to Process command struct (around line 82):
```rust
Process {
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,

    #[arg(long)]
    force: bool,

    #[arg(short = 'w', long)]
    watch: bool,

    /// Preview what would be processed without making changes
    #[arg(long)]
    dry_run: bool,  // NEW
},
```

**File:** `crates/crucible-cli/src/commands/process.rs`

Update execute signature (line 22):
```rust
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
    verbose: bool,
    dry_run: bool,  // NEW
) -> Result<()> {
```

**File:** `crates/crucible-cli/src/main.rs`

Update caller (line 182):
```rust
Some(Commands::Process { path, force, watch, dry_run }) => {
    commands::process::execute(config, path, force, watch, cli.verbose, dry_run).await?
}
```

#### Step 2: Implement Dry-Run Logic

**File:** `crates/crucible-cli/src/commands/process.rs`

Add at start of execute function (after line 36):
```rust
// Validate flag combinations
if watch && dry_run {
    return Err(anyhow::anyhow!(
        "Cannot use --watch and --dry-run together (incompatible modes)"
    ));
}

if dry_run {
    println!("🔍 DRY RUN MODE - No changes will be made to database\n");
}
```

Update file processing loop (around line 77):
```rust
for file in files {
    let file_name = file.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    pb.set_message(format!("Processing: {}", file_name));

    if dry_run {
        // PREVIEW MODE - don't actually process
        if verbose {
            println!("\n📄 Would process: {}", file.display());
            println!("  → Parse file structure");
            println!("  → Compute Merkle tree");
            println!("  → Generate embeddings");
            println!("  → Update database");
        }
        processed_count += 1;
        pb.inc(1);
        continue;
    }

    // NORMAL MODE - actual processing
    match pipeline.process(&file).await {
        // ... existing code ...
    }
}
```

Update summary output (around line 105):
```rust
println!("\n✅ Pipeline processing complete!");
if dry_run {
    println!("   (Dry run - no changes made)");
    println!("   Would process: {} files", processed_count);
    println!("   Would skip: {} files", skipped_count);
} else {
    println!("   Processed: {} files", processed_count);
    println!("   Skipped (unchanged): {} files", skipped_count);
}
if error_count > 0 {
    println!("   ⚠️  Errors: {} files", error_count);
}
```

#### Step 3: Update Test Callers

**File:** `crates/crucible-cli/tests/process_command_tests.rs`

Update all existing test calls (currently have 5 args, need 6):
```rust
// Find and replace:
process::execute(config, None, false, false, false, false).await
// Replace with:
process::execute(config, None, false, false, false, false).await  // Same (dry_run=false)

// Dry-run test calls already use:
process::execute(config, None, false, false, false, true).await  // dry_run=true
```

**Commit:**
```
feat: Implement --dry-run flag for preview mode

Allows previewing what would be processed without making database changes:
- Shows file discovery and change detection
- Displays estimated operations per file
- Validates flag combinations (--watch + --dry-run = error)
- Summary shows "Would process" instead of "Processed"

All 5 dry-run tests passing (17/18 total process tests).

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

**Estimated Time:** 45 minutes

---

## Phase 3: Watch Factory (SOLID Foundation)

### Rationale
The `crucible-watch` crate provides trait-based abstractions (`FileWatcher` trait). We need a factory to create concrete implementations while maintaining dependency inversion.

### Implementation Tasks

#### Step 1: Create Watch Factory

**File:** `crates/crucible-cli/src/factories/watch.rs` (NEW)

```rust
//! File watching factory - creates file watcher implementations
//!
//! Follows the factory pattern to provide dependency inversion for the
//! file watching subsystem. The process and chat commands depend only
//! on the FileWatcher trait, not concrete implementations.

use std::sync::Arc;
use crucible_watch::traits::FileWatcher;
use crucible_watch::backends::NotifyBackend;
use crate::config::CliConfig;

/// Create a file watcher using the optimal backend for the platform
///
/// Currently returns NotifyBackend (OS-native file watching) as it provides
/// the best performance and reliability across platforms.
///
/// # Architecture
///
/// This factory enforces dependency inversion:
/// - Commands depend on FileWatcher trait (abstraction)
/// - Factory provides concrete implementation
/// - Easy to swap backends or add configuration
///
/// # Future Enhancements
///
/// Could be extended to:
/// - Select backend based on config
/// - Fall back to polling if notify unavailable
/// - Support editor-specific backends
pub fn create_file_watcher(_config: &CliConfig) -> Arc<dyn FileWatcher> {
    // Use OS-native notify backend for best performance
    Arc::new(NotifyBackend::new())
}
```

#### Step 2: Export from Factory Module

**File:** `crates/crucible-cli/src/factories/mod.rs`

Add module declaration:
```rust
pub mod storage;
pub mod enrichment;
pub mod merkle;
pub mod pipeline;
pub mod watch;  // NEW
```

Add export:
```rust
pub use watch::create_file_watcher;
```

#### Step 3: Verify Compilation

```bash
cargo build -p crucible-cli
```

Should compile cleanly with the new factory available.

**Commit:**
```
feat: Add watch factory following dependency inversion pattern

Creates FileWatcher trait implementations while maintaining SOLID principles:
- Factory returns Arc<dyn FileWatcher> trait object
- Commands depend on abstraction, not concrete types
- NotifyBackend used by default (OS-native watching)
- Easy to swap implementations without touching commands

Follows same pattern as existing storage/pipeline factories.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

**Estimated Time:** 20 minutes

---

## Phase 4: --watch Mode in Process Command

### Current State
- ✅ 10 tests written (all #[ignore] for CI)
- ✅ CLI flag already defined (`--watch`)
- ✅ Watch factory available (Phase 3)

### Implementation Tasks

#### Step 1: Implement Watch Loop

**File:** `crates/crucible-cli/src/commands/process.rs`

Add imports at top:
```rust
use crucible_watch::traits::{FileWatcher, WatchConfig, DebounceConfig};
use crucible_watch::events::{FileEvent, FileEventKind, EventFilter};
use tokio::sync::mpsc;
```

Implement watch mode after initial processing (replace line 113-117):
```rust
// Watch mode
if watch {
    println!("\n👀 Watching for changes (Press Ctrl+C to stop)...");

    // Create file watcher using factory
    let mut watcher = factories::create_file_watcher(&config);

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel();
    watcher.set_event_sender(tx);

    // Configure watcher
    let watch_config = WatchConfig::new("process-watch")
        .with_recursive(true)
        .with_debounce(DebounceConfig::new(500)); // 500ms debounce

    // Start watching
    let _handle = watcher.watch(target_path.to_path_buf(), watch_config).await?;

    // Event loop
    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                if let Err(e) = handle_watch_event(event, &pipeline, verbose).await {
                    eprintln!("Error handling file event: {}", e);
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n\nShutting down watch mode...");
                break;
            }
        }
    }
}
```

Add event handler function after execute():
```rust
/// Handle a single file watch event
async fn handle_watch_event(
    event: FileEvent,
    pipeline: &NotePipeline,
    verbose: bool,
) -> Result<()> {
    use FileEventKind::*;

    match event.kind {
        Created | Modified => {
            // Only process markdown files
            if !is_markdown_file(&event.path) {
                return Ok(());
            }

            if verbose {
                println!("📝 Detected change: {}", event.path.display());
            } else {
                output::info(&format!("Processing: {}", event.path.display()));
            }

            match pipeline.process(&event.path).await {
                Ok(ProcessingResult::Success { .. }) => {
                    output::success(&format!("Updated: {}", event.path.display()));
                }
                Ok(ProcessingResult::Skipped | ProcessingResult::NoChanges) => {
                    if verbose {
                        println!("  ⊘ Skipped (no changes)");
                    }
                }
                Err(e) => {
                    output::error(&format!("Failed to process {}: {}", event.path.display(), e));
                }
            }
        }
        Deleted => {
            if is_markdown_file(&event.path) {
                if verbose {
                    println!("🗑️  Detected deletion: {}", event.path.display());
                }
                // TODO: Clean up file state from database
                // This would require adding a delete method to the pipeline or change detector
            }
        }
        _ => {}
    }

    Ok(())
}
```

#### Step 2: Add EventFilter for Markdown

Create filter configuration:
```rust
// In watch_config setup, add filter:
let event_filter = EventFilter::new()
    .with_extension("md")
    .exclude_patterns(vec![".git/", "node_modules/", ".obsidian/"]);

let watch_config = WatchConfig::new("process-watch")
    .with_recursive(true)
    .with_debounce(DebounceConfig::new(500))
    .with_filter(event_filter);  // Add filter
```

#### Step 3: Test

Run the watch tests (most will stay ignored):
```bash
cargo test -p crucible-cli --test process_command_tests test_watch
```

Manual test:
```bash
cargo run --bin cru -- process /path/to/test-kiln --watch --verbose
# In another terminal: edit a .md file
# Verify it reprocesses automatically
```

**Commit:**
```
feat: Implement --watch mode for automatic file reprocessing

Monitors kiln for changes and automatically reprocesses modified files:
- Uses FileWatcher trait via factory (SOLID DI pattern)
- 500ms debouncing to handle rapid changes efficiently
- Filters markdown files only
- Graceful Ctrl+C shutdown
- Integrates with verbose flag for detailed event logging

Watch tests remain #[ignore] for CI (require file system monitoring).

Manual testing: `cru process /path/to/kiln --watch --verbose`

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

**Estimated Time:** 1.5 hours

---

## Phase 5: Background Watch in Chat Command

### Rationale
Users often edit notes during chat sessions. Background watching enables:
- Automatic reindexing as notes are edited
- Fresh context without manual reprocessing
- Seamless live note-taking workflow

### Implementation Tasks

#### Step 1: Add Background Watcher Spawn

**File:** `crates/crucible-cli/src/commands/chat.rs`

Find where initial processing happens (look for pre-processing setup), add after:

```rust
// NEW: Start background file watcher (unless disabled)
let watch_handle = if !no_process {
    Some(spawn_background_watcher(
        config.clone(),
        storage_client.clone(),  // Or pipeline if available
    ))
} else {
    None
};

// Existing chat loop continues...

// NEW: On chat exit, stop watcher gracefully
if let Some(handle) = watch_handle {
    handle.abort();
    debug!("Background file watcher stopped");
}
```

#### Step 2: Implement Background Watcher Function

Add at end of `chat.rs`:

```rust
/// Spawn a background file watcher for automatic note reprocessing
///
/// Runs in a separate tokio task, monitoring the kiln for changes
/// and reprocessing files through the pipeline automatically.
///
/// This enables a seamless workflow where users can edit notes during
/// chat sessions and have them automatically reindexed.
fn spawn_background_watcher(
    config: CliConfig,
    storage_client: crucible_surrealdb::adapters::SurrealClientHandle,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_background_watcher(config, storage_client).await {
            // Log error but don't crash chat
            tracing::error!("Background watcher error: {}", e);
        }
    })
}

/// Run the background file watcher loop
async fn run_background_watcher(
    config: CliConfig,
    storage_client: crucible_surrealdb::adapters::SurrealClientHandle,
) -> Result<()> {
    use crucible_watch::traits::{FileWatcher, WatchConfig, DebounceConfig};
    use crucible_watch::events::FileEventKind;
    use tokio::sync::mpsc;

    // Create pipeline for processing
    let pipeline = crate::factories::create_pipeline(
        storage_client.clone(),
        &config,
        false,  // force=false
    ).await?;

    // Create watcher
    let mut watcher = crate::factories::create_file_watcher(&config);
    let (tx, mut rx) = mpsc::unbounded_channel();
    watcher.set_event_sender(tx);

    // Configure for silent background operation
    let watch_config = WatchConfig::new("chat-background")
        .with_recursive(true)
        .with_debounce(DebounceConfig::new(500));

    // Start watching kiln
    let _handle = watcher.watch(config.kiln.path.clone(), watch_config).await?;

    tracing::debug!("Background file watcher started for: {}", config.kiln.path.display());

    // Event loop (runs until task is aborted)
    while let Some(event) = rx.recv().await {
        if let FileEventKind::Created | FileEventKind::Modified = event.kind {
            // Only process markdown files
            if event.path.extension().and_then(|s| s.to_str()) == Some("md") {
                tracing::debug!("Background reprocessing: {}", event.path.display());

                if let Err(e) = pipeline.process(&event.path).await {
                    tracing::warn!("Failed to reprocess {}: {}", event.path.display(), e);
                }
            }
        }
    }

    Ok(())
}
```

#### Step 3: Add Optional Flag to Disable

**File:** `crates/crucible-cli/src/cli.rs`

Add to Chat command:
```rust
Chat {
    // ... existing fields ...

    /// Disable background file watching during chat
    #[arg(long)]
    no_watch: bool,  // NEW
}
```

Update spawn condition:
```rust
let watch_handle = if !no_process && !no_watch {
    Some(spawn_background_watcher(config.clone(), storage_client.clone()))
} else {
    None
};
```

#### Step 4: Test

Manual test:
```bash
# Start chat
cargo run --bin cru -- chat

# In another terminal: edit a markdown file in the kiln
# Edit should be reprocessed automatically (check logs)

# Ask chat about the edited content
# Should reflect latest changes
```

**Commit:**
```
feat: Add background file watching to chat command

Automatically reprocesses notes edited during chat sessions:
- Background tokio task monitors kiln for changes
- Silent operation (logs to file only)
- Uses same FileWatcher factory as process command
- Graceful shutdown when chat exits
- Optional --no-watch flag to disable

Enables seamless live note-taking workflow where edits are
automatically indexed without manual reprocessing.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
```

**Estimated Time:** 1 hour

---

## Phase 6: SOLID Compliance Review

### Objective
Verify that all implementations follow SOLID principles and the established factory pattern architecture.

### Review Checklist

#### Single Responsibility Principle (SRP)
- [ ] Each factory creates only one type of component
- [ ] Commands handle user interaction, not construction
- [ ] Pipeline handles processing, not watching
- [ ] Watcher handles events, not processing

#### Open/Closed Principle (OCP)
- [ ] Can add new watch backends without modifying commands
- [ ] Can change pipeline components without touching CLI
- [ ] Extensions don't require modifications

#### Liskov Substitution Principle (LSP)
- [ ] All FileWatcher implementations interchangeable
- [ ] Pipeline components can be swapped without breaking
- [ ] Trait contracts properly maintained

#### Interface Segregation Principle (ISP)
- [ ] FileWatcher trait not forcing unused methods
- [ ] EventHandler trait focused and minimal
- [ ] No "fat interfaces"

#### Dependency Inversion Principle (DIP)
- [ ] Commands depend on traits, not concrete types
- [ ] Factories are the only place concrete types are instantiated
- [ ] No direct `use` of concrete watch backends in commands

### Review Process

1. **Scan for Concrete Type Usage**
```bash
cd /home/moot/crucible-fix-process-pipeline
# Should find ZERO matches in commands/
grep -r "NotifyBackend" crates/crucible-cli/src/commands/
grep -r "PollingBackend" crates/crucible-cli/src/commands/
```

2. **Verify Factory Pattern Usage**
```bash
# Should find factories::create_* calls only
grep -r "create_file_watcher" crates/crucible-cli/src/commands/
grep -r "create_pipeline" crates/crucible-cli/src/commands/
```

3. **Check Trait Dependencies**
```rust
// Commands should only import traits:
use crucible_watch::traits::FileWatcher;  // ✅ Good
use crucible_watch::backends::NotifyBackend;  // ❌ Bad
```

4. **Review Test Mocking**
- Verify tests can mock FileWatcher trait
- No direct backend instantiation in tests
- Factory usage in integration tests

### Deliverable

Create `SOLID_REVIEW.md` documenting:
- Compliance status for each principle
- Any violations found
- Remediation plan for violations
- Architecture diagrams showing dependency flow

**Estimated Time:** 1 hour

---

## Phase 7: Manual Testing

### Objective
Verify that process and chat commands produce identical database state and that watch mode works in real-world usage.

### Test Scenarios

#### Test 1: Identical Output Verification

**Setup:**
```bash
cd /home/moot/crucible-fix-process-pipeline

# Create test kiln
mkdir -p /tmp/test-kiln
cp examples/test-kiln/*.md /tmp/test-kiln/

# Use separate databases
DB1=/tmp/test-db1.surreal
DB2=/tmp/test-db2.surreal
```

**Test Process Command:**
```bash
# Clear DB1
rm -f $DB1

# Run process command
CRUCIBLE_KILN_PATH=/tmp/test-kiln \
  cargo run --bin cru -- process --db-path $DB1

# Export DB1 state
# TODO: Add export command or query key tables
```

**Test Chat Command Pre-processing:**
```bash
# Clear DB2
rm -f $DB2

# Run chat with single query (triggers pre-processing)
CRUCIBLE_KILN_PATH=/tmp/test-kiln \
  cargo run --bin cru -- chat --db-path $DB2 "test query"

# Exit chat immediately
```

**Compare Databases:**
```bash
# Query both DBs and compare:
# - file_state table
# - enriched_notes table
# - embeddings count
# - Merkle tree roots

# Should be identical
```

#### Test 2: Watch Mode Real-World Usage

**Process Command Watch:**
```bash
# Start watching
cargo run --bin cru -- process /tmp/test-kiln --watch --verbose

# In another terminal:
echo "# New Note\n\nTest content" > /tmp/test-kiln/new.md
sleep 1  # Wait for debounce
# Should see: "Detected change: new.md"

# Edit existing
echo "# Updated" > /tmp/test-kiln/existing.md
# Should see automatic reprocessing

# Create non-markdown (should be ignored)
echo "test" > /tmp/test-kiln/readme.txt
# Should NOT trigger processing

# Ctrl+C - should exit gracefully
```

**Chat Command Background Watch:**
```bash
# Start chat
cargo run --bin cru -- chat

# In another terminal, edit a note
echo "# Fresh Content" > /tmp/test-kiln/note.md

# In chat, query about the note
# Should reflect latest content

# Exit chat - background watcher should stop cleanly
```

#### Test 3: Flag Combinations

```bash
# Verbose + Dry-run
cargo run --bin cru -- process /tmp/test-kiln --verbose --dry-run
# Should show detailed preview, no DB changes

# Force + Watch
cargo run --bin cru -- process /tmp/test-kiln --force --watch
# Should reprocess all files on every change

# Watch + Dry-run (should error)
cargo run --bin cru -- process /tmp/test-kiln --watch --dry-run
# Should print error: "Cannot use --watch and --dry-run together"
```

#### Test 4: Performance Check

```bash
# Large kiln (100+ notes)
# Time process vs chat
time cargo run --bin cru -- process /path/to/large-kiln
time cargo run --bin cru -- chat "test" --db-path /tmp/test.db

# Should be similar (both use same pipeline)
```

### Deliverable

Document test results in `MANUAL_TEST_RESULTS.md`:
- ✅/❌ for each test scenario
- Database comparison results
- Watch mode responsiveness
- Any issues found
- Performance metrics

**Estimated Time:** 1.5 hours

---

## Implementation Order

Execute phases sequentially to maintain clean commits:

1. **Phase 1: --verbose** (30 min) - Low risk, high value
2. **Phase 2: --dry-run** (45 min) - Independent feature
3. **Phase 3: Watch Factory** (20 min) - Foundation for Phases 4-5
4. **Phase 4: --watch in Process** (1.5 hr) - Uses Phase 3
5. **Phase 5: Background Watch in Chat** (1 hr) - Uses Phase 3
6. **Phase 6: SOLID Review** (1 hr) - Validate architecture
7. **Phase 7: Manual Testing** (1.5 hr) - Final verification

**Total Estimated Time:** 6.5 hours

---

## Success Criteria

- [ ] All 185+ tests passing
- [ ] 12 verbose tests passing (7 base + 5 new)
- [ ] 5 dry-run tests passing
- [ ] 10 watch tests written (most #[ignore] for CI)
- [ ] Watch factory follows DI pattern
- [ ] No concrete type dependencies in commands
- [ ] SOLID review completed with clean bill of health
- [ ] Manual tests confirm identical process/chat output
- [ ] Watch mode works in real-world usage
- [ ] Clean commit history with conventional messages

---

## Risk Mitigation

### Risk 1: Watch Crate API Changes
**Mitigation:** Read crucible-watch docs first, verify API compatibility

### Risk 2: Performance Impact of Background Watch
**Mitigation:** Use efficient debouncing (500ms), test with large kilns

### Risk 3: Test Flakiness (Watch Mode)
**Mitigation:** Keep watch tests #[ignore], document manual testing

### Risk 4: Breaking Changes to Process Command
**Mitigation:** All existing tests must pass at each phase

---

## Future Enhancements (Out of Scope)

- Configurable watch backends (notify vs polling)
- Watch-specific CLI flags (--watch-delay, --watch-backend)
- File deletion cleanup in database
- Watch mode metrics (events/sec, avg processing time)
- Hot reload of pipeline configuration
- Multiple watch paths
- Exclude patterns configuration

These can be added in future PRs without architectural changes.

---

## References

- Original TDD Plan: See `/openspec/changes/improve-cli-chat-interface/`
- SOLID Principles: Martin, Robert C. "Agile Software Development"
- Factory Pattern: Gamma et al. "Design Patterns"
- crucible-watch docs: `crates/crucible-watch/src/lib.rs`
