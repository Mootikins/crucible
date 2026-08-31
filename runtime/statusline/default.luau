--!strict
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

local sl = cru.statusline

-- `{ any }`, deliberately. A region holds items, bare strings (rendered as
-- literal text) and nested rows, and Luau unifies an array literal's element
-- type from its first entry — so `{ sl.mode:hl("X"), " " }` reads the string
-- as the wrong type unless every list carries a cast. Casts at every level
-- would state nothing and hide the checks that matter.
--
-- What IS checked is each item as it is built: `sl.model{ max = 25 }`,
-- `sl.any(...)` and `sl.when(cond, item)` all take `StatusSlot`, so a
-- misspelled option or a non-renderable argument is caught here. Membership of
-- the list is checked at run time by `value_to_item`, which warns and drops.
local layout: { [string]: any } = {
  -- A region is an ordered list and position is the arrangement, so the input
  -- being an element is what puts this row underneath it. Move `sl.input` down
  -- and the row moves above.
  -- `:: { any }` on this ONE list, because it is genuinely heterogeneous: an
  -- item beside a nested row. Luau unifies an array literal's element type
  -- from the first entry, so `{ sl.input, { ... } }` reads the row as the
  -- wrong type. Everything inside each row is one type and needs no cast.
  prompt = {
    sl.input,
    {
      sl.mode:hl("StatusMode"),
      sl.text(" "),
      sl.model{ max = 25 },
      sl.align,
      -- Tools that outran the split threshold have no transcript node to
      -- show progress in. This renders nothing while none are running.
      sl.tasks,
      -- A notification takes the right-hand slot while it is showing, and
      -- context usage takes it back afterwards.
      sl.any(sl.notification, sl.context),
    },
  } :: { any },
}

return layout
