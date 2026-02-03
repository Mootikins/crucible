# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
