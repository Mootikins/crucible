# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
- **API auth**: Bearer token middleware with auto-generated key (`~/.config/crucible/api_key`), constant-time comparison, localhost bypass with X-Forwarded-For awareness
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
- Bearer auth constant-time token comparison (prevents timing attacks)
- Bearer auth checks X-Forwarded-For to prevent proxy bypass
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
