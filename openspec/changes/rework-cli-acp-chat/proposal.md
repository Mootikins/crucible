# CLI Rework: ACP-Based Chat Interface

## Why

The current CLI is pre-pipeline architecture and needs a fundamental rework to align with the refined architecture:
1. **Old processing code**: Uses scattered processing logic instead of the unified `NotePipeline` orchestrator (5-phase pipeline)
2. **Wrong interface paradigm**: SurrealQL REPL is database-focused, not knowledge-focused
3. **Missing ACP integration**: No natural language interface despite ACP being core to the short-term roadmap
4. **Architecture drift**: Doesn't follow trait-based, plaintext-first principles from ARCHITECTURE.md

The ACP MVP (short-term roadmap) requires a chat interface that spawns external agents and enriches prompts with kiln context - the current CLI cannot support this.

## What Changes

**BREAKING CHANGES:**
- **BREAKING**: Replace SurrealQL REPL with ACP-based natural language chat as default mode
- **BREAKING**: Remove old processing commands (replaced by `process` command using `NotePipeline`)
- **BREAKING**: Change from `crucible-cli` to simpler `cru` command pattern

**New Features:**
- Add `cru chat` command (default) - natural language interface via Agent Client Protocol
- Add `cru process` command - explicit pipeline processing with progress indicators
- Add `cru status` command - kiln statistics and processing metrics
- Refactor `cru search` - use semantic search instead of fuzzy text matching
- Keep `cru config` - already well-designed

**Architecture:**
- Create `CrucibleCore` facade pattern - clean trait-based interface between CLI and core
- Implement ACP client using official `agent-client-protocol` crate
- Add context enrichment - semantic search results injected into agent prompts
- Background processing - pipeline runs on startup unless `--no-process` flag
- File watching integration - auto-process on file changes in watch mode

**Code Cleanup:**
- Remove `commands/repl/` directory - replaced by chat mode
- Remove `commands/fuzzy.rs` - replaced by semantic search
- Remove `commands/diff.rs` - not MVP critical
- Remove all `*.disabled` files - already marked for deletion
- Update dependency on `crucible-pipeline` instead of old processing code

## Impact

### Affected Specs
- **CLI** (new capability) - Define natural language chat interface, pipeline commands
- **Pipeline** (reference) - CLI becomes primary consumer of `NotePipelineOrchestrator` trait

### Affected Code
**Major Changes:**
- `crates/crucible-cli/src/main.rs` - Refactor entry point for new command structure
- `crates/crucible-cli/src/cli.rs` - New clap structure with simplified commands
- `crates/crucible-cli/src/commands/` - Complete rewrite of command modules
- `crates/crucible-cli/src/acp/` - NEW - ACP client implementation
- `crates/crucible-cli/src/core_facade.rs` - NEW - Clean interface to core functionality

**Deletions:**
- `crates/crucible-cli/src/commands/repl/` - All REPL code removed
- `crates/crucible-cli/src/commands/fuzzy.rs` - Replaced by semantic search
- `crates/crucible-cli/src/commands/diff.rs` - Not needed for MVP
- `crates/crucible-cli/src/common/*.disabled` - Clean up disabled code

**Dependencies Added:**
- `agent-client-protocol = "0.6"` - Official ACP Rust implementation
- `rustyline = "13.0"` - Interactive input for chat
- `indicatif = "0.17"` - Progress indicators
- `walkdir = "2.4"` - Directory traversal

### User-Facing Impact
- **Migration required**: Users must switch from SurrealQL queries to natural language chat
- **New workflows**: `cru chat` becomes primary interface instead of database queries
- **Agent dependency**: Requires installing external agent (claude-code, gemini-cli, etc.)
- **Improved UX**: Natural language is more intuitive than SQL for knowledge queries
- **Better architecture**: Clean separation, testable, follows SOLID principles

### Timeline
- **Week 1**: Core refactoring + ACP client + basic chat
- **Week 2**: Complete features + testing
- **Estimated effort**: 1-2 weeks for working MVP
