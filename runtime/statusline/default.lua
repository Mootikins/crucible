-- The statusline Crucible ships with.
--
-- This is the real default, not an example: the daemon evaluates it and sends
-- the result to every client. Copy it into your own `init.lua` and edit — the
-- vocabulary here is the whole vocabulary.
--
-- The TUI also carries a compiled-in copy of this layout so it renders
-- correctly with no daemon at all. A test asserts the two agree, so changing
-- this file without changing `builtin_default()` in `statusline_items.rs` is a
-- build failure rather than a silent divergence.

local sl = crucible.statusline

return {
  -- A region is an ordered list and position is the arrangement, so the input
  -- being an element is what puts this row underneath it. Move `sl.input` down
  -- and the row moves above.
  prompt = {
    sl.input,
    {
      sl.mode:hl("StatusMode"),
      " ",
      sl.model{ max = 25 },
      sl.align,
      -- A notification takes the right-hand slot while it is showing, and
      -- context usage takes it back afterwards.
      sl.any(sl.notification, sl.context),
    },
  },
}
