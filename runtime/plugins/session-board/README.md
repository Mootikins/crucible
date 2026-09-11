# session-board

Every live session as a surface — `:surfaces` in the TUI, the **Surfaces** panel
in the browser.

This is the reference plugin for `cru.surface.*`. It is deliberately small: one
`declare`, one `set_rows`, and two hooks.

## What it demonstrates

- **A row is data.** It states a `mark` (`busy`, `blocked`, `ok`, `failed`) and
  each client picks its own glyph. The TUI draws `●`/`⏸`/`○`/`✗`; the browser
  draws a coloured dot. Neither knows the other's choice, and the plugin ships
  no character of its own.
- **`declare` survives a reload.** It is idempotent and keeps existing rows and
  the version, so a panel a user has open does not empty itself when the plugin
  reloads.
- **The error half is read.** `cru.session.list()` returns a value *and* an
  error. `ipairs` on the result raises when the call fails, which turns a
  background refresh into a broken panel.
- **Daemon-wide hooks.** `session:created` and `session:ended` fire for every
  session, not only the one a handler runs in, which is what a board needs.

## Try it

```
cru chat
:surfaces
```

`j`/`k` move, `esc` closes. Start another session elsewhere and the board
updates in place, keeping the cursor on the same row.

## Configuration

None. The board is every session the daemon knows.
