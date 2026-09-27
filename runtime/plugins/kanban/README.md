# kanban

A ticket policy plugin for native Obsidian Bases. Run `/kanban` in a session,
passing a registered `kiln`, to create `tickets.base` if it is absent. The
`tickets` folder must exist. Existing base definitions are never overwritten.
The initialization follows the session's apply/propose disposition.

Open `tickets.base` in the web editor or embed `![[tickets.base#Board]]`.
Use `cru base query --help` for the terminal query surface. The TUI does not
yet have a note viewer. Legacy `kanban/board` web blocks redirect to this base;
pass `kiln` in their parameters.

Ticket notes remain ordinary Markdown. Native Bases owns reads, grouping,
card moves and entry creation. `kanban_board` queries it; `kanban_move` receives
the tool invocation's explicit session context and uses the daemon write API.
There is no global board publication, manual YAML parser or direct file writer.
Unattended plugin writes need permission under the session's card, mode and
operator rules; proposals enter the Inbox instead of changing disk.

Optional setup keys: `folder` (default `tickets`), `base` (default
`tickets.base`), `wip` (status -> maximum count), and `transitions` (old status
-> allowed new statuses). The `base:before_write` policy applies to human
Bases edits as well as plugin edits, using final note properties for creations,
property edits and folder moves. Policy errors and timeouts refuse writes.
Reload removes the old source's policy callback before activation registers it
again. Post-commit observers use `base:changed`; proposed writes do not emit it.
