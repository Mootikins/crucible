# Oil Module Refactoring Plan

## Goal
Refactor the `oil` TUI module to be completely domain-agnostic (no Crucible-specific logic), then add precise invariant testing for viewport+scrollback to catch the content duplication bug.

## Phase 1: Audit Domain Dependencies

**Task**: Identify all Crucible-specific logic in the oil module.

**What to look for**:
- Imports from `crucible_core`, `crucible_rig`, `crucible_daemon`
- Domain types: `Message`, `ToolCall`, `Notification`, `Session`
- Business logic: Message formatting, tool execution, session management
- File I/O: Reading/writing session files, kiln operations

**Expected findings**:
- `chat_app.rs` - Likely has domain logic for message handling
- `chat_runner.rs` - Likely has session/agent integration
- `components/` - Should be mostly pure UI, but may have domain coupling
- `markdown.rs` - Should be pure rendering, verify no domain types

**Output**: Document in `.sisyphus/notepads/oil-domain-audit.md`

---

## Phase 2: Extract Domain Logic

**Task**: Move Crucible-specific logic out of oil module into `crucible-cli` layer.

**Strategy**:
1. Create adapter layer in `crucible-cli/src/tui/adapters/`
2. Define pure interfaces in oil (traits for data providers)
3. Implement adapters that bridge oil ↔ crucible domain

**Example refactoring**:

**Before** (oil has domain logic):
```rust
// In oil/chat_app.rs
use crucible_core::types::Message;

impl InkChatApp {
    fn handle_message(&mut self, msg: Message) {
        // Domain logic mixed with UI
    }
}
```

**After** (oil is domain-agnostic):
```rust
// In oil/chat_app.rs
pub trait MessageProvider {
    fn get_messages(&self) -> Vec<DisplayMessage>;
}

impl InkChatApp {
    fn handle_message(&mut self, msg: DisplayMessage) {
        // Pure UI logic
    }
}

// In crucible-cli/src/tui/adapters/message_adapter.rs
impl MessageProvider for SessionAdapter {
    fn get_messages(&self) -> Vec<DisplayMessage> {
        self.session.messages()
            .map(|m| DisplayMessage::from_domain(m))
            .collect()
    }
}
```

**Files to refactor**:
- `chat_app.rs` - Extract message/session logic
- `chat_runner.rs` - Extract agent/daemon integration
- `components/notification_area.rs` - Already clean (uses generic `Notification`)

---

## Phase 3: Precise Invariant Testing

**Task**: Create comprehensive tests for viewport+scrollback invariants.

### Invariant 1: XOR Placement
**Rule**: Content appears in EITHER viewport OR scrollback, NEVER both.

**Test**:
```rust
#[test]
fn content_never_in_both_viewport_and_scrollback() {
    let mut app = InkChatApp::default();
    
    // Stream content
    for chunk in streaming_chunks() {
        app.on_message(ChatAppMsg::TextDelta(chunk));
        
        // Verify XOR invariant at every step
        let viewport_content = extract_viewport_content(&app);
        let scrollback_content = extract_scrollback_content(&app);
        
        for item in &viewport_content {
            assert!(
                !scrollback_content.contains(item),
                "Content '{}' appears in BOTH viewport and scrollback",
                item
            );
        }
    }
    
    // Complete streaming (triggers graduation)
    app.on_message(ChatAppMsg::StreamComplete);
    
    // Verify XOR invariant after graduation
    let viewport_content = extract_viewport_content(&app);
    let scrollback_content = extract_scrollback_content(&app);
    
    for item in &viewport_content {
        assert!(
            !scrollback_content.contains(item),
            "After graduation: Content '{}' appears in BOTH viewport and scrollback",
            item
        );
    }
}
```

### Invariant 2: Content Preservation
**Rule**: Total content (viewport + scrollback) equals all streamed content.

**Test**:
```rust
#[test]
fn graduation_preserves_all_content() {
    let mut app = InkChatApp::default();
    let mut all_streamed = String::new();
    
    for chunk in streaming_chunks() {
        all_streamed.push_str(&chunk);
        app.on_message(ChatAppMsg::TextDelta(chunk));
    }
    
    app.on_message(ChatAppMsg::StreamComplete);
    
    let viewport = extract_viewport_content(&app);
    let scrollback = extract_scrollback_content(&app);
    let total = format!("{}{}", scrollback, viewport);
    
    assert_eq!(
        normalize_whitespace(&total),
        normalize_whitespace(&all_streamed),
        "Content lost or duplicated during graduation"
    );
}
```

### Invariant 3: Graduation Atomicity
**Rule**: Graduation happens atomically - no intermediate state where content is in both.

**Test**:
```rust
#[test]
fn graduation_is_atomic() {
    let mut app = InkChatApp::default();
    
    // Stream content
    app.on_message(ChatAppMsg::TextDelta("Test content".to_string()));
    
    // Capture state before graduation
    let before_viewport = extract_viewport_content(&app);
    let before_scrollback = extract_scrollback_content(&app);
    
    // Trigger graduation
    app.on_message(ChatAppMsg::StreamComplete);
    
    // Capture state after graduation
    let after_viewport = extract_viewport_content(&app);
    let after_scrollback = extract_scrollback_content(&app);
    
    // Content should move atomically (not duplicated)
    let before_total = before_viewport.len() + before_scrollback.len();
    let after_total = after_viewport.len() + after_scrollback.len();
    
    assert_eq!(
        before_total, after_total,
        "Content count changed during graduation (duplication or loss)"
    );
}
```

### Invariant 4: Rendering Idempotence
**Rule**: Rendering the same state multiple times produces identical output.

**Test**:
```rust
#[test]
fn rendering_is_idempotent() {
    let mut app = InkChatApp::default();
    
    app.on_message(ChatAppMsg::TextDelta("Content".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);
    
    let render1 = render_app(&app);
    let render2 = render_app(&app);
    let render3 = render_app(&app);
    
    assert_eq!(render1, render2, "First and second render differ");
    assert_eq!(render2, render3, "Second and third render differ");
}
```

---

## Phase 4: Property-Based Testing

**Task**: Use `proptest` to generate random streaming patterns and verify invariants hold.

**Test**:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn graduation_xor_invariant_holds_for_any_content(
        chunks in prop::collection::vec(any::<String>(), 1..100)
    ) {
        let mut app = InkChatApp::default();
        
        for chunk in chunks {
            app.on_message(ChatAppMsg::TextDelta(chunk));
        }
        
        app.on_message(ChatAppMsg::StreamComplete);
        
        // Verify XOR invariant
        let viewport = extract_viewport_content(&app);
        let scrollback = extract_scrollback_content(&app);
        
        for item in &viewport {
            prop_assert!(
                !scrollback.contains(item),
                "XOR invariant violated: '{}' in both viewport and scrollback",
                item
            );
        }
    }
}
```

---

## Phase 5: Hands-On QA

**Task**: Run `cru chat` with real LLM and verify Bug #1 is fixed.

**Test scenarios**:
1. Stream a message with a table → verify no duplication after completion
2. Stream multiple paragraphs → verify spacing preserved
3. Stream code blocks → verify formatting correct
4. Interrupt streaming mid-way → verify partial content handled correctly

**How to test**:
```bash
# Build with debug logging
RUST_LOG=crucible_cli::tui::oil::graduation=debug cargo build

# Run chat
./target/debug/cru chat

# In chat, ask for content with tables:
> "Show me a table comparing Rust and Go"

# Observe:
# 1. Content streams in viewport
# 2. When complete, content graduates to scrollback
# 3. Verify table appears ONCE (not duplicated)
```

**If bug persists**: Escalate to HITL (human-in-the-loop) debugging with detailed logs.

---

## Success Criteria

- [ ] Oil module has ZERO imports from `crucible_core`, `crucible_rig`, `crucible_daemon`
- [ ] All domain logic moved to adapter layer in `crucible-cli`
- [ ] 4 invariant tests added and passing
- [ ] Property-based tests added and passing (100+ random cases)
- [ ] Hands-on QA confirms Bug #1 is fixed
- [ ] All existing tests still pass (no regressions)

---

## Timeline Estimate

- Phase 1 (Audit): 1-2 hours
- Phase 2 (Extract): 4-6 hours
- Phase 3 (Invariant tests): 2-3 hours
- Phase 4 (Property tests): 1-2 hours
- Phase 5 (QA): 30 minutes

**Total**: ~8-14 hours of focused work
