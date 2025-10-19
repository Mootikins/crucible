# Crucible PoC: Terminal-First Architecture

> **Status**: Active Development (2025-10-19)
> **Goal**: Build a queryable knowledge layer for terminal-centric workflows

## Vision

Crucible is a **queryable intelligence layer** that sits between your plaintext vault and both you (via CLI) and AI agents (via MCP). It is NOT a text editor or GUI application.

**Core Principle**: Editor-agnostic. Use nvim, VSCode, or any editor. Crucible provides the knowledge infrastructure.

## What We're Building (PoC Scope)

### Daemon with TUI

A terminal application that:
- Watches vault files continuously
- Parses markdown, frontmatter, wikilinks, tags
- Indexes content to SurrealDB
- Displays rolling logs of indexing activity
- Provides REPL for queries and tool execution

### Architecture

```
File System
    ↓
┌─────────┐
│ Watcher │ (notify-debouncer)
└────┬────┘
     ↓
┌─────────┐
│ Parser  │ (frontmatter, wikilinks, tags)
└───┬─┬───┘
    │ │
    │ └──────────┐
    ↓            ↓
┌──────────┐  ┌────────┐
│SurrealDB │  │ Logger │ (tracing)
└────┬─────┘  └───┬────┘
     │            │
     │            ↓
     │    ┌──────────────┐
     │    │  TUI (ratatui) │
     │    │  ┌──────────┐ │
     │    │  │   Logs   │ │
     │    │  ├──────────┤ │
     │    │  │   REPL   │ │
     │    │  └──────────┘ │
     │    └──────┬────────┘
     │           │
     └───────────┴──────────┐
                            ↓
                    ┌────────────────┐
                    │ Tools/Rune     │
                    │ - search       │
                    │ - metadata     │
                    │ - semantic     │
                    │ - custom.rn    │
                    └────────────────┘
```

### Data Flow

1. **File Change** → Watcher detects modification
2. **Parser** → Extracts structured data (frontmatter, links, tags)
3. **Dual Output**:
   - **SurrealDB** → Indexes content for queries
   - **Logger** → Emits trace events
4. **TUI Display** → Shows logs in rolling window
5. **User Interaction** → Types queries/commands in REPL
6. **REPL Execution** → Routes to SurrealDB, Tools, or Rune scripts

## Key Design Decisions

### 1. Logging is First-Class

- Use `tracing` crate throughout
- Display logs in rolling window (configurable, default 15-20 lines)
- Persist to `~/.crucible/daemon.log` for debugging
- Log levels: TRACE, DEBUG, INFO, WARN, ERROR
- **Why**: Data flow visibility is critical for debugging indexing

### 2. SurrealQL Pass-Through (No Abstraction)

- Direct SurrealQL queries in REPL
- Full access to graph traversal, recursion, advanced features
- Accept coupling to SurrealDB for iteration speed
- **Why**: Flexibility > portability during PoC phase

### 3. REPL as Command Center

The REPL handles three input types:

**Built-in Commands** (`:` prefix):
- `:tools` - List available tools
- `:run <tool> <args>` - Execute tool directly
- `:rune <script>` - Run Rune script
- `:stats` - Vault statistics
- `:config` - Show configuration
- `:log <level>` - Set log level
- `:help` - Show help
- `:quit` - Exit daemon

**SurrealQL Queries**:
```sql
SELECT * FROM notes WHERE tags CONTAINS '#project';
SELECT ->links->note.title FROM notes WHERE path = 'foo.md';
```

**Tool Execution**:
```
:run search_by_tags project ai
:run semantic_search "agent orchestration"
:rune custom_query.rn
```

### 4. Tool Architecture

**Direct Execution** (not MCP protocol overhead):
- Built-in Rust tools (search, metadata, etc.)
- Rune scripts as first-class tools
- Tools call SurrealDB directly
- **Why**: Simpler, faster, no protocol marshaling in REPL

**MCP Server** (separate component, future):
- HTTP/SSE transport
- Exposes same tools via MCP protocol
- Uses same SurrealDB backend
- Allows agents to query vault remotely
- **Why**: Clean separation - REPL for humans, MCP for agents

## TUI Layout

```
┌─────────────────────────────────────────────────────────┐
│ Crucible Daemon v0.1.0 | SurrealDB | 43 docs | 2.3MB    │
├─────────────────────────────────────────────────────────┤
│ LOGS (rolling window, last 15 lines)                    │
│ 12:34:56 INFO  Watcher started on ~/vault               │
│ 12:34:57 DEBUG Indexed Projects/crucible.md (23ms)      │
│ 12:34:58 DEBUG Extracted 3 links, 2 tags                │
│ 12:35:01 INFO  File changed: tasks/plan.md              │
│ 12:35:01 DEBUG Re-parsing...                            │
│ 12:35:01 DEBUG Re-indexed (89ms)                        │
│ 12:35:02 INFO  Embeddings updated (243ms)               │
│                                                          │
├─────────────────────────────────────────────────────────┤
│ REPL (SurrealQL | :tools | :help)                       │
│ > SELECT path, title, tags FROM notes                   │
│   WHERE tags CONTAINS '#project'                        │
│   ORDER BY modified DESC LIMIT 5;                       │
│                                                          │
│ ┌────────────────┬─────────────┬──────────────────────┐ │
│ │ path           │ title       │ tags                 │ │
│ ├────────────────┼─────────────┼──────────────────────┤ │
│ │ Projects/cr... │ Crucible    │ #project, #ai, #rust │ │
│ │ tasks/poc.md   │ PoC Plan    │ #project, #planning  │ │
│ └────────────────┴─────────────┴──────────────────────┘ │
│                                                          │
│ > _                                                      │
└─────────────────────────────────────────────────────────┘
```

## Implementation Roadmap

### Phase 1: Foundation ✅
- [x] Fix crucible-watch build error
- [x] Document architecture

### Phase 2: Daemon Core
- [ ] Implement TUI with ratatui
- [ ] Rolling log window display
- [ ] Integrate tracing → TUI logs
- [ ] Watcher → Parser → Logger pipeline

### Phase 3: Database Integration
- [ ] SurrealDB connection management
- [ ] Parser → SurrealDB indexing
- [ ] Schema for notes, links, tags
- [ ] Graph edge creation for wikilinks

### Phase 4: REPL
- [ ] REPL input handling (reedline/rustyline)
- [ ] Command parsing (`:` prefix for commands)
- [ ] SurrealQL query execution
- [ ] Result formatting (tables)

### Phase 5: Tool Integration
- [ ] Tool registry (built-in + Rune)
- [ ] `:tools` command
- [ ] `:run` command for tool execution
- [ ] Rune script loader and executor

### Phase 6: Dogfooding
- [ ] Daily usage testing
- [ ] Performance optimization
- [ ] Error handling refinement
- [ ] Documentation

### Future: MCP Server (Post-PoC)
- [ ] HTTP/SSE transport layer
- [ ] MCP protocol implementation
- [ ] Tool exposure via MCP
- [ ] Agent integration testing

## What We're NOT Building (Deferred)

- ❌ Desktop UI / Svelte frontend
- ❌ Canvas mode / infinite zoom
- ❌ GPU acceleration
- ❌ Visual programming interface
- ❌ Real-time collaboration sync
- ❌ Text editor / editing interface
- ❌ Mobile app
- ❌ Web interface

**Use your editor of choice. Crucible is infrastructure, not an app.**

## Success Criteria

A successful PoC means:

1. ✅ Run `crucible daemon` in terminal
2. ✅ Edit notes in nvim/VSCode/any editor
3. ✅ See real-time indexing in log window
4. ✅ Query vault with SurrealQL from REPL
5. ✅ Execute search tools from REPL
6. ✅ Run custom Rune scripts
7. ✅ All without opening a GUI

## Why This Architecture Works

### Addresses Core Challenges

1. **Parsing advanced syntax**: Foundation for dataview-like queries
2. **DB insertion/migration**: Direct SurrealQL access for debugging
3. **Efficient tool exposure**: REPL for user, MCP for agents
4. **Dogfoodable**: Use while building
5. **Terminal-centric**: Fits existing workflow

### Avoids Perfectionism Traps

1. **No UI bikeshedding**: Terminal is the interface
2. **No frontend complexity**: No React/Svelte decisions
3. **No abstraction paralysis**: Direct SurrealQL, accept coupling
4. **Editor-agnostic**: Don't build what exists
5. **Focus on data flow**: Infrastructure, not presentation

## Technical Stack

- **Rust**: Core daemon implementation
- **ratatui**: TUI framework
- **tracing**: Structured logging
- **notify-debouncer**: File watching
- **SurrealDB**: Embedded database
- **Rune**: Scripting runtime
- **reedline/rustyline**: REPL line editor

## Configuration

Daemon uses `crucible-config` crate:
- YAML/TOML/JSON support
- Environment profiles (dev/test/prod)
- Located at `~/.crucible/config.yaml`

Example config:
```yaml
vault_path: "~/Documents/vault"
database:
  backend: "surrealdb"
  path: "~/.crucible/db"
logging:
  level: "info"
  file: "~/.crucible/daemon.log"
  window_lines: 20
repl:
  history_file: "~/.crucible/history"
  prompt: "> "
```

## Related Documentation

- [ARCHITECTURE.md](./ARCHITECTURE.md) - Full system architecture
- [CLAUDE.md](../CLAUDE.md) - AI agent development guide
- [README.md](../README.md) - Project overview

---

**Last Updated**: 2025-10-19
**Status**: In Development
**Next Steps**: Implement Phase 2 (Daemon Core)
