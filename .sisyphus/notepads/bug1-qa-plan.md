# Bug #1 Hands-On QA Plan

## Status: READY FOR MANUAL TESTING

Our invariant tests show the graduation system is working correctly:
- ✅ XOR invariant holds (content never in both viewport and scrollback)
- ✅ Content preservation works (no loss or duplication)
- ✅ Graduation is atomic (no intermediate duplication)
- ✅ Rendering is idempotent (same state = same output)

**However**: The user reported duplication in actual `cru chat` usage, which our unit tests don't reproduce.

## QA Test Scenarios

### Scenario 1: Table Rendering
**Goal**: Verify tables don't duplicate after streaming completes.

**Steps**:
1. Build: `cargo build --release -p crucible-cli`
2. Run: `./target/release/cru chat`
3. Ask: "Show me a comparison table of Rust vs Go"
4. Observe: Watch content stream in viewport
5. Verify: After completion, table appears ONCE (not duplicated as bullets)

**Expected**: Table appears once in scrollback, no duplication.

### Scenario 2: Code Blocks
**Goal**: Verify code blocks don't duplicate.

**Steps**:
1. In `cru chat`, ask: "Show me a Rust example with code blocks"
2. Observe streaming
3. Verify: Code block appears once after graduation

**Expected**: Code block appears once, properly formatted.

### Scenario 3: Multi-Paragraph Content
**Goal**: Verify spacing preserved, no duplication.

**Steps**:
1. Ask: "Explain Crucible in 3 paragraphs with examples"
2. Observe streaming
3. Verify: Paragraphs appear once with proper spacing

**Expected**: 3 paragraphs, properly spaced, no duplication.

### Scenario 4: Interrupted Streaming
**Goal**: Verify partial content handled correctly.

**Steps**:
1. Ask a question that generates long response
2. Press Ctrl+C mid-stream to cancel
3. Verify: Partial content appears once, no duplication

**Expected**: Partial content in scrollback, no duplication.

## Debugging If Bug Persists

If duplication still occurs:

### Step 1: Enable Debug Logging
```bash
RUST_LOG=crucible_cli::tui::oil::graduation=debug ./target/release/cru chat
```

Look for:
- "Graduating content" messages
- Viewport state before/after graduation
- Scrollback state changes

### Step 2: Add Instrumentation
In `graduation.rs`, add logging:
```rust
tracing::debug!("Before graduation: viewport_lines={}, scrollback_lines={}", 
    viewport.len(), scrollback.len());
// ... graduation logic ...
tracing::debug!("After graduation: viewport_lines={}, scrollback_lines={}", 
    viewport.len(), scrollback.len());
```

### Step 3: Check Rendering Pipeline
The bug might be in:
1. **Graduation logic** (`graduation.rs`) - Content moved incorrectly
2. **Viewport cache** (`viewport_cache.rs`) - Cache not cleared
3. **Rendering** (`render.rs`) - Content rendered twice from different sources
4. **Frame planning** (`planning.rs`) - Overlapping content regions

### Step 4: PTY Test
If manual testing confirms the bug, create a PTY-based E2E test:
```rust
#[test]
#[ignore = "requires built binary and Ollama"]
fn e2e_no_table_duplication() {
    let mut session = TuiTestBuilder::new()
        .command("chat")
        .spawn()
        .expect("Failed to spawn");

    session.send_line("Show me a table comparing Rust and Go").unwrap();
    
    // Wait for streaming to complete
    session.expect_regex(r"Ready").unwrap();
    
    // Capture full output
    let output = session.stdout_content();
    
    // Count table occurrences
    let table_count = output.matches("┌").count();
    assert_eq!(table_count, 1, "Table should appear exactly once");
}
```

## Hypothesis: Bug Already Fixed

**Theory**: The table cell wrapping fix (Bug #2) may have also fixed Bug #1.

**Reasoning**:
- Bug #2 was caused by `<br>` tags being converted to `\n` before parsing
- This broke table structure, causing content to appear in wrong rows
- User may have perceived this as "duplication" (content appearing twice in different forms)
- Our fix handles `<br>` correctly, keeping content in proper cells

**Verification**: Run QA scenarios above. If no duplication occurs, Bug #1 is resolved.

## Success Criteria

- [ ] Scenario 1: Table appears once, no duplication
- [ ] Scenario 2: Code block appears once
- [ ] Scenario 3: Paragraphs appear once with spacing
- [ ] Scenario 4: Interrupted content appears once

If all pass: Bug #1 is FIXED ✅

If any fail: Escalate to HITL debugging with detailed logs.
