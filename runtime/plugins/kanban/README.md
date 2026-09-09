# kanban

A board over a folder of markdown tickets, declared as an Oil tree.

A ticket is a `.md` file with `status:` in its frontmatter. Nothing else is a
ticket, and the plugin stores nothing of its own — moving a card rewrites that
one line, so the folder stays a folder of notes.

Embed the board in any note:

    ```oil
    kanban/board
    { "folder": "tickets" }
    ```

`folder` is optional and resolves against the active kiln. It defaults to
`tickets`.

## Why this plugin exists

It is the smallest honest test of one question: can a plugin declare a whole
view — not a settings form — and have the TUI and the web both draw it from
that one declaration? See `docs/Meta/Analysis/Oil in Documents.md`.
