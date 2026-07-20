# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.11.4] - 2026-07-20

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
