# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.21.0] - 2026-08-03

Sandboxing, and what it takes to make it real. 0.20 could put an agent's tools
in a container; it could not *prove* they stayed there, could not tell you so
from a browser, and could not run an external agent under it at all. This
release closes those three, then generalises the result: where a session's
files live and where its process runs become two plugin-contributed axes rather
than one hardcoded setting, and the composer is built from what plugins publish
instead of from TypeScript literals.

### Added
- **Session isolation is default-deny, by tool *surface*.** A plugin calls
  `crucible.require_isolation` and from then on any host-touching tool it did
  not handle is refused. Interception used to be an allowlist of six tool names,
  which is complete only by coincidence — a seventh workspace tool, a
  plugin-contributed tool or an MCP gateway tool escaped silently. The gate now
  asks what a tool's executor can *reach* (`Host` / `Daemon` / `Unknown`), so
  kiln tools survive a claim by construction and one added next year inherits
  that instead of falling off a list nobody updated.
- **External (ACP) agents can run inside the sandbox.** A claiming plugin
  supplies the argv that relocates a command into its container, and the daemon
  launches the agent *through* it — `cru chat -a claude` in a container instead
  of a refusal. The daemon cannot intercept tools an ACP agent runs in its own
  process, but it does not have to when that process is already confined.
- **Per-session isolation opt-in.** `session.create` takes an `isolation`
  parameter, forwarded to plugins untouched. Absent, `false` and a named
  environment are three different instructions and stay distinguishable end to
  end.
- **The project's devcontainer decides the environment.** `oci` reads
  `.devcontainer/devcontainer.json`, so an agent works in the container the
  human's editor builds rather than a second one configured separately. Six
  keys that configure the container *from outside it* — `runArgs`, `mounts`,
  `initializeCommand`, `features`, and the compose keys — need operator opt-in,
  because the sandboxed agent can write that file.
- **Plugins publish what they offer.** `crucible.publish("<key>", value)` is a
  generic contribution channel that clients read back verbatim. Web used to
  infer the isolation offer by matching on the shape of `[plugins.oci]`, which
  put one plugin's config schema in the rendering layer and would have ignored a
  second isolating plugin entirely.
- **Plugins declare settings once and every frontend renders them.**
  `crucible.options{…}` is an Ace3-style tree of typed nodes; the web settings
  pane switches on `type` alone, so a plugin shipped tomorrow gets a pane for
  free. Values changed there persist and are replayed through the plugin's own
  setter on reload.
- **Plugins can report progress.** `crucible.set_status` fills a per-session
  slot the TUI and web both render, and `cru.shell.spawn` streams a command's
  output line by line — a container build now reports itself instead of looking
  like a hang.
- **Workspace and runtime targets, both contributed by plugins.** Two orthogonal
  axes: *where do the files live* (a worktree, a remote checkout) and *where
  does the process run* (a container, an ssh host). They compose — a session can
  run in a container against a worktree — which `oci` already assumed and
  nothing declared. Providers publish themselves on the `targets` channel and
  enumerate on demand, because a branch list depends on the selected project and
  changes when someone creates a branch outside the app.
- **A `worktree` plugin.** Shells out to git the way `oci` shells out to podman.
  Resolution runs before the session exists, so the session is *born* in the
  right checkout rather than moved into one afterwards, and a target that cannot
  be resolved refuses the session — an agent that quietly works on `main` when
  it was told `feat/x` commits there.

### Changed
- **The composer's chips are built from publications, not literals.** Three
  controls became two: the branch chip (which called `scm.worktree_add` directly
  and confirmed with `window.confirm`), a hardcoded "run on" chip whose only
  enabled row was *This machine*, and a separate isolation toggle. The last two
  were always the same question asked twice. Menus drill into a submenu once a
  second provider answers on an axis, and open on hover.
- **A runtime target names its provider.** `oci` previously raised on any
  isolation name it did not recognise — correct while it was the only plugin on
  that channel, and fatal the moment a second one exists. It now ignores a
  target addressed elsewhere. Bare `true` / `false` / `"profile-name"` keep
  working; every existing config sends those.

### Fixed
- **Delegation escaped the sandbox.** `create_child_session` fired no plugin
  start hooks, so a subagent of a sandboxed session ran with no container and no
  claim — on the host. Enforcement is now shared by every path that starts a
  session, including `session.fork` and `cru.tools.call`.
- **A devcontainer could configure its own way out.** The agent can write the
  file that describes its own sandbox; `runArgs: ["--privileged"]` and a `/`
  bind mount were both verified escapes, as were local `features`. Gated behind
  operator config, and documented as a speed bump rather than a boundary — which
  is what the devcontainer spec's own maintainers call it.
- **Git worked in a container only by accident.** A linked worktree's `.git` is
  a *file* holding an absolute host path, so mounting the worktree alone left
  every git command inside the container broken. The common git dir is mounted
  alongside it. (Under podman a `-v` whose source and destination are identical
  is silently dropped — `--mount` is used instead.)
- **A plugin's odd publication shape no longer hides the whole offer.** An empty
  Lua table encodes as `{}`, not `[]`; iterating it threw, the rejection was
  swallowed, and the isolation control silently never appeared.
- **`cru session create --format json` is honoured when piped.** Output falls
  back to the bare session id when stdout is not a terminal — right for
  `$(cru session create)` — but that implicit default outranked the explicit
  flag, so JSON was printed only on a terminal and silently degraded to an id
  exactly when something was parsing it. `--quiet` is explicit too, and still
  wins.

### Removed
- **Five tests that could not pass, and one that could not fail.** The four
  `test_watch_*` CLI tests aborted the watch task and then asserted
  `result.is_err() || result.unwrap().is_ok()` — a tautology on an aborted
  handle, so "detects file modification" wrote a file and checked nothing; they
  had also stopped running at all once storage moved daemon-side. Watch is
  tested where it happens, in five unignored daemon suites. `test_event_stream`
  asserted that *no* event arrived within 100ms, which cannot distinguish
  correct from broken and failed against any daemon with traffic.
- **`scm.branches`, `scm.worktree_add`, `/api/scm/branches`, `/api/scm/worktree`
  and `[scm] worktree_dir`.** Worktrees are the plugin's business now; the
  daemon and the web frontend each held their own copy of what a branch is. The
  destination template moved to `[plugins.worktree] template`. `scm.clone`
  stays — cloning has no plugin behind it yet.

## [0.20.1] - 2026-08-01

### Fixed
- **`cru setup` no longer freezes your defaults.** It copied the whole runtime
  tree, `defaults/init.lua` included, into a directory that outranks every
  shipped root — and defaults are read first-hit-wins, so the copy shadowed the
  real file permanently. Any default added in a later release reached nobody who
  had run setup. Plugins and themes are still copied: those layer per name, so
  your copy shadows only what it names and a newly shipped plugin still loads.
  Override defaults in `~/.config/crucible/init.lua`, which already runs after
  them.

## [0.20.0] - 2026-08-01

Two themes. The first is packaging: everything under `runtime/` — plugins,
themes, the bundled help skills — reached nobody who installed Crucible rather
than cloning it, because no release ever put that directory on disk. The second
is the mirror of the last release's: 0.19.0 asked whether features reached the
*user*, and this one found two places where a tool's answer never reached the
*model*.

### Added
- **The `runtime/` tree ships inside the `cru` binary.** Release archives
  carried no `runtime/` directory, so bundled plugins (`kiln-expert`, `oci`,
  `reflection`), the help skills and the themes were dead for every installed
  user. `cargo-dist`'s `include` key does put the tree in the archive, but the
  shell installer it generates moves only binaries and libraries out of the
  unpacked directory and deletes the rest — and `cargo install` never had a
  data path at all. So the tree travels in the binary (144K) and is extracted
  on first daemon start when nothing on disk answers. Covers the tarball, the
  installer, `cargo install` and distro packages at once.
- **`cru setup` works on an installed binary.** It resolved its source the same
  exe-relative way the daemon does, found nothing, and exited with "Could not
  find Crucible runtime files" — for exactly the users it exists to serve. With
  no source on disk it now writes the copy compiled into the binary.
- **An operator can admit a plugin tool to plan mode.** Plan mode refused
  plugin tools categorically, which made it unusable for the one class that
  belongs there: research. Naming a tool exactly in the mode's `tools` list
  admits it — a glob or `*` does not, so a plugin cannot pick a name that walks
  through a rule written before it existed.
- **An experimental `web-search` plugin**, off by default, with a provider
  chain and a normalised result shape. Its DuckDuckGo scraper is not in the
  default chain.

### Fixed
- **Tool failures reach the model.** Every failure path — permission denial,
  dispatch timeout, containment refusal, unknown tool, plugin cancel — set the
  result empty and put the text in an `error` field that the message-list
  builder never read. The model received `""` and either repeated the identical
  call or invented an outcome, while the TUI was shown the real message through
  a separate channel.
- **Large tool output is readable again.** Results over 10KB are replaced by a
  reference to `$CRU_SESSION_DIR/tools/…`, but `read_file` did no environment
  expansion, so it could not follow the reference it was handed — and the
  failure then arrived as an empty string. Only `bash` could reach spilled
  output, which is why agents learned to reach for `bash` instead of the tool
  that would have worked.
- **A new project no longer pays for the whole prompt.** The system prompt
  opened with the workspace path, so prompt caching diverged at the first byte
  and two sessions in different projects shared nothing — not the persona, not
  `AGENTS.md`, not the skills catalog. Stable content leads now, with its own
  cache breakpoint.
- **An empty list no longer reaches the model as an object.** Lua cannot tell
  an empty list from an empty map and the encoder resolved it as a map, so a
  tool returning results emitted `[…]` when it found some and `{}` when it did
  not — the JSON type of the field tracking the data.
- **Bundled skills load from an installed layout.** Skills discovery tried only
  the dev tree; all four runtime-root resolvers now share one list.

## [0.19.0] - 2026-07-31

Mostly one thing: `docs/Meta/Product.md` was audited against the code, a third
of its `[x]` claims did not survive, and this release builds the ones worth
building. Twelve features that the product map said shipped were reaching
nobody. The recurring cause is a test that asserts a value round-trips rather
than that it arrives — `test_temperature_round_trip` passed for months while no
LLM request carried a temperature.

### Added
- **`cru search --type text` looks inside notes.** It matched titles and
  filenames only; the FTS5 index it was credited with was never written to or
  read from. Kilns processed before this are backfilled once, on open, so an
  existing kiln does not search as empty.
- **Project rules files reach the agent's system prompt.** `AGENTS.md` and
  friends, walked from the repo root down to the workspace. The loader was
  deleted with `crucible-context` in February and the config key kept parsing.
- **`@file` in the composer attaches the file's contents.** It used to insert
  the literal string `@src/main.rs` and push it onto a field nothing read; the
  agent could only get the content by choosing to call `read_file`.

### Fixed
- **Notes created while the daemon runs are indexed.** The watcher's handler
  registry was built empty behind a feature flag that exists in no
  `Cargo.toml`, so the reprocess task downstream — complete and correct — had
  never received an event.
- **Every named colour rendered as the wrong palette slot.** `Color::Red` came
  out as bright red and `bright_red` as red, across all six chromatic pairs,
  because crossterm's variant names sit one slot up from these. Set `blue` in
  your colorscheme and you now get the slot 4 your terminal is configured with.
- **The session's temperature, max tokens, context budget, context strategy and
  context window reach the model.** The agent factory built the handle with the
  thinking budget and nothing else. No context strategy had ever run against a
  real session, and tool-schema deferral was always guessing at the window.
- **`:set precognition` reaches the daemon** instead of setting a TUI-local
  string the `:set` readout reads back to you.
- **Shell output reaches the composer and the transcript.** `i` closed the
  modal and inserted nothing; a finished `!command` left no trace in the
  conversation. Both halves of the same key press, both landing nowhere.
- **Type stubs describe the VM plugins actually run on.** The generator
  fabricated six `cru.*` namespaces out of bare globals — measured against a
  real plugin VM, 6 stubbed namespaces did not exist and 12 that did had no
  stubs at all. `cru plugin new` now writes the stub directory into the
  scaffolded `.luarc.json`, which is what "zero-config IDE setup" meant.
- **`cru.tools.call` respects the operator's `[permissions]` rules**, which its
  own documentation already claimed. It went straight to the executor with no
  permission check of any kind, so any loaded plugin could run `bash`
  unprompted, in any mode, including plan.
- **The permission config applies to ACP-hosted agents at all.** The gate took
  its tool name from ACP's `title` — `"Read src/main.rs"`, prose meant for a
  human — so no `[permissions]` rule naming a tool could ever match it and the
  whole config was inert on that path. Rules now match against the tool `kind`,
  the only tool identity ACP puts on the wire.
- **An explicit `deny` or `ask` outranks the read-only exemption.**
  `deny = ["read_file:*"]` was ignored because a hardcoded name list was
  consulted first; `ask = ["read_file:*"]` was ignored because the engine
  reported "a rule said ask" and "nothing matched, the default is ask" as the
  same answer.
- **An interrupted text-index backfill is finished on the next open**, instead
  of being abandoned because the index was no longer empty.
- **Bundled skills no longer shadow skills you wrote.** They shared a scope with
  `<kiln>/skills`, so a name collision was settled by search-path order — and
  the shipped one won.
- **The runtime tree `cru setup` writes is read.** It copied to
  `~/.config/crucible/runtime` and printed instructions to set an env var,
  because no resolver looked there.
- **Bundled help skills load from an installed binary.** Skills discovery tried
  only the dev layout, so `~/.local/bin/cru` looked in `~/runtime`. _Release
  tarballs still ship no `runtime/` directory, so this is not yet enough on its
  own._
- Setters on `SessionConfigRpc` no longer report success for work not done. A
  plugin writing `session.thinking_budget = 4096` on the daemon VM was told it
  worked; it now says what happened.

### Changed
- **BREAKING — the MCP server serves the kiln, not a second copy of `bash`.**
  `read_file`, `edit_file`, `write_file`, `bash`, `glob` and `grep` are gone
  from `cru mcp`; the surface is 15 tools, kiln and delegation. Any harness
  speaking MCP already has its own file and shell tools, Crucible enforced no
  permissions on the copies it served, and the agent factory added the same six
  separately — so every kiln session was advertising each of them to the model
  twice. Configure the client's own equivalents instead.
- A config with `default = "deny"` now denies read-only tools too, on both the
  ACP path and `cru.tools.call`. A blanket deny is taken at its word.

## [0.17.1] - 2026-07-28

No user-visible changes. This release covers the build and test infrastructure
behind 0.17.0.

### Fixed
- **A daemon test failed on CI for a real reason, not flakiness.** The daemon multiplexes replies and server-pushed notifications down one socket, so a single read routinely returns a reply followed by the head of the next message. Three test helpers each allocated their read buffer per call, so returning a reply discarded whatever else that read had captured; the next call began mid-message and parsed a fragment. `plugin.reload` pushes a notification twice the size of the read chunk, which is why one test failed and its siblings did not, and why a retry did not help — the second attempt re-runs the same sequence. The helpers are now one connection type that owns its buffer, covered by a regression test that forces the interleaving deterministically. Reproducing the original needed contention rather than repetition: 64 sequential runs on an idle machine were clean, while twelve copies pinned to two CPUs failed 54 times out of 72.
- The shared daemon fixture waited for the socket *file* to appear and then slept a fixed 50ms. `bind()` creates that path before the daemon is listening, so the constant was doing the work; it now polls until a connection is actually accepted. This was the last copy of a pattern already replaced elsewhere after the same intermittent failures.

### Changed
- The dependency licence gate runs on CI. It shipped in 0.17.0 wired into `just ci` only, and the GitHub workflow does not invoke `just ci` — so the check that exists to catch a new dependency's unexpected licence ran only when someone happened to run the full local suite. Both sides now run the same command, over all features rather than only those the release binary enables, so a licence cannot enter through a feature that is off today and on in a later release.

## [0.17.0] - 2026-07-27

### Added
- **Crucible reads and writes JSON Canvas 1.0** — the `.canvas` format Obsidian uses for its infinite-canvas view — with an editor in the web UI. Cards, groups, images, links and edges; pan, zoom, marquee select, grid snapping, undo/redo. Using Obsidian's spec rather than inventing one is the whole point: an existing vault opens without conversion, and a canvas Crucible saves is byte-identical to one Obsidian saves, down to tab indentation and key order. Unknown keys round-trip verbatim, so styling authored by a plugin like Advanced Canvas survives a save here instead of being silently dropped.
- **Canvases are citizens of the knowledge graph.** A canvas contributes each file card, and every wikilink written inside a text card, as links to those notes — so a note's backlinks show the canvases that reference it. Obsidian does neither: canvas references never appear in backlinks there, and text-card wikilinks never appear at all. Renames and moves rewrite canvas references through the typed document model rather than by splicing bytes into a JSON string.
- **Note cards are live views of the file, not copies.** A card renders the real note through the same markdown engine as the rest of the app, and editing one writes back to the source. The canvas stores only the path, so note edits never touch the `.canvas` file — they are separate documents with separate undo histories, which is the only way Ctrl+Z stays unambiguous.
- **Web cards embed the live page, and can be created three ways.** `link` nodes were always read and written — they are part of the spec — but rendered as a static card that could not be authored here at all, so a canvas made in Obsidian showed its web cards and Crucible could not add one. The page now renders in place, and a card is made from the toolbar, by pasting a URL onto the canvas, or by dragging a link out of the browser onto the spot you want it. Embedding means opening a canvas contacts every third party it references, so the frame is sandboxed into an opaque origin: it may run scripts, submit forms and open popups, but is never granted `allow-same-origin`, so a card pointed at Crucible's own address cannot read your session cookie or call the API as you. Only `http:` and `https:` can be embedded or authored, and a dangerous scheme is refused when the card is made rather than merely rendered inert, so it never reaches the document. The frame ignores the pointer until the card is selected, which is what keeps an unselected card draggable.
- **A zoom control with a minimap.** Clicking the zoom readout opens a slider — logarithmic, so equal distances mean equal ratios rather than crushing everything below 100% into the first fifth of the track — with a detent at 100%, stepped zoom buttons, and a minimap you drag to move the canvas. Zooming is eased rather than stepped, and a canvas opens at 100% centred on its content instead of at whatever scale happened to fit.
- **Canvases publish to the documentation site as read-only boards.** Cards sit where they were authored, edges are drawn, note cards link to the notes they reference, and text cards render as markdown with wikilinks resolved. The site imports the application's own geometry rather than reimplementing it, so a board looks the same in both places. Web cards are links there, not live frames — a documentation page that silently contacted every site a board mentions would be a worse bargain than the application's, not a better one.
- **A canvas may only reference files inside the root that owns it.** Not "any open kiln" — the specific kiln, or, for a canvas that lives in a repository rather than a vault, that project, so an architecture board can sit with the code it describes. Enforced in three layers, because the UI layer is worth nothing alone: drop targets filter and explain rejections, the save path validates every reference before anything touches disk, and the read path redacts references that fail the check so a client never receives a path it could not have asked for. Otherwise a text editor is a bypass. Rejections cover `..` traversal, absolute paths, interior NULs, and symlinks escaping the root; a reference to a merely *deleted* note stays legal and renders as a broken card.

### Changed
- **One predicate decides what the kiln considers a file.** `KilnFileKind` replaced twelve hardcoded `extension == "md"` checks that had already drifted apart — some accepted `.markdown`, none were case-insensitive, so a note named `Notes.MD` was indexed by some code paths and invisible to others.
- **Frontmatter collapses to a single square.** A note's Properties block is collapsed by default and reduced to one small toggle tucked into the corner, so the note's own first heading sits at the top of the page instead of below a header-shaped bar. Expanded, the first property row flows beside the toggle rather than under a full-width band.
- Canvas cards gained the live/source switch every other markdown surface has — a card had no way to reach raw text at all, so fixing a link's syntax meant opening the note somewhere else. Cards also carry resize handles on all four edges, each constrained to its own axis, and each edge is one hover area that both resizes and starts a connection.
- **Wikilink resolution is one ladder with no ambient fallback.** Resolving a link used to be able to fall back to "whichever kiln is currently configured", which is how a link in one vault opened a same-named note from another. A file's kiln is now derived from the file's own path, and content whose kiln is unknown has no links to follow rather than guessing one. Resolution is a path lookup and no longer depends on a note having been indexed.

### Fixed
- **The note index never removed notes that were deleted from disk.** Reprocessing a kiln added and updated, but never diffed against the filesystem, so a note deleted or moved outside the app stayed in the index forever — surfacing in search results, backlinks and autocomplete as a file that could not be opened. The documentation kiln had accumulated 70 such ghosts out of 180 entries. Reprocessing now reconciles against disk, and declines to do so when the kiln root is missing entirely, so an unmounted or renamed directory does not empty the index it was supposed to refresh.
- Following a wikilink from the standalone editor panel resolved nothing, because the panel passed no kiln.
- **Clicking a card inside a group selected the group.** Groups are painted behind every card, but hit testing walked raw document order, so a group listed after its members won the click — and dragging a connection onto such a card connected it to the group instead. Hit testing now matches what is painted.
- **The Properties card could not be expanded in the editor.** Clicking it in live preview dropped you into the raw YAML instead of opening it, because the editor treats a click on rendered prose as a request to edit the source behind it and only the foldable callout title was exempt. Expanding it also pushed the note off the top of the viewport: a disclosure changes the widget's height after the editor has measured it, and the browser's scroll anchoring then compensated for the growth.
- Curved edges were visibly jagged. The canvas carried a `shape-rendering` value copied from a community performance patch which does not merely relax precision — it switches anti-aliasing off.
- Dragging a connection gave no sign of where it would land. The destination is now outlined with the anchor the edge will attach to, resolved by the same hit test the drop performs, so the highlight cannot promise a connection the release then refuses.
- The debug web server served stale assets after a rebuild, because the production service worker keeps its precache until an update prompt is accepted — a freshly deployed change was invisible and read as a bug in the change. Debug builds now ship a service worker that unregisters itself.

## [0.16.1] - 2026-07-27

### Fixed
- **The daemon crashed parsing notes containing multi-byte characters.** The footnote extension walked a `Vec<char>` while slicing the note by byte offset; those indices agree only for ASCII, so one em dash before an inline footnote desynchronized them and the slice landed mid-codepoint. In the daemon that panic closed the client's connection, and `cru status` against a real kiln reported "Connection closed by daemon". Present since 0.15.0. Parsing is now covered by a suite that runs every extension over multi-byte text — em dashes, CJK, emoji, ZWJ sequences and combining marks — in the positions where a byte/character mix-up bites.
- **A panicking handler no longer takes the connection with it.** Every RPC now dispatches behind a panic boundary, so a bug in one handler returns an error for that request instead of dropping the socket mid-conversation. Panics are still bugs and are logged at error level with the method name; they are simply no longer fatal to everything else the client was doing.


## [0.16.0] - 2026-07-27

### Added
- **The TUI is themed from Lua.** `crucible.colorscheme.setup{}` defines a palette, `crucible.hl.set/link` define and link highlight groups, and `crucible.ui.setup{}` sets per-surface geometry — borders, padding, prompt glyphs. The Lua VM lives only in the daemon, so all of it is delivered to every attached client as data over a `ui.config` RPC, and a change repaints without a restart. Before this, `crucible.theme` was nil in the VM that evaluates your `init.lua`, so a theme never parsed at all.
- **Colours can defer to your terminal.** `term4`, a bare `4`, or `"bright_magenta"` name a terminal palette slot rather than a fixed colour, so a colorscheme follows whatever the user's terminal theme puts there. The names are slot aliases, not appearance promises — `"blue"` is exactly slot 4, which plenty of themes fill with something else, so `term4` is the honest spelling and hex is for when you mean a specific colour. Adaptive `{ dark, light }` pairs cross the wire unresolved, because the daemon cannot know which terminal a client is attached to.
- **Code blocks follow the colorscheme.** `crucible.syntax.setup{ theme = "derived" }` (the default) builds a syntect theme from the palette so a fenced block does not clash with the chat around it; `:set syntax_theme=<name>` switches to any bundled theme at runtime. Terminal palette slots survive into code even though syntect's colour type is RGB-only.
- **The statusline is composed from named items.** `sl.mode`, `sl.model{ max = 25 }`, `sl.expr("git")` and friends, with `sl.any`/`sl.when` for fallback and conditions the TUI alone can answer. Deliberately not `%X` format strings: sigils need escaping rules, and the distinction between "insert as text" and "re-interpret as format" is exactly where ANSI injection lives.
- **The screen is three ordered regions** — `top`, `prompt`, `bottom` — and `sl.input` is an element within one, so rows written above or below the editor render above or below it. Position in the list is the arrangement; there is no ordering field to get wrong. The shipped default is authored in `runtime/statusline/default.lua` in the same vocabulary you would write.
- **Daemon-computed statusline values.** A handler supplies a git branch or a queue depth with `cru.statusline.set(session, "git", value)`, and the placed `sl.expr("git")` renders it. Values are text, never escape sequences, which is what lets the TUI strip control characters from them unconditionally. An unset value renders nothing, so a bar does not jump when one first arrives.
- **Lua can hook file-watch events.** `crucible.on("FileChanged", ...)` fires when the workspace changes — the trigger a value like git status actually needs, since files change while you are not in a turn.
- Per-edge border characters, matching Neovim's `nvim_open_win` order. An empty string means that edge is absent and occupies no cell, distinct from `" "`, which is blank but still takes one.

### Changed
- **"Theme" meant three unrelated things and now means none of them.** `crucible.colorscheme` is the palette, `crucible.ui` is geometry, `crucible.syntax` is code highlighting. `:set theme=` became `:set syntax_theme=`.
- The statusline config shape changed with the region model: bars used to be named (`main = { anchor = ..., items = ... }`). That spelling still reads as valid, so a key that is not a region now warns rather than silently placing nothing.
- Gray and dark gray now emit the terminal's own palette entries on every render path. One path mapped them to the palette while another hardcoded 256-colour approximations, so the same colour rendered differently depending on how it got there, and neither followed the user's terminal.

### Fixed
- **A theme chosen at runtime was reset by the next `cru chat`.** Registering the UI namespaces seeded the built-in default unconditionally, and `lua.init_session` re-enters that on a throwaway VM for every client — so a palette installed by `ui.set_theme` snapped back to the default the moment a second client connected.
- **A global style change reached no one.** Delivery filters on a session id and every TUI subscribes to its own, so a change addressed to a placeholder session was dropped while the RPC reported success. The wildcard was also asymmetric: subscribing to `*` worked, but addressing `*` matched nothing.
- **Statusline values were recorded and never delivered.** The change notifier had no production caller, so a pushed value updated only when something else happened to trigger a repaint.
- **Bidi and zero-width characters could reorder the statusline.** They are not control characters, so stripping `is_control` let them through, and a right-to-left override changes how a bar reads without changing what it contains — in a branch name, that is attacker-influenced in any repo you clone.
- **The completion popup could paint over the footer.** It reserved a fixed three lines, which was the footer height only while the footer was one bar and the input one line; it was already wrong for a wrapped multi-line message. It now measures the prompt region.
- The web model picker could show a stale list, because two overlapping requests could resolve out of order and the older one win.


## [0.15.0] - 2026-07-24

### Added
- **Unified Navigator.** One left panel with a scope swapper (Sessions / each kiln / each project) that swaps the body, plus a search button that takes it over — replacing the separate Files, Sessions and Search tabs. It composes the existing engines, so drag-and-drop, context menus, grep and session actions all keep working.
- **Search across notes, files and sessions.** A new `search_grep` RPC exposes the ripgrep engine (previously reachable only from the MCP tool) over RPC and `POST /api/search/grep`, with fail-closed containment: a `root` is accepted only if it canonicalizes inside a registered project or open kiln. The search pane fans one debounced query out to notes, files and sessions, highlights match spans from the daemon's char offsets, and takes its scope from context — a kiln searches its notes, a project its files, with operator hints that follow the scope.
- **Semantic search in the web UI.** `POST /api/search/semantic` embeds the query and runs a vector search (the same two-step the CLI uses); the search panel gains a Text | Semantic toggle with per-hit similarity scores.
- **LaTeX and Mermaid render everywhere.** `$…$` / `$$…$$` render with KaTeX and ` ```mermaid ` fences render as diagrams — in the reading view, in chat, and in the live-preview editor, each behind its own Settings → Editor toggle. Mermaid is lazily imported the first time a diagram actually renders, so notes without diagrams pay nothing.
- **Review an agent's proposed edit in the real editor.** An Edit/Write/MultiEdit tool card gains "Open in editor": it opens (or focuses) the file with the proposed change overlaid as an inline unified-merge diff — green/red with per-hunk Accept/Reject in the gutter. Accept the hunks you want and save; the accepted text is what reaches disk. A proposal is never staged over a buffer with unsaved edits.
- **Git-aware project setup.** `scm.clone` creates a project from a remote repo URL (contained to the projects dir), and `scm.branches` / `scm.worktree_add` back a repo/branch picker pair on the composer: pick a branch to jump to its existing worktree or create one from a configurable `worktree_dir` template — N sessions across N worktrees without leaving the composer.
- **Session-unique scratch workspaces.** A session created without an explicit workspace now gets a private directory at `<session_workspace_dir>/<session_id>` (`[scm] session_workspace_dir`, default `~/.crucible/workspaces`) as its filesystem containment boundary, instead of falling back to the kiln. Directory-creation failure warns and falls back; session creation never fails over it.
- **Edge panels host full split trees.** An edge panel carries the same recursive pane/split layout as the center tiling, rendered by the same stack: directional drop zones, keyboard split, per-pane tab bars, and ribbons that aggregate tabs across every leaf group.
- **The file tree shows the whole folder.** `fs.list_dir`'s single flag conflated gitignored entries with dotfiles; split into `show_ignored` (the tree always requests them — a file browser shows the folder, not the git index) and `show_hidden` (off by default, toggled from the context menu or the palette, persisted per browser). `.git` is never listed. Configured extensions can be hidden (default `.md`), and the tree reveals and syncs to the active file.
- **ACP agents wear their own mark** in the composer's agent picker — a glyph per built-in agent, resolved by name so a custom profile extending one keeps its family's mark.
- Docs: `Help/Diagrams and Math` documents both renderers and doubles as a render check.

### Changed
- **The session composer is a New Session tab**, not an empty-pane splash: a pane with no tabs now renders nothing at all, in any region. Starting a session is a deliberate act (the ribbon, the command palette). The older, plainer draft surface is gone — one creation path.
- **Sessions are always resumable.** A session's event log is on disk, so lifecycle state never blocks continuing a conversation: sending to an ended or evicted session transparently revives it (resident, else resumed from storage, with the kiln resolved from a new `session_kilns` index). The sessions surface drops the lifecycle axis entirely, and the session list is global rather than implicitly kiln-scoped.
- The composer paints last-known values instantly from an SWR catalog cache instead of blank chips, and recents are served from the daemon.
- An Obsidian-minimal pass over the shell: 13px tree rows, quiet user bubbles, hover-only absolute times, mic and send sharing one pill, chat input matching the composer.

### Fixed
- **A proposed edit could destroy unsaved work.** The proposal's baseline is the file on disk; staging it into a buffer with unsaved edits overwrote content that baseline never contained, and dismissing then "restored" the disk text. The review now waits for a clean buffer, clears itself once saved, and survives an unrelated editor reconfigure without resurrecting accepted hunks.
- **A fresh profile opened a dead left panel.** The Navigator refactor unregistered the `files` and `sessions` panels and remapped them in persisted layouts, but left them in the default seed — two tabs rendering "Unknown content type". Edge panels now derive their opening tab from their own roster.
- **Inline math could swallow a sentence.** A currency `$` closed on the `$` inside a later code span, consuming everything between; inline math may no longer cross a backtick.
- **Diagrams rendered as a speck in a huge empty box.** Every dagre-based diagram (flowchart, state, class, ER, git) shipped a viewBox several times its content; the frame is now refitted to what was actually drawn.
- **Live preview rendered `$$` blocks quoted inside code fences**, splitting the fence around a formula, and keyboard motion skipped mermaid and display-math blocks so they could only be opened by clicking.
- `search_grep` was dispatched but missing from the RPC `METHODS` list, so clients enumerating methods could not discover it.
- Live preview renders the tail of large files, `---` as a rule, and tables and callouts on scroll rather than only on cursor move; mermaid node labels survive sanitization.
- Edge panels stay mounted while collapsed so expanding is a pure translate; a partial or corrupt layout payload degrades to an empty pane instead of bricking the shell; standalone instances persist layout separately.

### Security
- `scm.clone` with an explicit destination is contained to the projects dir (canonicalized, `..`-free, symlink-hop safe) — it was the one write path skipping the allowlist containment every other endpoint enforces.
- Delegation and agent trust gates resolve data classification through the workspace config *and* a kiln walk-up: session-unique scratch workspaces carry no `.crucible` config, and the workspace-only lookup had been silently downgrading confidential kilns to Public at delegation time.
- Mermaid SVG is sanitized on both sides of the viewBox refit, since fitting reparses and re-serializes the markup.

## [0.14.0] - 2026-07-22

### Added
- **Subagent delegation actually works** — the internal `delegate_session` path was never wired in production (three independent breaks). Delegated children are now real sessions driven through the main scheduler: working tool execution, Precognition knowledge injection, Lua hooks, per-turn events on the child's own session id, and standard persistence. Blocking and background modes both honor `timeout_secs` and `result_max_bytes`.
- **Agent cards define delegation targets.** The documented card format is now fully implemented (only `description` is required; `name`/`version` default; `mcps` alias; `tools:` accepts `true`/`false`/`allow`/`ask`/`deny`). Cards are discovered from `~/.config/crucible/agents/`, kiln `agents/`, and project `.crucible/agents/` (later shadows earlier), and `delegate_session` resolves targets card-first, then ACP profiles. `session.create` accepts `agent_name` for card-configured internal agents.
- **Card model-resolution chain**: card-explicit `provider`/`model` > `specialty:` mapped through the new `[llm.models]` config table (`reasoning = "openai/o1"`, bare model inherits the provider) > inherit from the spawning context (the delegating parent, or configured defaults).
- **Child sessions are hidden but real**: excluded from `session.list` unless `include_children` (`cru session list --include-children`), linked via `parent_session_id`, ended with their turn, and archived/deleted/cancelled together with their parent.
- **Real nested delegation**: `max_depth` now works as documented — `2` lets a delegated child delegate once more; depth derives from the parent-session chain, so a child cannot lift its own cap.
- Production-wiring delegation e2e (real server construction + scripted LLM) — the test class whose absence let the unwired path ship.

### Security
- `[permissions]` config is now enforced for internal agents (was ACP-only): config `deny` is absolute — including over an agent card's `allow` — and `default = "allow"` skips prompts. Non-interactive sessions deny would-prompt tools immediately instead of hanging.
- The project `[security.shell]` policy now applies to the agent `bash` tool, checked per chained statement (defense-in-depth, not a sandbox).
- Workspace file tools are contained to the workspace + kilns + session directory: symlink and `..` escapes are blocked for reads and writes, and glob patterns can no longer traverse out.
- Delegation trust derives from the target's actual provider trust level instead of assuming Cloud, so a local-model card can serve a confidential kiln while cloud targets stay blocked.

### Changed
- `delegate_session` results carry `child_session_id`, and `delegation_id` now equals the child session id; delegation lifecycle events (`delegation_spawned`/`completed`/`failed`) include the child session id.
- Blocking delegations are no longer killed by the flat 30s tool timeout (they get the delegation timeout plus margin).

### Fixed
- A delegation whose awaiter subscribed after completion no longer burns its full timeout (results were silently dropped by the watch channel when no receiver existed yet).
- Ending, archiving, or deleting a session now cleans up its delegated children (previously orphaned background tasks kept running).

## [0.13.0] - 2026-07-22

### Added
- **Remote terminal (opt-in, fail-closed)**: `cru web --remote-shell` (or `[server] remote_shell = true`) serves the PTY terminal to authenticated non-localhost clients. Requires an API key — without one the opt-in is ignored and the terminal stays loopback-only. The WebSocket origin guard now accepts same-origin upgrades (any LAN IP/hostname the server answers as) while still rejecting cross-site origins; `/api/config` reports the opt-in so remote clients get an explanation instead of a dead reconnect loop.
- **Terminal fidelity overhaul**: WebGL renderer with vector-drawn powerline/box-drawing glyphs (no more seams from font fallback), a coherent warm-ember 16-color ANSI palette as first-class `--color-term-*` design tokens, `COLORTERM=truecolor` on the PTY (prompts stop downgrading to 256-color), Unicode 11 width tables, a 4.5:1 minimum contrast floor, and a screen-reader text layer. Terminal font family and size are configurable (Settings → Terminal) and apply live; the PTY starts in the server's launch directory instead of `$HOME`.
- **Single-purpose palettes**: Ctrl+P is a pure command palette (every registered panel gets an "Open …" command, so closed windows — graph, terminal, backlinks — can always be brought back); Ctrl+O is a dedicated note quick switcher (recency-sorted, path subtitles, path-segment fuzzy matching). `[[` and `>` cross between the two mid-typing.
- **Backlinks show the referencing block**: each linked mention renders the line that contains the wikilink (exact occurrence from the daemon's link index, which now returns byte spans in `get_backlinks`), and hovering a backlink opens the preview scrolled to that referencing section in both reading and editor modes.
- **Frontmatter Properties card**: YAML and TOML frontmatter render as a structured card in live preview and reading view instead of raw text (TOML frontmatter previously leaked into the body); the editor opens with the cursor past the frontmatter, and touching the card reveals the raw source.
- **Graph local mode**: BFS-scoped view (1–3 hops around the focused note), external `scheme://` link filtering, and degree-aware clustering forces.
- **Obsidian-style shell chrome**: raised-chip active tabs, edge panels that slide in/out with the neighboring content reflowing in step (the panel's own content never squishes), always-visible edge ribbons carrying panel toggles, palette, new-session, and settings.

### Changed
- **The header bar and status bar are gone.** The ribbons and a floating bottom-right chip cluster replace them: notification bell with an Adobe-style popout panel, an attention chip when something is waiting, and the (setting-gated) save chip for dirty buffers. Chat mode lives in each composer; the Inbox is reachable from the palette and the attention chip.
- **One code palette everywhere**: rendered code blocks (reading view, chat) switched from github-dark to one-dark-pro, matching the CodeMirror editor and live preview.
- **Live preview ↔ reading parity**: matched typography scale, heading colors, code/blockquote/table surfaces, and column gutters between the two views.
- Session rows fade out long titles and marquee them on hover (ping-pong) instead of truncating; delete is red; hover actions no longer overlay the title.
- Hover previews hug their anchor and ignore links crossed while the pointer travels into the window (no more preview hijacking by the link below).
- The home page is gone — a fresh shell opens to an empty workspace; users compose their own layout from panels.

### Fixed
- Service-worker update prompt is actionable (a "Reload & update" toast that never auto-dismisses) — deploys no longer stranded stale bundles silently.
- Terminal background matched to the dock; "No LLM providers detected" no longer flashes during the provider probe.
- Backlinks panel no longer retargets while hovering wikilink previews (hover buffers open in the background).
- xterm's accessibility layer no longer swallows clicks on the reconnect button.

### Docs
- The docs kiln was pruned to product documentation (~290 internal analysis/research/planning notes and session recordings removed).

## [0.12.0] - 2026-07-21

### Added
- **Unified session creation on the web (draft surface)**: one "new session" flow replaces the old dialog. A draft chat panel opens instantly with scope chips for kiln, workspace, agent, and model; the real daemon session is created lazily on the first message. The first message is handed off in-memory (never persisted with tab layout), so a reload can't re-send it.
- **ACP agents from the web**: pick Claude Code, OpenCode, Gemini CLI, etc. right in the draft panel. `agents.list_profiles` now probes each agent's availability so unavailable ones are grayed out, and the daemon resolves the profile server-side — an unknown agent name errors without creating a session.
- **Kiln-less sessions**: the kiln is now optional everywhere (web, CLI, RPC). Omitted, the daemon resolves its home-kiln default in exactly one place — clients never pre-empt it.
- **Multi-kiln sessions**: attach extra knowledge kilns at creation (`connect_kilns`) or mid-session via new `session.connect_kiln` / `session.disconnect_kiln` / `session.set_workspace` RPCs and interactive scope chips. Attach re-runs data-classification trust checks (and the check runs *before* the kiln is opened, so a rejected attach leaves no trace); detach is always safe; the primary kiln is immutable. The `semantic_search` tool now fans out across the primary plus all connected kilns through the same engine precognition uses, labeling results with their source kiln.
- **Transcript convergence**: message ids are backend-canonical (turn id from send; assistant = `{id}-response`; segments = `{id}-seg-N`), tool calls are first-class transcript entries rendered as grouped blocks in chronological order (user → tools → answer), and the daemon emits a persisted `segment_complete` event at each text→tool boundary — so live viewers, second panes, and reloads all render byte-identical transcripts, including turns where the agent narrates between tool calls.

### Changed
- **The daemon now owns default-agent resolution**: `session.create` accepts an optional agent spec and resolves provider/model/endpoint (or an ACP profile) server-side, configuring the agent as part of create. Web sessions now default to the *configured* default provider (same as the CLI) instead of the first detected one.
- Session scope mutations claim the session's request slot atomically — a scope change and an in-flight turn exclude each other in both directions.
- `semantic_search` results dedup per note across kilns (highest score wins), matching precognition's merge policy.

### Fixed
- Streaming no longer recreates the whole transcript on every token — expanded tool cards stay open while the agent streams, and markdown renders once per message instead of per token.
- Narration before a tool call ("Let me look that up…") no longer renders twice in the final answer bubble on text→tool→text turns.
- Tools left `running` when a turn completes or errors are finalized instead of spinning forever.
- Kiln/project pickers in the scope chips close on Escape and outside click; the draft panel autofocuses its message box.
- Kiln-less session creation respects an embedded server's injected data root instead of always using the process-global home directory.

### Added
- **Project files in the web file-tree**: opening a file from a project root (README, source, configs — anything outside an attached kiln) no longer 404s. `/api/kiln/file` (and a new raw-bytes `/api/file/raw`) resolve files within a registered project too, reusing the daemon's project allowlist. Governed by a new `project_files` policy in `.crucible/project.toml` `[security]` — `read-write` (default), `read-only`, or `off`. Kiln notes remain always read-write.
- **Rich document rendering (web editor)**: reading view renders embedded HTML (DOMPurify-sanitized) — a README's centered `<p align="center">` demo now displays — with a hover copy button on code blocks. Images render in **both** reading view and the live-preview editor, and relative image srcs (e.g. `assets/demo.gif`) load through the raw-file endpoint. Chat/hover previews keep HTML disabled.
- **More editor syntax highlighting**: TOML, JSON, Python, Go, shell, CSS, HTML, YAML, and the long tail now highlight in the whole-file editor via lazily-loaded `@codemirror/language-data` grammars (previously only md/js/ts/rust).
- **Task-list checkboxes**: GFM `- [ ]` / `- [x]` render as styled checkboxes in reading view and live-preview; clicking a live-preview checkbox toggles the source. Colored list markers; completed items dim and strike through.
- **Colored filetype icons** (VSCode/seti-style) in the file tree — per-extension icon + hue.

### Fixed
- Web model picker prefixed every model with the provider's wire *type*, so any OpenAI-compatible endpoint (a local GLM server, an OpenRouter/Z.AI gateway) showed "openai/…" for all models. Now shows the model id as-is.
- Web file-tree root selector no longer lists the same kiln twice (name-vs-path aliasing is resolved and deduped).
- Badge/image spacing in the reading view: consecutive badge lines flow inline instead of stacking with large gaps.

## [0.11.3] - 2026-07-20

### Changed
- Callouts now render as full admonition blocks (icon, colored title row, tinted body) inside the live-preview editor — matching reading mode and the way tables already render — instead of only tinting the raw source lines. Foldable `-`/`+` callouts render as collapsible `<details>` (clicking the title toggles the fold without dropping into the source); clicking a callout body, or moving the cursor in (including vim `j`/`k`), reveals its raw markdown for editing.

## [0.11.2] - 2026-07-20

### Fixed
- File-tree icons align: file rows now take the chevron-width indent step that folder rows spend on their disclosure chevron, so icons and names line up within a level.
- Revealed table source (live preview) rendered at the full prose font size after the header-alignment fix, and its background tint stopped at the readable-column edge while wide rows kept going; the source now keeps its compact size and the tint covers the full overflowing row.

## [0.11.1] - 2026-07-20

### Added
- **Knowledge graph view** (web): an Obsidian-style interactive graph of the kiln — notes as uniform nodes with collision spacing, wikilinks as edges, unresolved targets as ghost nodes, optional tag nodes. Smooth canvas force layout with zoom-to-cursor, pan, node dragging, hover neighborhood highlighting with eased fade-in labels (hover-only), click-to-open, and a persisted settings card (search filter, tags/unresolved/orphans toggles, display + physics sliders). Backed by a new `kiln.graph` RPC / `GET /api/kiln/graph` over the resolved link index.
- **Callouts**: `> [!note] Title` blockquotes render as colored admonition blocks across reading mode, chat, and hover previews — all 13 Obsidian variants plus aliases, foldable `-`/`+` forms, icons, and live-preview tinting. Documented in `Help/Callouts`.
- **Editor code highlighting**: fenced ` ```lang ` blocks now highlight inside the live/source editor (grammars lazy-load per language); reading mode already highlighted via shiki.
- **Table editing**: entering a rendered table auto-aligns its source into a monospace, non-wrapping column grid (alignment markers preserved) and re-tidies on exit; vim `j`/`k` and other vertical motions now move *into* rendered tables instead of skipping over them.
- **New-session chooser**: creating a session now offers kiln and project-workspace selection with defaults prefilled (Enter keeps the one-keypress fast path).
- `scripts/sanity-web.sh`: post-install smoke check (binary, daemon socket ownership, UI/assets, graph API, LAN reachability, remote-API auth enforcement).

### Changed
- Sessions now dock as tabs in the **right edge panel** (auto-expanding) instead of splitting the center tiling; persisted layouts migrate on load (center chat tabs move right, emptied panes collapse, legacy session-less chat tabs are pruned).

### Fixed
- Kilns indexed before the resolved-link index existed had permanently empty graphs/backlinks; the relink pass now also fires for them.
- Frontmatter `tags:` are now indexed alongside inline `#tags` (tag search and graph tag nodes were empty for frontmatter-tagged kilns).
- Table header rows drifted out of column alignment while editing (bold header tokens are metrically wider; revealed table lines now pin font metrics).
- `[[...]]` inside code blocks, inline code, and frontmatter is no longer treated as a wikilink (TOML `[[table]]` headers were getting link pills and bracket hiding).
- File-tree folder chevrons now rotate on expand; tab bars no longer compress below their intended height; several panels rooted at a mismatched background tone were unified.
- Duplicate "no active session" notices in the chat panel reduced to one.

## [0.11.0] - 2026-07-19

### Added
- **Wikilink link integrity** (file-tree Phase 3): a deterministic resolved-link index (`note_links` v2 — resolution computed at index time and persisted per occurrence with byte spans) replaces fuzzy query-time backlink matching. New `note.rename`/`note.move` RPCs rewrite exactly the unambiguous inbound links by byte-span splice (aliases, `#heading`/`^block` refs, embeds, and the author's bare-vs-path link style all survive); ambiguous stems are never touched and are surfaced as warnings. Moving a note into or out of folders converges the index no matter how the file moved — every note add/remove/title-change re-resolves affected links.
- **File-tree drag-and-drop** (Phase 2): drag any tree row onto a folder (or the tree root) to move it on disk, onto an editor pane to open it there, or into editor text to insert a `[[wikilink]]` (kiln notes) or relative path — one drag, three targets, innermost wins. Kiln `.md` moves route through the link-rewrite pipeline, so drags never break links. Built on native HTML5 drag-and-drop (pragmatic-drag-and-drop).
- **Right-click menus**: tree rows gain Rename (inline, link-safe), New note, New folder (`fs.mkdir`), and Delete (`fs.trash` → `.crucible/trash/`, recoverable); tabs gain Close / Close Others / Close to the Right; editors gain clipboard actions. Shift+right-click and images/links always fall through to the browser menu so Copy Image / Save As keep working.
- New daemon RPCs: `fs.move`, `fs.mkdir`, `fs.trash`, `note.rename`/`note.move` — all fail-closed (registered projects / already-open kilns only, canonicalize-and-contain, overwrite refusal).

### Changed
- **`crucible-web` crate**: the web UI server (Axum routes + embedded SolidJS frontend) moved out of `crucible-cli` into its own crate behind a default-on `web` cargo feature; `--no-default-features` builds a slim CLI. Release binaries still embed the web UI.
- Backlinks now read the resolved-link index (exact, deterministic) instead of fuzzy stem/title matching.
- Test-suite consolidation: shared server/agent test fixtures and parametrized suites (~1,150 lines removed, coverage unchanged).

### Fixed
- Lazily loaded project folders in the file tree rendered empty (loaded children were discarded instead of persisted).
- Center-pane opens (file click, palette, drops) silently did nothing on layouts carrying a stale tab-group reference; the group is now materialized on demand.
- A rename round-trip (A→B→A) could skip re-indexing at the destination due to stale-but-identical change-detection state; renames now force the reindex.

## [0.10.1] - 2026-07-18

### Added
- **Web file-tree explorer** (Phase 1): hierarchical file tree with a top-right kiln/project root dropdown, live file-change updates over a new `/api/fs/events` SSE channel, keyboard/ARIA navigation, sort, collapse-all, reveal-active, and a read-only context menu. Backed by a new `fs.list_dir` daemon RPC (registry-allowlisted, symlink-contained, dotfiles/gitignored hidden by default).
- **Custom font selection**: choose the UI font (`--font-sans`) and code font (`--font-mono`) in Settings — presets (IBM Plex, System, Serif) or a custom CSS font-family, applied live.
- **Markdown caching**: Parse results cached between frames, keyed on content + terminal width
- **`cru.storage` Lua API**: Plugin-namespaced EAV properties for structured data
- **Precognition daemon setting**: `precognition.results` wired as session-scoped config
- **Note path normalization**: Daemon normalizes to kiln-relative paths at ingest
- **SQLite v2 migration**: Note path dedup and schema versioning
- **Prompt caching**: Anthropic prompt caching enabled via genai CacheControl
- **Execution limits**: Context management, agent undo, output validation
- **CLI-recorded parity test**: JSONL fixture captured from real session, replayed through test framework to catch rendering divergence
- **Spacing acceptance tests**: 10 tests exercising the live rendering path (drain_graduated + viewport) covering user→assistant, tool→tool, thinking→tool, and multi-frame graduation transitions
- **Plugin install**: `plugins.toml` declaration + git bootstrap on daemon startup; `cru plugin add/remove/update` CLI commands
- **LuaCATS auto-ship**: Type stubs auto-generated at `~/.config/crucible/luals/` on daemon start for IDE autocomplete
- **Declarative schedules**: `[[schedules]]` section in `crucible.toml` with human-readable intervals (`1h`, `30m`, `5s`)
- **Fuzzy finder**: nucleo-backed fuzzy matching replaces substring filtering in all autocomplete; `:pick` command for full-screen picker
- **`session.fork`**: RPC method + `cru.sessions.fork(id, opts)` Lua API to branch conversations
- **`session.messages`**: `cru.sessions.messages(id, opts)` Lua API to read conversation history with role/limit filtering
- **`session.inject`**: `cru.sessions.inject(id, role, content)` inserts messages into live session context
- **`subagent.collect`**: `cru.sessions.collect_subagents(ids, timeout?)` awaits multiple subagents with shared deadline
- **`lua.eval`** RPC + `cru lua` CLI command with `=expr` Neovim convention
- **Auto-linking**: `suggest_links` RPC detects unlinked note mentions via word-boundary matching
- **Webhook API**: `POST /api/webhook/:name` receives payloads, broadcasts `webhook:received` event for Lua handlers
- **API auth**: HTTP auth middleware with auto-generated key (`~/.config/crucible/api_key`), constant-time comparison, localhost bypass with X-Forwarded-For awareness
- **Scheduled Lua hooks**: `cru.schedule({every=N}, fn)` with `cru.schedule.cancel(handle)` and 256-schedule limit
- **Runtime plugin infrastructure**: `PluginSource` provenance tracking (user/runtime/kiln/env-path); `plugin.list` RPC includes source/version
- **Clean Lua error messages**: `format_lua_error()` strips FFI frames, prepends `[plugin_name]`
- **`:help` categories**: `:help commands`, `:help keys`, `:help config`, `:help tools`
- **"Did you mean?"** suggestions for unknown REPL/slash commands via Levenshtein distance
- **`cru doctor`** enhancements: plugin health check, config validation
- E2E ACP delegation pipeline test
- Diagnostic logging for MCP transport negotiation
- Strict content checks in `validate-demos.sh`

### Changed
- **Unified Taffy spacing**: Single spacing system via Taffy `gap()` for both graduated (stdout) and viewport content; drain-based graduation at app layer replaces key-tracked `GraduationState`
- **Terminal scrollback for history**: Graduated content writes to stdout; removed PageUp/PageDown viewport scrolling in favor of terminal emulator native scrollback
- **Unified event processing**: Removed `is_replay` branching; session resume uses same event path as live streaming
- **Wall-clock spinner**: Spinner animation uses `Instant::elapsed()` instead of tick count for consistent animation during rapid streaming
- **Model prefetch**: Models fetched at TUI startup; `:model` opens popup directly
- Autocomplete filtering uses nucleo fuzzy scoring instead of substring matching
- `lua.eval` RPC returns proper RPC errors instead of `{"error": ...}` JSON
- `collect_jobs` uses shared deadline across all jobs (was per-job timeout)
- Demo pipeline: modernized VHS tapes and justfile recipes
- Regenerated all demo GIFs via VHS

### Fixed
- **IBM Plex webfont never loaded**: the `@fontsource` `@import`s sat after `@import "tailwindcss"`, so they were invalid CSS and the bundler dropped every `@font-face` — the web UI silently rendered in `system-ui` (OS-dependent). Reordered the imports so the designed font actually loads on every platform.
- **TUI spacing**: Consistent 1 blank line between all container types; consecutive tool groups tight (no gap); thinking summary spaced from text below it
- **Code block spacing**: Eliminated extra blank lines in code blocks; code renders as single text node with embedded newlines
- **Ordered list numbering**: Lazy list merging and incremental numbering across tool boundaries
- **Tool graduation**: Individual tool calls graduate independently instead of waiting for entire group
- **Thinking block rendering**: Collapsed thinking summary stops spinner when text starts streaming; correct ordering and contrast; shared graduation key between collapsed/expanded
- **Bullet character**: Configurable via `theme.decorations.bullet_char`
- **Tool output spilling**: Moved to daemon with env var injection; correct line count and summary
- **`list_notes`/`search_notes`**: Treat LLM-sent `folder="null"` string as None instead of constructing invalid path
- **Precognition dedup**: Deduplicate notes by normalized filename, not display title
- **Bounded overflow indicator**: Auto-detect indent level
- API key file written with `0o600` permissions (was world-readable)
- API auth: constant-time key comparison (prevents timing attacks)
- API auth: checks X-Forwarded-For to prevent proxy bypass
- Deduplicated `inject_context` logic between RPC handler and Lua bridge
- Role filter validation in `load_messages` (rejects invalid roles with error)
- Auto-link UTF-8 safety guard (returns empty for non-ASCII-safe text instead of wrong offsets)
- Max 256 active scheduled tasks (prevents resource exhaustion)
- Empty plugin names from malformed URLs now rejected
- Zero-duration schedules rejected with actionable config error
- `session.fork` copies parent agent configuration (model, provider, etc.)
- `delegate_session` filtered from `list_tools` when unavailable
- Real providers passed to ACP agent MCP server

### Removed
- **`Node::Static`**: Removed variant, `StaticNode`, `ElementKind`, `GraduationState`, `GraduatedContent`, `scrollback()` builders from crucible-oil
- **Viewport scrolling**: PageUp/PageDown/End keybindings, `scroll_offset` field, `↑NL` status indicator
- **Legacy renderer**: Removed non-Taffy rendering path; all rendering unified on Taffy pipeline
- **Unused node variants**: `ErrorBoundary`, `Focusable` removed from `Node` enum
- **Decrypt animation module** removed from crucible-oil
- Dead `CrdtManager` (142 LOC) and `CanvasNode`/`CanvasEdge` (123 LOC) code stubs
- `yrs` workspace dependency (only used by removed CRDT module)
- Stale `TODO: METHODS array is incomplete` comment
- Duplicate `#[test]` attribute in config includes tests

## [0.9.0] - 2026-07-10

### Added
- **Full-visibility permission prompts**: the permission modal shows the entire bash command / tool arguments, word-wrapped — never truncated. `:set perm.full_commands=false` restores the compact one-line view
- **`cru.config` store wired end to end**: `:set` values mirror into the daemon config store; Lua plugins read the same values via `cru.config.get`
- **`:lua <expr>` / `:=` escape hatch** on the TUI command line (evaluated daemon-side)
- **nvim-style minimal completion popups** for inline (`@` file, `[[` note) triggers, anchored at the word being completed; `:set completion_style=auto|panel|minimal`

### Fixed
- TUI: top screen row no longer freezes during long streaming turns
- TUI: narrow terminals — status/row content shrinks and ellipsizes instead of dropping off-grid
- TUI: `:set theme` actually switches syntax highlighting; `:set theme&` reverts to the config-seeded theme
- TUI: `--set` startup overrides now reach the daemon, and drafts typed while streaming are no longer lost
- TUI: `:model` reliably lists models (startup prefetch was never spawned)
- TUI: completion popup surface tracks the prompt background; no background painted on blank filler rows; end-of-line cursor sits after the text
- TUI: `:set` routes through the shared CLI classifier — the TUI and `--set` accept the same keys
- Web: SSE handlers map the daemon's real event shapes (status, error envelopes); tests pin the wire contract
- Web: `/api/layout` GET/POST/DELETE served; auto-title reads the real history shape
- Web/daemon: keyless custom endpoints (e.g. local llama.cpp) work as session defaults
- Daemon: `plugin.install`/`plugin.remove` advertised in `daemon.capabilities`
- Docs site: deploy fixed (stale sidebar slug had failed every deploy since April)

### Changed
- **~16k net LOC removed**: dead Lance store implementations, manual flex-layout engine, `CompletionBackend`, legacy accessors, dead trait clusters, orphaned snapshots and fixtures
- **Test infrastructure overhaul**: the fictional feature-based test-tier system is gone (nextest profiles are timeout presets, not filters); RPC parity gates cover all 14 session config knobs; mock daemons deduplicated into a canonical `web/test_support`; vt100 screen-level regression suites for spinner leaks and spacing

## [0.4.0] - 2026-03-19

### Added
- Per-agent permission profiles via `[acp.agents.<name>.permissions]`
- `--permissions` CLI flag and `CRUCIBLE_PERMISSIONS` env var for headless sessions
- Shell completion generation for bash and zsh (`cru completions`)
- Top-level `cru search` command with `-f json` output
- JSON output format (`-f json`) for `cru stats`, `cru models`, `cru skills`, `cru tools`, `cru doctor`
- `CRU_SESSION` env var support for all session commands
- LLM-powered session auto-titling
- Session auto-archive with configurable `auto_archive_hours`
- Thinking positional rendering with `Ctrl+T` streaming filter
- Compact tool display format with render blocks and Lua display hooks
- Lua `tool:display_start` and `tool:display_complete` handler types
- Daemon auto-discovery of LLM providers with classification filtering
- Connected kiln names injected into agent system prompt
- Shipped `defaults/init.lua` with precognition format and session hooks
- Multi-session web UI with tab management and file explorer
- E2E Playwright tests for web UI

### Changed
- Config split: `kiln.toml` and `project.toml` replace monolithic `workspace.toml`
- Session commands: renamed `unpause` → `resume`, `resume` → `open`
- Grounding-first default system prompt replaces size-tiered prompts
- Demo pipeline: VHS tapes replace asciinema, `glm-4.7-flash` model
- ACP: table-driven built-in profile initialization
- RPC client: `daemon_*` prefix → `rpc` submodule

### Fixed
- ACP MCP transport fallback (retry with stdio when HTTP rejected)
- Permission gating for headless sessions (`is_interactive` threading)
- CORS restricted to explicit origin allowlist
- Symlink traversal validation for web file operations
- Async file write flushing in session storage
- UTF-8 panic in thinking truncation
- Web: double-prefix in model display, provider detection for defaults
- Config: `{env:VAR}` template resolution in CLI config loading

## [0.3.0] - 2026-03-08

### Added
- **Error handling**: `BackendError::is_retryable()` and `retry_delay_secs()` for typed transient failure classification
- **Daemon retry**: `DaemonClient::call_with_retry()` with exponential backoff on timeout errors for idempotent RPC methods
- **File reprocessing**: Daemon automatically re-parses and re-indexes files on change via `file_changed` events
- **Kiln path lookup**: `find_kiln_for_path()` with longest-prefix matching for nested kiln support
- **CLI long help**: All 10 commands now have detailed `--help` with usage examples
- **Setup wizard**: First-run TUI wizard auto-triggers on `cru chat` when no kiln exists
- **ACP host**: Spawn and control external AI agents (Claude Code, Codex, Gemini CLI) with Crucible's memory and permission system
- **Precognition**: Auto-RAG injects relevant vault context before each agent turn (`:set precognition on`)
- **Session search**: Past conversations indexed and searchable; `cru session reindex` for batch processing
- **Interaction modals**: All 7 InteractionRequest variants (Ask, AskBatch, Edit, Show, Permission, Popup, Panel) fully implemented
- **Batch interactions**: Multi-select ask, batch permission prompts with queuing
- **Subagent spawning**: Background job manager for parallel subagent tasks with cancellation
- **Permission system**: Multi-layer permissions with pattern whitelisting and Lua hooks
- **MCP gateway**: Connect upstream MCP servers with prefixed tool names and auto-reconnect
- **Per-session MCP servers**: Agent cards define MCP servers propagated to session agents
- **Lua session API**: Scripted agent control for temperature, max_tokens, thinking_budget, model, mode
- **Plugin error surfacing**: `:plugins` command shows load status; failures as toast notifications
- Initial open-source release
- MIT + Apache 2.0 dual licensing
- GitHub Actions CI
- Contributing guidelines
- Lua plugin system with manifest-based lifecycle management
- `CRUCIBLE_PLUGIN_PATH` environment variable for custom plugin directories
- ViewportCache with configurable max items (`with_max_items()`)

### Changed
- **BREAKING**: Renamed `crucible-ink` crate to `crucible-oil` (Obvious Interface Language)
  - Update imports: `crucible_ink::*` → `crucible_oil::*`
  - TUI module path: `tui::ink::*` → `tui::oil::*`
- ACP protocol version bumped from 0.7.0 to 0.10.6
- Daemon connection errors now include recovery suggestions

### Fixed
- Crash-risk `unwrap()` sites in CLI: home dir fallback, guarded strip_prefix, enricher clone
- Provider detection uses config/env instead of HTTP probes
- UTF-8 panic, popup backspace fallthrough, and Action::Send chaining in TUI
- Markdown parser: LazyLock for regexes, frontmatter edge cases
- Rig: eliminated unwrap panics and lossy token casts
- Config: hardened credentials and profile loading

### Testing
- 12 snapshot tests: thinking display, context usage, subagent events, precognition, multi-turn tools, error interrupts, stream cancellation
- 10 interaction tests: BackTab mode cycling, `:set` commands, notification lifecycle, model loading popup states
- 3 subagent property tests with proptest generators for panic-freedom and state corruption detection
- 2 `call_with_retry` tests verifying retry/no-retry behavior
- 4 `find_kiln_for_path` unit tests

## [0.1.0] - 2025-12-19

Initial development version.

### Added
- Core knowledge management system with wikilink-based graphs
- Markdown parser with frontmatter support
- Block-level embedding generation
- Semantic, fuzzy, and text search
- SurrealDB storage with EAV graph schema
- MCP server for AI agent integration
- CLI interface (`cru`)
- Unified LLM provider system (Ollama, OpenAI, FastEmbed, LlamaCpp)
- Lua/Fennel scripting integration
- File system watching for incremental updates
- TOON Query (tq) - jq-like query language
