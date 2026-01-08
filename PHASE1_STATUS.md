# Phase 1 Status: Incomplete Unification

## Current State

TuiState and ViewState are **separate structures**, not in a parent-child relationship as specified in the plan skeleton.

### What Exists Now

**TuiState** (runner-level concerns):
- streaming, mode_id, pending_tools
- should_exit, ctrl_c_count, status_error
- notifications, pending_context, show_reasoning
- **DEPRECATED:** input_buffer, cursor_position, has_popup (added by Phase 1 agent)

**ViewState** (UI-level concerns - defined elsewhere):
- input_buffer, cursor_position
- popup, dialog_stack, conversation
- scroll_offset, width, height

### What the Plan Specified

From `TUI_ARCHITECTURE_PLAN.md` lines 62-95:

```rust
pub struct TuiState {
    /// ViewState owns ALL view-related fields
    pub view: ViewState,  // <-- CONTAINMENT relationship
    /// TuiState owns non-view concerns only
    pub streaming: StreamingState,
    pub history: CommandHistory,
    pub mode_id: String,
}
```

### What Phase 1 Agent Did

1. ✅ Deleted `sync_input_to_view()` from harness.rs
2. ✅ Removed 13 call sites of the sync method
3. ⚠️ Added deprecated fields to TuiState as "transitional" measure
4. ⚠️ Did NOT implement the containment relationship from the plan

### Why Tests Are Failing

Tests create `TuiState` directly and call methods like:
```rust
let mut s = TuiState::new("plan");
s.input_buffer = "hello world";  // Writes to deprecated field
s.delete_word_backward();        // Operates on... where?
assert_eq!(s.input_buffer, "hello ");  // Reads deprecated field
```

The problem:
1. Tests write to `TuiState.input_buffer` (deprecated field)
2. Methods operate on... unclear (TuiState or ViewState?)
3. Tests read from `TuiState.input_buffer` (deprecated field)
4. No synchronization happens between the two structures

## Path Forward

### Option A: Complete Unification (As Per Plan)
Make TuiState contain ViewState:

```rust
pub struct TuiState {
    pub view: ViewState,  // Add this field
    pub streaming: Option<StreamingBuffer>,
    pub mode_id: String,
    // ... other non-view fields
    // REMOVE deprecated fields
}
```

Add accessor methods:
```rust
impl TuiState {
    pub fn input(&self) -> &str { &self.view.state().input_buffer }
    pub fn input_mut(&mut self) -> &mut String { &mut self.view.state_mut().input_buffer }
    pub fn cursor(&self) -> usize { self.view.state().cursor_position }
    pub fn set_cursor(&mut self, pos: usize) { self.view.state_mut().cursor_position = pos; }
    pub fn has_popup(&self) -> bool { self.view.popup.is_some() }
}
```

Then update all code to use accessors instead of direct field access.

**Effort:** Medium-High (affects many files)
**Benefit:** Matches plan spec, clean architecture

### Option B: Keep Separate, Add Sync
Keep TuiState and ViewState separate, but implement proper synchronization:

```rust
impl TuiState {
    pub fn input(&self) -> &str {
        // Delegate to ViewState wherever it's stored
        // (need to figure out where ViewState lives in the runner)
    }
}
```

**Effort:** High (need to understand ViewState lifecycle)
**Benefit:** Less disruptive to existing structure

### Option C: Accept Transitional State
Leave deprecated fields in place for now, document as technical debt, continue with other phases.

**Effort:** Low
**Benefit:** Unblocks other work
**Drawback:** 50 failing tests remain, deprecation warnings persist

## Recommendation

**Complete Option A** - Implement the containment relationship as specified in the plan. This is what Phase 1 was supposed to accomplish, and it will resolve the test failures naturally.

## Estimated Work

1. Add `view: ViewState` field to TuiState: 30 min
2. Implement accessor methods: 30 min
3. Update all direct field access to use accessors: 2-3 hours
4. Update tests: 1 hour
5. Run full test suite: 30 min

**Total:** 4-5 hours

## Next Steps

1. Decide which option to pursue
2. Implement chosen option
3. Verify all tests pass
4. Mark Phase 1 as COMPLETE
5. Proceed to Phase 3 (which depends on Phase 1)
