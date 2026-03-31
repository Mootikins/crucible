---
date: 2026-03-31T09:00:00-05:00
feature: TUI Rendering Bugs - Part 3 (Spinner in scrollback)
status: in_progress
---

# Handoff: Spinner in Scrollback After Permission Modal

## Summary

Spinner characters (◑, ◐, ◒) leak from the live viewport into terminal scrollback
during the graduation cycle, specifically when permission modals trigger rapid
sequential graduations. This did NOT happen before the Graduation type refactor.

## What Was Done This Session

### Commits on master (14 commits)
```
fb5e61e7 refactor(tui): replace stdout_delta String with Graduation node type
e1482045 merge: vt100 test runtime and terminal generification
45640424 fix(tui): remove redundant viewport spacer after graduation
f7f192b2 fix(tui): fix double blank line before cross-frame graduations
63d2ce0c fix(tui): graduate thinking before permission modal
f90bf73d test(tui): add spinner-in-graduation and spacing regression tests
01228a36 test(tui): add spinner-in-scrollback reproduction tests
9e4522fa test(tui): add scrollback spinner leak tests for small terminals
4cc02ad5 test(tui): replay cast recording through vt100 with scrollback
44b68a63 fix(tui): restore \r\n+force_redraw in apply(), fix flaky color tests
4eec0b10 fix(tui): restore \r\n+force_redraw in apply(), fix flaky color tests
1aa0db9a fix(tui): aggressive clear() for permission modal spinner leak
83c4a671 revert: remove aggressive terminal clear, restore original clear()
2b9e13f4 chore: add clear() diagnostic trace, fix all flaky env var tests
```

### Key Changes
- `Graduation { node, width, leading_blank }` replaces String stdout_delta
- `Terminal<W: Write>` / `OutputBuffer<W: Write>` generic over writer
- `TestRuntime` wraps `Terminal<Vec<u8>>`
- `Vt100TestRuntime` for screen-level assertions (20+ tests)
- `permission_pending` flag for thinking graduation before tool call
- All flaky env var tests eliminated (resolve_inner, detect_dark_from_colorfgbg)
- Terminal exit restores cursor below viewport
- Terminal restored on error before propagating

### What's NOT Fixed
The spinner-in-scrollback bug. vt100 replay and headless tests don't reproduce it.
It only shows in real terminals (confirmed in Zellij via reproduce.cast).

## Root Cause Analysis

### What we know from the log (~/.crucible/chat.log)

At T=13:28:10, 5 rapid graduations happen within 25ms:
```
13:28:10.077 [clear] previous_visual_rows=10 cursor_offset=2  ← big viewport
13:28:10.077 render prev=0 next=6 force=true                   ← viewport shrinks to 6
13:28:10.091 [clear] previous_visual_rows=6 cursor_offset=2    ← immediate re-graduation
13:28:10.091 render prev=0 next=6 force=true
13:28:10.095 [clear] previous_visual_rows=6 cursor_offset=2    ← and again
13:28:10.095 render prev=0 next=6 force=true
13:28:10.097 [clear] previous_visual_rows=6 cursor_offset=2
13:28:10.100 [clear] previous_visual_rows=6 cursor_offset=2
```

Each graduation cycle: clear(N) → write graduation + \r\n → force_redraw → render viewport (6 rows). Then immediately another graduation.

### The \r\n hypothesis

The key difference from old code: we now write `\r\n` after graduation content in
`apply()`. The old code had `\r\n` as part of the `stdout_delta` string (from
`drain_completed()`). The effect should be identical, BUT:

After `clear()` erases the viewport, the cursor is at row 0 of the old viewport area.
Graduation content is written (say 3 lines, cursor ends at row 2). Then `\r\n` moves
cursor to row 3. Then `force_redraw()` resets state. Then `render_with_overlays()` draws
the new viewport starting at row 3.

The new viewport (6 rows) goes from row 3 to row 8. The old viewport was 10 rows
(row 0 to row 9). Rows 0-2 have graduation content. Row 3-8 have new viewport.
Row 9 was cleared by `Clear(FromCursorDown)` during `clear()`.

But the NEW viewport at rows 3-8 includes a turn spinner at its top (row 3). On the
NEXT graduation, `clear(6, cursor_offset=2)` moves up `6-1-2=3` rows from row 5
(cursor at input) to row 2. Then `Clear(FromCursorDown)` clears rows 2-58.

But row 0-1 are NOT cleared — they have graduation content from the PREVIOUS cycle.
That's fine. But what about the spinner? The spinner was at row 3. `Clear(FromCursorDown)`
from row 2 clears row 2 onward, including row 3. So the spinner IS cleared.

### What's actually wrong

I'm stuck. The cursor math appears correct on paper. The vt100 parser confirms
correct behavior. Yet the real terminal shows spinners in scrollback.

Possible theories still open:
1. **Synchronized update timing**: We use `\x1b[?2026h` (begin) / `\x1b[?2026l` (end)
   in render_with_overlays. If the terminal processes the graduation write + \r\n OUTSIDE
   a synchronized update, and the viewport render happens INSIDE one, there could be a
   visible flash where the old spinner is visible before the viewport overwrites it. If
   the terminal captures that flash in scrollback, the spinner persists.

2. **The \r\n causes a terminal scroll**: If the cursor is at the LAST ROW of the terminal
   when `\r\n` is written, the terminal scrolls everything up by one row. The top row
   (which had graduation content) moves into scrollback. But the row that scrolled into
   scrollback might have been a viewport frame with a spinner from a PREVIOUS render,
   not the graduation content.

3. **cursor_offset_from_end is wrong**: The `last_cursor.row_from_end` might not match
   the actual cursor position in the terminal. If the cursor was positioned by
   `position_cursor()` but the terminal handled it differently (e.g., because of the
   permission modal overlay), the offset would be wrong.

## How to investigate further

1. **Compare old vs new apply() byte-for-byte**: Record the raw bytes sent to stdout
   in both the old code and new code for the same graduation sequence. Diff them.

2. **Add row tracking**: Before and after each write in apply(), emit the cursor
   position (use `crossterm::cursor::position()`) to the log. This tells us exactly
   where the terminal thinks the cursor is.

3. **Test outside Zellij**: Run `cru chat` in a raw terminal (not Zellij) to rule out
   multiplexer issues. (User insists Zellij is VT100 compliant and this didn't happen
   before, so this is likely NOT the issue, but eliminates it.)

4. **Bisect**: Find the exact commit where the spinner leak starts. The old code
   (before fb5e61e7) didn't have this issue.

## Key Files

| File | What |
|------|------|
| `crates/crucible-oil/src/terminal.rs:185` | `apply()` — graduation + \r\n + force_redraw |
| `crates/crucible-oil/src/output.rs:232` | `clear()` — viewport erasure with cursor math |
| `crates/crucible-oil/src/output.rs:64` | `render_with_overlays()` — viewport drawing |
| `crates/crucible-cli/src/tui/oil/chat_container.rs:689` | `drain_completed()` — graduation logic |
| `crates/crucible-cli/src/tui/oil/chat_app/rendering.rs:147` | Turn spinner in render_containers() |
| `crates/crucible-cli/src/tui/oil/tests/vt100_runtime.rs` | Vt100 test infrastructure (20+ tests) |
| `crates/crucible-cli/tests/scrollback_spinner_test.rs` | Cast replay test |
| `reproduce.cast` | Asciinema recording showing the bug |
| `~/.crucible/chat.log` | Debug log from session chat-2026-03-31T1328 |

## Resume

```bash
cd /home/moot/crucible
cat thoughts/shared/sessions/tui-rendering-bugs_2026-03-31-part3.md
# Log file: ~/.crucible/chat.log (session chat-2026-03-31T1328-yzvqw9)
# Cast recording: reproduce.cast
# Next step: compare raw bytes from old vs new apply(), or bisect
```
