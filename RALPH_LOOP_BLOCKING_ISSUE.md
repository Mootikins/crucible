# Ralph Loop Blocking Issue - Type Name Conflicts

**Date:** 2026-01-08
**Status:** BLOCKED on Sprint 2.1
**Issue:** Type name conflicts between old and new module locations

---

## Problem

When extracting types from `state.rs` to `state/types/`, we created a situation where both locations have types with the same names:
- `crate::tui::state::PopupKind` (old)
- `crate::tui::state::types::popup::PopupKind` (new)

This causes compilation errors because the compiler can't disambiguate which type is being referenced when code uses just `PopupKind`.

## What We Did

✅ Created new modules:
- `state/types/popup.rs` - Complete PopupItem, PopupKind definitions
- `state/types/context.rs` - Complete ContextAttachment, ContextKind definitions
- `state/types/mod.rs` - Module exports

✅ Added trait implementation to popup.rs:
- `impl crate::tui::widgets::PopupItem for PopupItem`

❌ Attempted to update imports but hit type conflicts

## Solution Approaches

### Option A: Use `pub use` to Re-export (RECOMMENDED)

In `state.rs`, replace the old definitions with re-exports:

```rust
// Remove old enum definitions (lines 62-156)
// Replace with:
pub use self::types::popup::{PopupItem, PopupItemKind, PopupKind};
pub use self::types::context::{ContextAttachment, ContextKind};
```

This makes `crate::tui::state::PopupItem` work as an alias to `crate::tui::state::types::popup::PopupItem`, maintaining backward compatibility.

### Option B: Mass Renaming

Rename all references to use the new path:
- Find: `use crate::tui::state::{PopupItem, ...}`
- Replace: `use crate::tui::state::types::{PopupItem, ...}`
- Then remove old definitions from state.rs

This is more invasive but clearer.

### Option C: Deprecation Strategy

Keep both temporarily, mark old ones as deprecated:
```rust
#[deprecated(since = "0.1.0", note = "Use crate::tui::state::types::popup::PopupItem instead")]
pub enum PopupItem { ... }
```

Then migrate incrementally.

## Recommendation

**Use Option A** (pub use re-exports):

**Steps:**
1. In `state.rs`, remove lines 62-108 (old enum definitions)
2. In `state.rs`, remove lines 420-454 (PopupItemKind)
3. In `state.rs`, remove lines 536-595 (trait impl)
4. Add at top of state.rs after imports:
   ```rust
   // Re-export types from types/ submodules
   pub use self::types::popup::*;
   pub use self::types::context::*;
   ```
5. Add `pub mod types;` declaration
6. Run tests
7. Fix any remaining compilation errors

**Files Affected:**
- `state.rs` - Remove ~400 lines, add re-exports
- 10+ files already updated with new imports (stashed)

**Estimated Time:** 15-20 minutes

---

## Files Modified (Stashed)

- popup.rs
- components/generic_popup.rs
- conversation_view.rs
- registries/*.rs
- runner.rs
- testing/harness.rs
- testing/popup_snapshot_tests.rs
- state.rs
- state/types/popup.rs

---

## Next Session Actions

1. **Restore stash**: `git stash pop`
2. **Implement Option A**: Use pub use re-exports in state.rs
3. **Remove old definitions**: Delete old enum/impl blocks
4. **Test**: `cargo test -p crucible-cli --lib`
5. **Commit**: Task 2.1 completion
6. **Continue to Task 2.2**: Extract action handlers

---

## Git Commands

```bash
# To restore work:
git stash pop

# To check what's stashed:
git stash show -p | head -100

# To see all commits:
git log --oneline | head -20
```

**Current commit:** 71e428c3 "docs: update session summary with Sprint 2.1 progress"

---

## Success Criteria

When Task 2.1 is complete:
- ✅ No old type definitions in state.rs (lines 62-156, 420-454, 536-595 removed)
- ✅ Re-exports in place: `pub use self::types::*;`
- ✅ All imports working (either old or new paths)
- ✅ All 1288+ tests passing
- ✅ state.rs reduced by ~400 lines
- ✅ Git commit documenting completion
