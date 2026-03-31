---
date: 2026-03-31T14:00:00-05:00
feature: TUI Rendering Bugs - Part 4 (Test infra overhaul + reproduction)
status: in_progress
---

# Handoff: Test Infrastructure Fixes + Spinner Bug Reproduction

## Summary

Overhauled the TUI test infrastructure to eliminate divergence between
test and production render paths. Successfully reproduced the spinner-in-
scrollback bug in a unit test using the exact event sequence that triggers it:
**permission modals**.

## What Was Done This Session

### Commits on master (8 commits)

```
952f5f4e fix(tui): wrap graduation cycle in unified sync update, normalize cursor
64a564d6 test(tui): add scrollback inspection, sync boundary verification, property tests
3012d09a fix(tui): position cursor from viewport bottom, not relative to last
e7fd8908 test(tui): incremental byte feeding in vt100, reproduction test
958b6458 chore: add graduation diagnostic trace for spinner-in-scrollback repro
f5806019 test(tui): tall vt100 parser for scrollback, small terminal repro test
83dbae8d refactor(oil): TestRuntime delegates to Terminal<Vec<u8>> via FrameRenderer
c923ad6e test(tui): reproduce spinner-in-scrollback via permission modal events
```

### Key Changes

#### 1. TestRuntime now delegates to real Terminal

`TestRuntime` previously reimplemented parts of the graduation rendering
path. Now it delegates entirely to `Terminal<Vec<u8>>` via `FrameRenderer`.
`FrameRenderer` is implemented for `Terminal<W: Write>` (all writers),
not just `Terminal<Stdout>`. Tests use the exact same `apply()` path as
production.

**Files:**
- `crates/crucible-oil/src/runtime.rs` — TestRuntime delegates to Terminal
- `crates/crucible-oil/src/terminal.rs` — `impl<W: Write> FrameRenderer for Terminal<W>`

#### 2. Vt100TestRuntime with tall parser

`Vt100TestRuntime` now maintains TWO vt100 parsers fed the same bytes:
- **Normal parser** (user's terminal size, 1000 scrollback) — models real behavior
- **Tall parser** (1000 rows, 0 scrollback) — captures full history since nothing scrolls off

The tall parser is the key to detecting spinners in scrollback: since
its terminal has 1000 rows, content that would scroll into scrollback on a
normal terminal instead stays in the visible area. `full_history()` returns
everything the terminal has ever displayed.

**Why not vt100 set_scrollback()?** The vt100 crate has an overflow bug
in `Grid::visible_rows()` when `scrollback_offset > rows_len` (small
terminals with lots of scrollback). The tall parser avoids this entirely.

**File:** `crates/crucible-cli/src/tui/oil/tests/vt100_runtime.rs`

#### 3. Incremental byte feeding

`Vt100TestRuntime` feeds bytes to vt100 respecting synchronized update
boundaries: content inside `\x1b[?2026h`..`\x1b[?2026l` blocks is fed
atomically (modeling real terminal buffering), content outside is fed
byte-by-byte (modeling incremental processing).

#### 4. apply() changes (attempted fix — incomplete)

- Moved `BEGIN_SYNCHRONIZED_UPDATE`/`END_SYNCHRONIZED_UPDATE` from
  `render_with_overlays()` to `apply()`, wrapping the entire graduation cycle
- Normalized cursor to viewport bottom before `clear()`, eliminating
  `cursor_offset_from_end` as a parameter
- Simplified `position_cursor()` to always move from bottom

**These changes did NOT fix the spinner leak.** The reproduction test
still fails with the current code. The sync update and cursor changes are
structurally good but don't address the root cause.

## The Reproduction (FAILING TEST)

```
reproduce_permission_modal_spinner_leak
```

**Location:** `crates/crucible-cli/src/tui/oil/tests/vt100_runtime.rs`

**Event sequence that triggers the bug:**
1. `UserMessage("tell me about this repo")`
2. `ThinkingDelta("I'll explore...")` + render frames
3. `tool("get_kiln_info", "c1")` (no permission) + render frames
4. **`OpenInteraction(Permission(bash ls -la))`** ← THE TRIGGER
5. Render frame → thinking graduates, turn spinner appears in viewport
6. `ToolCall("bash", "c2")` + `ToolResultComplete` + render
7. **Check: spinner `◐` found at line 9 between Get Kiln Info and Bash**

The `OpenInteraction(Permission)` causes `permission_pending = true` on
the container list, which makes the trailing `AssistantResponse` graduatable.
On the next render, `drain_completed()` graduates the thinking + tool group.
The viewport then shows the turn spinner (via `needs_turn_spinner()`).
The spinner ends up in the tall parser's history between graduated content.

**The spinner is NOT in the graduation content (stdout_delta).** It's
in the viewport that gets rendered on the same frame. The mechanism by which
it enters scrollback is still unclear — the `apply()` bytes are
sequentially: clear → graduation → viewport render. The clear should erase
the old viewport. But the tall parser (different geometry) sees it differently.

## What's NOT Fixed

The spinner-in-scrollback bug. The reproduction test fails. The root cause
is still in how the viewport content (containing the turn spinner) interacts
with the graduation cycle in `apply()`.

## Open Questions

1. **Tall parser fidelity**: The tall parser (1000 rows) processes the same
   bytes as the normal parser, but cursor movements produce different screen
   states due to different geometry. Is the tall parser accurately modeling
   what a real terminal's scrollback would contain? The cast replay test
   (`scrollback_spinner_test.rs`) confirms the same spinners appear in both
   the tall parser and the real terminal, so the answer is likely yes.

2. **Where exactly does the spinner enter the byte stream?** The graduation
   content (stdout_delta) is confirmed clean (no spinner chars). The spinner
   is in the viewport render. But `apply()` writes: clear → graduation →
   viewport. The old viewport (with spinner) should be erased by clear. Need
   to instrument `apply()` to dump bytes per step and feed each step to vt100
   separately to find the exact moment the spinner enters the tall parser's
   visible area.

3. **Is the issue in clear() or in viewport rendering?** If clear() doesn't
   fully erase the old viewport on the tall parser (because cursor math differs
   at 1000 rows), the old spinner row survives. This would mean the tall parser
   is exposing a real clear() bug that also affects real terminals.

## Key Files

| File | What |
|------|------|
| `crates/crucible-oil/src/terminal.rs` | `apply()` — sync update, cursor normalization, graduation writes |
| `crates/crucible-oil/src/output.rs` | `clear()` simplified, `render_with_overlays()` without sync markers |
| `crates/crucible-oil/src/runtime.rs` | `TestRuntime` delegates to `Terminal<Vec<u8>>`, `FrameRenderer` trait |
| `crates/crucible-cli/src/tui/oil/tests/vt100_runtime.rs` | `Vt100TestRuntime` with tall parser, reproduction test |
| `crates/crucible-cli/tests/scrollback_spinner_test.rs` | Cast replay test (also reproduces via tall parser) |
| `crates/crucible-oil/tests/graduation_properties.rs` | Property tests for graduation rendering |
| `crates/crucible-cli/src/tui/oil/chat_container.rs:826` | `is_response_complete()` — `permission_pending` flag |
| `crates/crucible-cli/src/tui/oil/chat_container.rs:851` | `needs_turn_spinner()` — viewport spinner logic |

## Test Infrastructure Summary

### Vt100TestRuntime

```
render_frame(app)
  → render_frame(app, TestRuntime, focus)  // real Terminal<Vec<u8>> path
  → take_bytes()                           // raw escape sequences
  → feed_bytes_respecting_sync(bytes)      // incremental + atomic
      → tall_vt.process(bytes)             // 1000-row parser
      → vt.process(chunks)                 // normal parser, sync-aware
```

**Key methods:**
- `full_history()` — tall parser contents (everything ever displayed)
- `scrollback_contents()` — tall minus normal = what scrolled off
- `assert_no_spinners_in_scrollback()` — canonical spinner chars from node.rs
- `screen_contents()` — normal parser visible screen
- `last_frame_bytes()` — raw bytes from last render

### Property tests

`crates/crucible-oil/tests/graduation_properties.rs`:
- Graduation content never contains spinner chars
- Sync markers balanced per frame
- Sync markers balanced across sequences

## Next Steps

1. **Instrument apply() per-step**: Feed bytes to a vt100 parser after each
   write in apply() (after clear, after graduation, after viewport render)
   to find exactly which step introduces the spinner to the tall parser.

2. **Compare tall vs normal parser state**: After the failing frame, dump
   both parsers' screen contents to see how they diverge. This reveals
   whether the tall parser is accurately modeling scrollback or if its
   different geometry causes false positives.

3. **Fix the root cause**: Once we know which step leaks the spinner,
   fix the byte output. The test `reproduce_permission_modal_spinner_leak`
   will verify the fix.

## Resume

```bash
cd /home/moot/crucible
# Failing test:
cargo test -p crucible-cli --lib -- reproduce_permission_modal_spinner_leak
# All tests (2549 pass, the repro test will fail):
cargo nextest run --profile ci -p crucible-oil -p crucible-cli
# Cast replay (also fails, needs reproduce.cast):
cargo test -p crucible-cli --test scrollback_spinner_test -- --ignored
```
