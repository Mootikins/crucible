# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### 🚨 BREAKING CHANGES
- **cli!:** Empty search queries now return validation errors instead of showing help
- **core!:** Removed unused orchestrator and router modules causing test timeouts

### ✨ Added
- **cli:** Comprehensive search safety protections
  - Large file memory protection (10MB file limit, 1MB content limit)
  - UTF-8 encoding safety with error recovery and character replacement
  - Input validation with query length limits (2-1000 characters)
  - Memory-efficient streaming reads with 8KB buffers
- **tests:** New `run_cli_command_allow_failure` helper for error case testing
- **docs:** CPU embedding runtime research (Candle, FastEmbed-rs, ONNX Runtime)

### 🛠️ Changed
- **refactor(core):** Removed dead code modules causing test timeouts
  - Eliminated `orchestrator.rs` and `router.rs` modules
  - Reduced core tests from 111 to 91 tests
  - Improved test performance from 60s+ timeouts to 0.06s execution
- **perf(cli):** Optimized file reading with memory protection
- **tests:** Updated empty query test to handle validation errors

### 🧪 Testing
- **cli:** All 12/12 integration tests passing
- **core:** All 91/91 core tests passing in 0.06s with zero timeouts
- **Added:** Large file performance validation
- **Added:** UTF-8 corruption handling tests
- **Added:** Input validation edge case tests

### 📚 Documentation
- **research:** Comprehensive CPU embedding runtime analysis
  - **Candle:** Pure Rust implementation (recommended)
  - **FastEmbed-rs:** Specialized embedding interface (easiest integration)
  - **ONNX Runtime:** Production performance (most complex)
- **models:** Recommended HuggingFace models for CPU inference
  - TaylorAI/bge-micro (17.4MB, 384 dimensions)
  - sentence-transformers/all-MiniLM-L6-v2 (22.7MB, 384 dimensions)

### Changed
- **Documentation Cleanup**: Comprehensive cleanup and consolidation of project documentation
  - Archived 10 legacy documentation files from initial POC phase to `/home/moot/Documents/crucible-testing/Archive/2024-10-20-cleanup/`
  - Archived files include:
    - `A2A Management - Planning.md`
    - `FILES.md` (configuration examples documentation)
    - `PIPELINE_IMPLEMENTATION_SUMMARY.md`
    - `plan-doc-spec-evaluator.md`
    - `POC_ARCHITECTURE.md`
    - `README.md` (archive documentation)
    - `REPL_SUMMARY.md`
    - `SURREALDB_IMPLEMENTATION.md`
    - `SURREALDB_SUMMARY.md`
    - `TUI_DELIVERABLES.md`
  - All content preserved for reference - no documentation was deleted
  - Improved documentation organization and reduced clutter in main repository
  - Focused active documentation on current project state and architecture

### Added
- **CLI Interactive REPL**: Complete port of daemon REPL functionality to CLI crate
  - Interactive REPL with SurrealQL query execution
  - Built-in commands (`:tools`, `:run`, `:rune`, `:stats`, `:config`, `:help`, `:history`, `:clear`, `:quit`)
  - Syntax highlighting for SurrealQL queries
  - Command history with persistent storage
  - Autocomplete for commands and table names
  - Multiple output formats (table, JSON, CSV)
  - Tool execution framework with Rune script support
  - Progress indicators for running queries
  - Rich error formatting with helpful context

- **Default REPL Behavior**: CLI now starts interactive REPL by default
  - Running `crucible-cli` without arguments starts the REPL
  - Global options for REPL customization (--format, --tool-dir, --db-path)
  - More user-friendly and interactive experience out-of-the-box

- **CLI Documentation**: Comprehensive CLI documentation and usage examples
  - Complete command reference with examples
  - Configuration guide and troubleshooting
  - Advanced usage patterns and pipe integration

### Fixed
- **MCP Integration**: Export `create_provider` function from crucible-mcp crate root
  - Resolves "no 'create_provider' in the root" compilation error
  - Enables proper embedding provider creation in CLI commands

### Changed
- **CLI Structure**: Modified CLI to use optional commands with default REPL behavior
  - `arg_required_else_help` changed to false for default REPL
  - Commands now use `Option<Commands>` instead of required subcommands
  - REPL-specific options moved to global flags for consistency

### Features
- **Interactive Search**: Fuzzy search with real-time result filtering
- **Semantic Search**: AI-powered search using embeddings with similarity scoring
- **AI Chat Integration**: Multi-agent chat system with predefined agents (researcher, writer, etc.)
- **Note Management**: Complete CRUD operations for notes with metadata support
- **Vault Statistics**: Comprehensive vault analytics and file information
- **Rune Script Execution**: Run custom Rune scripts as CLI commands
- **Configuration Management**: Hierarchical configuration with environment support

## [0.1.0] - 2024-XX-XX

### Added
- Initial CLI implementation with basic search and note operations
- Tauri desktop application framework
- Core knowledge management functionality
- SurrealDB integration for data storage
- Embedding support for semantic search
- Multi-agent AI integration framework

---

## Migration Guide

### From 0.1.0 to 0.2.0

**Default Behavior Change**:
- **Before**: `crucible-cli` required a subcommand (e.g., `crucible-cli stats`)
- **After**: `crucible-cli` starts the interactive REPL by default
- **Migration**: No breaking changes - all existing commands still work

**REPL Command Removal**:
- **Before**: `crucible-cli repl [options]`
- **After**: `crucible-cli [options]` (starts REPL by default)
- **Migration**: Use `crucible-cli` instead of `crucible-cli repl`

**Global Options Addition**:
- **New**: `--db-path`, `--tool-dir`, `--format` are now global options
- **Migration**: These options now apply to the default REPL and can be used with any command

### Examples

```bash
# Old way (still works)
crucible-cli stats
crucible-cli search "query"
crucible-cli repl --format json

# New default way
crucible-cli                                    # starts REPL
crucible-cli --format json                     # REPL with JSON output
crucible-cli --tool-dir ~/tools --db-path ~/db  # REPL with custom paths
```

---