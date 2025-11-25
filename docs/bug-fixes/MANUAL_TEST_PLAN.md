# Manual Test Plan - Process Command Pipeline

**Date:** 2025-11-24
**Purpose:** Verify process and chat commands produce identical results and all new features work correctly
**Estimated Time:** 1.5 hours

## Prerequisites

### Build the Project
```bash
cd /home/moot/crucible-fix-process-pipeline
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### Prepare Test Environment
```bash
# Create test kiln with sample files
TEST_KILN=/tmp/test-kiln-$(date +%s)
mkdir -p "$TEST_KILN"

# Create sample markdown files
cat > "$TEST_KILN/note1.md" <<'EOF'
---
tags: [test, sample]
created: 2025-11-24
---

# Note 1

This is a test note with a [[wikilink]] and some content.

## Section 1
Some content here.
EOF

cat > "$TEST_KILN/note2.md" <<'EOF'
---
tags: [test]
---

# Note 2

Another note linking to [[note1]].
EOF

cat > "$TEST_KILN/note3.md" <<'EOF'
# Note 3 (No Frontmatter)

Just plain content.
EOF

echo "Test kiln created at: $TEST_KILN"
```

### Set Environment Variables
```bash
export CRUCIBLE_KILN_PATH="$TEST_KILN"
export CRUCIBLE_DB_PATH="/tmp/crucible-test-$(date +%s).db"
```

---

## Test Suite 1: Process Command Basic Functionality

### Test 1.1: Basic Process Command
**Objective:** Verify process command runs and processes files

```bash
# Clean database
rm -f "$CRUCIBLE_DB_PATH"

# Run process command
cru process

# Expected output:
# ✓ Storage initialized
# ✓ Pipeline ready
# 🔄 Processing 3 files through pipeline...
# [progress bar]
# ✅ Pipeline processing complete!
#    Processed: 3 files
#    Skipped (unchanged): 0 files
```

**Verification:**
- [ ] All 3 files processed
- [ ] No errors displayed
- [ ] Database created at `$CRUCIBLE_DB_PATH`

### Test 1.2: Process Command Idempotency
**Objective:** Verify change detection skips unchanged files

```bash
# Run process again without modifying files
cru process

# Expected output:
# ✓ Storage initialized
# ✓ Pipeline ready
# 🔄 Processing 3 files through pipeline...
# ✅ Pipeline processing complete!
#    Processed: 0 files
#    Skipped (unchanged): 3 files
```

**Verification:**
- [ ] 0 files processed
- [ ] 3 files skipped
- [ ] Processing was fast (change detection working)

### Test 1.3: Force Flag
**Objective:** Verify --force bypasses change detection

```bash
# Run with --force flag
cru process --force

# Expected output:
#    Processed: 3 files
#    Skipped (unchanged): 0 files
```

**Verification:**
- [ ] All files reprocessed despite no changes
- [ ] No skipped files

---

## Test Suite 2: Verbose Flag

### Test 2.1: Verbose Output
**Objective:** Verify --verbose shows detailed processing info

```bash
# Modify a file
echo "\n## New Section\nNew content" >> "$TEST_KILN/note1.md"

# Run with verbose
cru process --verbose

# Expected output includes:
# 📄 Processing: note1.md
#    ✓ Success
# 📄 Processing: note2.md
#    ⏭ Skipped (unchanged)
# 📄 Processing: note3.md
#    ⏭ Skipped (unchanged)
```

**Verification:**
- [ ] Shows "📄 Processing:" for each file
- [ ] Shows result status (✓ Success, ⏭ Skipped)
- [ ] note1.md marked as Success
- [ ] note2.md and note3.md marked as Skipped

### Test 2.2: Verbose Without Flag
**Objective:** Verify quiet mode is default

```bash
# Run without verbose
cru process

# Expected: Just progress bar and summary, no per-file details
```

**Verification:**
- [ ] No "📄 Processing:" messages
- [ ] Only progress bar visible
- [ ] Summary shown at end

---

## Test Suite 3: Dry-Run Flag

### Test 3.1: Dry-Run Preview
**Objective:** Verify --dry-run shows what would be processed

```bash
# Modify all files
echo "\nChanged" >> "$TEST_KILN/note1.md"
echo "\nChanged" >> "$TEST_KILN/note2.md"
echo "\nChanged" >> "$TEST_KILN/note3.md"

# Run dry-run
cru process --dry-run

# Expected output:
# 🔍 DRY RUN MODE - No changes will be made
# [progress bar]
#   Would process: note1.md
#   Would process: note2.md
#   Would process: note3.md
# ✅ Dry-run complete!
#    Would have processed: 3 files
```

**Verification:**
- [ ] Shows "🔍 DRY RUN MODE" message
- [ ] Lists all files that would be processed
- [ ] Database NOT modified (check with stats command)

### Test 3.2: Dry-Run Doesn't Modify Database
**Objective:** Verify dry-run has no side effects

```bash
# Get file count before dry-run
BEFORE=$(cru stats | grep "Total notes" | awk '{print $3}')

# Run dry-run
cru process --dry-run --force

# Get file count after dry-run
AFTER=$(cru stats | grep "Total notes" | awk '{print $3}')

# Compare
if [ "$BEFORE" = "$AFTER" ]; then
    echo "✅ Database unchanged (correct)"
else
    echo "❌ Database was modified (BUG!)"
fi
```

**Verification:**
- [ ] Database unchanged after dry-run
- [ ] File hashes not updated

### Test 3.3: Dry-Run with Verbose
**Objective:** Verify flags combine correctly

```bash
cru process --dry-run --verbose

# Expected:
# 📄 Processing: note1.md
#    ⏭ Would process (dry-run)
```

**Verification:**
- [ ] Shows verbose output
- [ ] Shows dry-run indicators
- [ ] No actual processing

---

## Test Suite 4: Watch Mode (Process Command)

### Test 4.1: Watch Mode Initialization
**Objective:** Verify watch mode starts successfully

```bash
# Start watch mode in background
cru process --watch --verbose &
WATCH_PID=$!

sleep 2  # Let it initialize

# Check if running
if kill -0 $WATCH_PID 2>/dev/null; then
    echo "✅ Watch mode running"
else
    echo "❌ Watch mode crashed"
fi

# Expected output:
# ✓ Storage initialized
# ✓ Pipeline ready
# 🔄 Processing 3 files through pipeline...
# ✅ Pipeline processing complete!
# 👀 Watching for changes (Press Ctrl+C to stop)...
```

**Verification:**
- [ ] Watch mode starts without errors
- [ ] Initial processing completes
- [ ] Shows "Watching for changes" message

### Test 4.2: Watch Mode Detects Changes
**Objective:** Verify watch mode reprocesses modified files

```bash
# Modify a file while watch is running
echo "\n## Watch Test\nTesting watch mode" >> "$TEST_KILN/note1.md"

# Wait for debounce (500ms) + processing
sleep 2

# Expected output in watch terminal:
# 📝 Change detected: /tmp/test-kiln-*/note1.md
#    ✓ Reprocessed successfully
```

**Verification:**
- [ ] Change detected and logged
- [ ] File reprocessed successfully
- [ ] Watch continues running

### Test 4.3: Watch Mode Debouncing
**Objective:** Verify rapid changes are debounced

```bash
# Make rapid successive changes
for i in {1..5}; do
    echo "Change $i" >> "$TEST_KILN/note1.md"
    sleep 0.1  # 100ms between changes
done

# Wait for debounce + processing
sleep 2

# Expected: Single reprocess (not 5)
# Count "Reprocessed successfully" messages - should be 1
```

**Verification:**
- [ ] Only single reprocess despite 5 changes
- [ ] Debouncing working correctly

### Test 4.4: Watch Mode Handles New Files
**Objective:** Verify watch detects new file creation

```bash
# Create new file
cat > "$TEST_KILN/note4.md" <<'EOF'
# New Note

Created while watch is running.
EOF

sleep 2

# Expected:
# 📝 Change detected: /tmp/test-kiln-*/note4.md
#    ✓ Reprocessed successfully
```

**Verification:**
- [ ] New file detected
- [ ] New file processed
- [ ] Added to database

### Test 4.5: Watch Mode Graceful Shutdown
**Objective:** Verify Ctrl+C stops watch cleanly

```bash
# Send interrupt signal
kill -INT $WATCH_PID

sleep 1

# Expected output:
# 👋 Stopping watch mode...
# ✅ Watch mode stopped

# Verify process stopped
if ! kill -0 $WATCH_PID 2>/dev/null; then
    echo "✅ Watch mode stopped cleanly"
else
    echo "❌ Watch mode still running"
    kill -9 $WATCH_PID
fi
```

**Verification:**
- [ ] Shows "Stopping watch mode" message
- [ ] Process terminates cleanly
- [ ] No zombie processes

---

## Test Suite 5: Chat Command vs Process Command Consistency

### Test 5.1: Database State Comparison
**Objective:** Verify chat and process produce identical database state

```bash
# Test with process command
rm -f "$CRUCIBLE_DB_PATH"
cru process
PROCESS_HASH=$(find "$CRUCIBLE_DB_PATH" -type f -exec md5sum {} \; | sort | md5sum)

# Test with chat command (let background watch run)
rm -f "$CRUCIBLE_DB_PATH"
# Run chat command, immediately exit
echo "/exit" | cru chat
sleep 3  # Wait for background processing
CHAT_HASH=$(find "$CRUCIBLE_DB_PATH" -type f -exec md5sum {} \; | sort | md5sum)

# Compare
if [ "$PROCESS_HASH" = "$CHAT_HASH" ]; then
    echo "✅ Databases are identical"
else
    echo "⚠️  Databases differ - investigating..."

    # Get detailed stats
    rm -f "$CRUCIBLE_DB_PATH"
    cru process
    PROCESS_STATS=$(cru stats)

    rm -f "$CRUCIBLE_DB_PATH"
    echo "/exit" | cru chat
    sleep 3
    CHAT_STATS=$(cru stats)

    echo "=== Process Stats ==="
    echo "$PROCESS_STATS"
    echo ""
    echo "=== Chat Stats ==="
    echo "$CHAT_STATS"
fi
```

**Verification:**
- [ ] Same number of notes indexed
- [ ] Same number of blocks stored
- [ ] Same number of links extracted
- [ ] Same number of tags indexed

### Test 5.2: Search Results Comparison
**Objective:** Verify search returns same results from both commands

```bash
# Index with process
rm -f "$CRUCIBLE_DB_PATH"
cru process

# Search
PROCESS_RESULTS=$(cru search "test" --limit 10 | md5sum)

# Index with chat
rm -f "$CRUCIBLE_DB_PATH"
echo "/exit" | cru chat
sleep 3

# Same search
CHAT_RESULTS=$(cru search "test" --limit 10 | md5sum)

if [ "$PROCESS_RESULTS" = "$CHAT_RESULTS" ]; then
    echo "✅ Search results identical"
else
    echo "⚠️  Search results differ"
fi
```

**Verification:**
- [ ] Identical search results
- [ ] Same ranking/ordering
- [ ] Same content returned

---

## Test Suite 6: Background Watch in Chat

### Test 6.1: Background Watch Starts
**Objective:** Verify background watch spawns during chat

```bash
# Enable debug logging
export RUST_LOG=debug

# Start chat in background
cru chat &
CHAT_PID=$!

sleep 3  # Let it initialize

# Check logs for background watch
if grep -q "Background watch spawned for chat mode" ~/.crucible/chat.log; then
    echo "✅ Background watch spawned"
else
    echo "❌ Background watch not found"
fi

kill $CHAT_PID
```

**Verification:**
- [ ] Log shows "Background watch spawned"
- [ ] No errors in log
- [ ] Chat command continues normally

### Test 6.2: Background Watch Reprocesses Files
**Objective:** Verify background watch detects and processes changes during chat

```bash
# Start chat
cru chat &
CHAT_PID=$!
sleep 3

# Modify file
echo "\n## Chat Test\nModified during chat" >> "$TEST_KILN/note1.md"
sleep 2

# Check logs for reprocessing
if grep -q "Background watch detected change.*note1.md" ~/.crucible/chat.log && \
   grep -q "Background reprocessed.*note1.md" ~/.crucible/chat.log; then
    echo "✅ Background watch reprocessed file"
else
    echo "❌ File not reprocessed"
fi

kill $CHAT_PID
```

**Verification:**
- [ ] Log shows change detection
- [ ] Log shows successful reprocessing
- [ ] Chat remains responsive

### Test 6.3: Background Watch Silent Operation
**Objective:** Verify background watch doesn't pollute stdout/stderr

```bash
# Start chat with output capture
rm -f /tmp/chat-output.txt
cru chat > /tmp/chat-output.txt 2>&1 &
CHAT_PID=$!
sleep 3

# Modify file
echo "\nChange" >> "$TEST_KILN/note1.md"
sleep 2

# Check captured output
if grep -q "Background watch\|Reprocessed" /tmp/chat-output.txt; then
    echo "❌ Background watch polluted output"
else
    echo "✅ Silent operation confirmed"
fi

kill $CHAT_PID
```

**Verification:**
- [ ] No background watch messages in stdout
- [ ] No background watch messages in stderr
- [ ] Only chat output visible

---

## Test Suite 7: Error Handling

### Test 7.1: Invalid File Processing
**Objective:** Verify graceful handling of invalid markdown

```bash
# Create invalid file
echo "<<<INVALID MARKDOWN>>>" > "$TEST_KILN/invalid.md"

# Process with verbose
cru process --verbose 2>&1 | tee /tmp/error-test.txt

# Check for error handling
if grep -q "Error processing.*invalid.md" /tmp/error-test.txt && \
   ! grep -q "panic\|thread.*panicked" /tmp/error-test.txt; then
    echo "✅ Error handled gracefully"
else
    echo "❌ Error not handled properly"
fi
```

**Verification:**
- [ ] Error logged for invalid file
- [ ] No panic/crash
- [ ] Processing continues for other files

### Test 7.2: Permission Errors
**Objective:** Verify handling of unreadable files

```bash
# Create unreadable file
touch "$TEST_KILN/noperm.md"
chmod 000 "$TEST_KILN/noperm.md"

# Process
cru process --verbose 2>&1 | tee /tmp/perm-test.txt

# Restore permissions
chmod 644 "$TEST_KILN/noperm.md"

# Check handling
if grep -qi "permission\|error" /tmp/perm-test.txt; then
    echo "✅ Permission error handled"
else
    echo "⚠️  No permission error detected"
fi
```

**Verification:**
- [ ] Permission error logged
- [ ] No crash
- [ ] Other files still processed

---

## Test Suite 8: Performance & Scalability

### Test 8.1: Large Kiln Processing
**Objective:** Verify performance with many files

```bash
# Create 100 test files
for i in {1..100}; do
    cat > "$TEST_KILN/gen-$i.md" <<EOF
# Generated Note $i

Content for note $i with [[gen-$(( (i % 100) + 1 ))]].
EOF
done

# Time processing
time cru process

# Check stats
cru stats
```

**Verification:**
- [ ] All 103 files processed (3 original + 100 generated)
- [ ] Processing completes in reasonable time (<30s)
- [ ] No memory issues

### Test 8.2: Change Detection Performance
**Objective:** Verify change detection is efficient

```bash
# Run without changes (should be fast)
time cru process

# Expected: <1 second (all files skipped)
```

**Verification:**
- [ ] All files skipped
- [ ] Completes in <1 second
- [ ] Change detection efficient

---

## Results Template

### Summary

| Test Suite | Tests | Pass | Fail | Notes |
|------------|-------|------|------|-------|
| 1. Basic Functionality | 3 | | | |
| 2. Verbose Flag | 2 | | | |
| 3. Dry-Run Flag | 3 | | | |
| 4. Watch Mode | 5 | | | |
| 5. Chat vs Process | 2 | | | |
| 6. Background Watch | 3 | | | |
| 7. Error Handling | 2 | | | |
| 8. Performance | 2 | | | |
| **Total** | **22** | | | |

### Critical Issues
- [ ] None found
- [ ] List any issues here

### Non-Critical Issues
- [ ] None found
- [ ] List any issues here

### Overall Assessment
- [ ] ✅ PASS - Ready for merge
- [ ] ⚠️  PASS WITH ISSUES - Address before merge
- [ ] ❌ FAIL - Major issues found

### Recommendations
1.
2.
3.

---

## Cleanup

```bash
# Remove test data
rm -rf "$TEST_KILN"
rm -f "$CRUCIBLE_DB_PATH"
rm -f /tmp/chat-output.txt /tmp/error-test.txt /tmp/perm-test.txt

echo "Cleanup complete"
```

---

**Tester:** _______________
**Date:** _______________
**Time Taken:** _______________
