# Design Document: CLI Rework with ACP Integration

## Context

The current CLI (crucible-cli) was built before the pipeline architecture was refined and uses scattered processing logic instead of the unified `NotePipeline` orchestrator. It also implements a SurrealQL REPL, which is database-focused rather than knowledge-focused.

The **short-term roadmap** (ARCHITECTURE.md) prioritizes:
1. ACP integration (Zed's Agent Client Protocol)
2. CLI chat shell for natural language queries
3. Trait-based extensibility
4. Plaintext-first, editor-agnostic approach

**Stakeholders:**
- End users: Need intuitive natural language interface
- Developers: Need clean, testable architecture
- AI agents: Need structured protocol for interaction (ACP)

**Constraints:**
- Must work with external agents (claude-code, gemini-cli, etc.)
- Must use existing `NotePipeline` orchestrator
- Must follow SOLID principles and trait-based design
- Must maintain offline-first capability

## Goals / Non-Goals

### Goals
1. **Natural Language Interface**: Replace SQL REPL with ACP-based chat
2. **Pipeline Integration**: Use `NotePipeline` for all processing (no scattered code)
3. **Context Enrichment**: Inject semantic search results into agent prompts
4. **Clean Architecture**: Trait-based facade pattern for testability
5. **MVP Timeline**: Working implementation in 1-2 weeks

### Non-Goals
1. ❌ Built-in LLM - agents handle this (ACP architecture)
2. ❌ Custom protocol - use official `agent-client-protocol` crate
3. ❌ Session persistence - defer to post-MVP
4. ❌ Multi-agent orchestration - single agent per session for MVP
5. ❌ Advanced permissions - auto-approve for MVP, add later

## Decisions

### Decision 1: ACP Over Custom Protocol with Agent Discovery

**Choice**: Use official `agent-client-protocol` Rust crate with automatic agent discovery

**Agent Discovery Strategy**:
```rust
// Try known agents in order
const KNOWN_AGENTS: &[(&str, &str)] = &[
    ("claude-code", "claude-code"),
    ("gemini", "gemini-cli"),
    ("codex", "codex"),
];

async fn discover_agent(preferred: Option<&str>) -> Result<String> {
    // Try user's preferred agent first
    if let Some(agent) = preferred {
        if is_available(agent).await? {
            return Ok(agent.to_string());
        }
    }

    // Fallback: try all known agents
    for (name, cmd) in KNOWN_AGENTS {
        if is_available(cmd).await? {
            return Ok(cmd.to_string());
        }
    }

    // None found - error with list of compatible agents
    Err(anyhow::anyhow!(
        "No compatible ACP agent found.\n\
         Compatible agents: claude-code, gemini-cli, codex\n\
         Install one with: npm install -g @anthropic/claude-code"
    ))
}
```

**Why**:
- Mature, maintained implementation (~6 months in production)
- Works with existing agents (claude-code, gemini-cli, codex)
- Handles JSON-RPC, subprocess management, streaming
- Future-proof as protocol evolves
- ~300 lines to implement vs. ~1500+ for custom protocol
- Auto-discovery improves UX - works out of box if any agent installed

**Alternatives Considered**:
- Custom JSON-RPC protocol: More control, but 5x more code + maintenance burden
- Direct LLM integration: Couples CLI to specific providers, violates separation of concerns
- MCP (Model Context Protocol): Wrong layer - MCP is for tools, ACP is for client-agent communication
- Require explicit agent selection: More friction, worse UX

**References**: docs/ACP-MVP.md, docs/ARCHITECTURE.md (line 586-669)

### Decision 2: Facade Pattern for Core Access

**Choice**: Create `CrucibleCore` facade with trait-based interfaces

**Structure**:
```rust
pub struct CrucibleCore {
    pipeline: Arc<dyn NotePipelineOrchestrator>,
    storage: Arc<dyn EnrichedNoteStore>,
    semantic_search: Arc<dyn SemanticSearchService>,
    config: Arc<CliConfig>,
}
```

**Why**:
- Single initialization point - easier testing
- Trait-based dependencies - mockable and swappable
- Hides complexity from command modules
- Follows Dependency Inversion Principle (SOLID)
- Clean interface boundary between CLI and core

**Alternatives Considered**:
- Direct core imports in commands: Tight coupling, hard to test
- Service locator pattern: Hidden dependencies, harder to reason about
- Global state: Thread-safety issues, violates functional principles

**References**: docs/ARCHITECTURE.md (line 10-17), clean architecture principles

### Decision 3: Background Processing Strategy

**Choice**: Spawn processing task on startup (unless `--no-process`)

**Flow**:
```rust
async fn main() {
    let core = initialize_core(&cli).await?;

    // Background processing
    if !cli.no_process {
        tokio::spawn(async move {
            run_pipeline_scan(core.clone()).await
        });
    }

    // Main command executes immediately (doesn't wait)
    match cli.command {
        Commands::Chat => chat_mode(core).await,
        _ => {}
    }
}
```

**Why**:
- Responsive UX - commands start immediately
- Eventual consistency - database updates in background
- Graceful degradation - errors don't block CLI
- User control - `--no-process` for quick commands
- Performance - incremental processing via Merkle diff

**Alternatives Considered**:
- Synchronous processing: Blocks CLI startup (2-5s for large kilns)
- No auto-processing: Users must manually run `cru process` (friction)
- Daemon process: Complexity, resource usage, harder debugging

**Trade-offs**: Commands may operate on slightly stale data if files just changed

**References**: docs/ARCHITECTURE.md (line 553-574 on file watching)

### Decision 4: Context Enrichment Approach

**Choice**: Configurable semantic search results, formatted as markdown context

**Implementation**:
```rust
async fn enrich_with_context(&self, query: &str) -> Result<String> {
    // Get configured context size (default 5)
    let context_size = self.config.agent.context_size.unwrap_or(5);

    let results = core.semantic_search(query, context_size).await?;

    let context = results.iter()
        .map(|r| format!("## {}\n\n{}\n", r.title, r.snippet))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
        context, query
    ))
}
```

**Configuration**:
```toml
# config.toml
[agent]
context_size = 5  # Number of semantic search results to include
```

```bash
# Via CLI
cru config set agent.context_size 10
```

**Why**:
- Simple, predictable behavior
- 5 results default balances context richness vs. token usage
- Configurable for different use cases (quick queries vs deep research)
- Markdown format is human-readable and agent-friendly
- Semantic search finds conceptually related content

**Alternatives Considered**:
- Hard-coded to 5: Less flexible, can't tune per use case
- Dynamic based on query: Too complex for MVP, unpredictable
- Graph traversal context: More complex, less predictable
- Full-text search: Misses semantic connections
- Vector similarity only: Loses graph structure
- Hybrid approach: Premature optimization for MVP

**Open Question**: Should we include graph structure (wikilinks, backlinks) in context?
- **Deferred**: Start with semantic search, add graph in post-MVP if needed

### Decision 5: Command Structure and Modes

**Choice**: Five core commands with chat as default, plus separate read/write modes

**Commands**:
```bash
cru [chat]     # Default - natural language (read-only/plan mode)
cru act        # Action mode - allows agent to write files
cru process    # Explicit pipeline run
cru status     # Kiln statistics
cru search     # Quick semantic search
cru config     # Configuration
```

**Chat Mode Strategy**:
- **Default (plan/ask mode)**: Agent can read files, no writes
- **Act mode (`cru act`)**: Agent can read AND write files
- Auto-enable watch mode during chat sessions for responsive file updates

**Why**:
- Chat-first aligns with ACP roadmap
- Separate modes prevent accidental file modifications
- Process gives explicit control when needed
- Status provides visibility
- Search for quick lookups without agent
- Config for setup and troubleshooting
- Watch mode during chat ensures agent sees latest changes

**Alternatives Considered**:
- REPL as default: Old paradigm, doesn't support ACP vision
- All-in-one command with modes: Confusing UX, hidden features
- Subcommand-heavy structure: Verbose, high cognitive load
- Always allow writes: Too permissive, accidents likely
- Always prompt for permission: Too much friction

## Architecture Diagrams

### System Context
```
┌─────────────────────────────────────────┐
│           User                           │
└───────────────┬─────────────────────────┘
                │
                ▼
┌─────────────────────────────────────────┐
│        Crucible CLI (cru)                │
│                                          │
│  ┌─────────────────────────────────┐    │
│  │  CrucibleCore Facade            │    │
│  │  - pipeline: NotePipeline       │    │
│  │  - storage: EnrichedNoteStore   │    │
│  │  - semantic_search: Service     │    │
│  └─────────────────────────────────┘    │
│                                          │
│  Commands:                               │
│  ├─ chat (ACP client)                   │
│  ├─ process (pipeline)                  │
│  ├─ status (metrics)                    │
│  ├─ search (semantic)                   │
│  └─ config                              │
└───────────────┬─────────────────────────┘
                │
                ├─────────────┐
                │             │
                ▼             ▼
    ┌──────────────────┐  ┌──────────────────┐
    │  External Agent  │  │  Core Libraries  │
    │  (claude-code)   │  │  - pipeline      │
    │  via ACP         │  │  - surrealdb     │
    └──────────────────┘  │  - enrichment    │
                          └──────────────────┘
```

### ACP Chat Flow
```
1. User Input
   │
   ▼
2. CLI enriches with semantic search
   │
   ▼
3. ACP Client sends enriched prompt
   │
   ▼
4. External Agent (claude-code) processes
   │
   ▼
5. Agent streams response via session_update
   │
   ▼
6. CLI displays to user
```

### Module Structure
```
crates/crucible-cli/src/
├── main.rs                 # Entry point, background processing
├── cli.rs                  # clap command definitions
├── core_facade.rs          # CrucibleCore facade
├── commands/
│   ├── mod.rs
│   ├── chat.rs            # ACP chat interface
│   ├── process.rs         # Pipeline processing
│   ├── status.rs          # Kiln statistics
│   ├── search.rs          # Semantic search
│   └── config.rs          # Configuration
├── acp/
│   ├── mod.rs
│   ├── client.rs          # ACP Client trait impl
│   ├── context.rs         # Context enrichment
│   └── agent.rs           # Agent spawning
└── pipeline/
    ├── mod.rs
    ├── processor.rs       # Background processing
    └── watcher.rs         # File watching
```

## Risks / Trade-offs

### Risk 1: Agent Availability

**Risk**: User doesn't have claude-code or compatible agent installed

**Mitigation**:
- Clear error messages: "Agent 'claude-code' not found. Install: npm install -g @anthropic/claude-code"
- Support multiple agents (claude-code, gemini-cli, codex)
- Document installation in README
- Fall back to search command for non-chat queries

**Impact**: Medium - affects first-time setup, but one-time issue

### Risk 2: Context Size vs. Quality

**Risk**: Top-5 semantic results may miss important context

**Mitigation**:
- Start with 5, make configurable: `cru config set agent.context_size 10`
- Monitor user feedback for optimal default
- Future: Add graph-based context expansion

**Impact**: Low - can iterate post-MVP

### Risk 3: Background Processing Delays

**Risk**: Large kilns (10,000+ files) may take minutes to process

**Mitigation**:
- Show progress: "Processing... 1,243 files (234/1,243)"
- Allow skipping: `--no-process` flag
- Incremental processing: Merkle diff only processes changed files
- Timeout: Default 5min, configurable

**Impact**: Medium - affects large kiln users, but Merkle diff minimizes

### Risk 4: Database Lock Conflicts

**Risk**: Background processing + command execution may conflict on SurrealDB

**Mitigation**:
- Use SurrealDB connection pooling (max 10 connections)
- Commands that write (process) run synchronously, not during background
- Chat mode is read-only during background processing

**Impact**: Low - SurrealDB handles concurrent access well

### Risk 5: ACP Protocol Breaking Changes

**Risk**: Official crate may have breaking changes

**Mitigation**:
- Pin to specific version: `agent-client-protocol = "0.6"`
- Monitor changelog before updates
- Trait abstraction allows swapping implementations

**Impact**: Low - protocol is stabilizing

## Migration Plan

### Phase 1: New Implementation (Week 1)
1. Create new module structure alongside old code
2. Implement facade, ACP client, chat command
3. Test with mock agents and real claude-code
4. No user impact yet - old CLI still works

### Phase 2: Feature Parity (Week 2)
1. Implement process, status, search commands
2. Add background processing
3. Test with real kilns
4. Still no breaking changes - both modes available

### Phase 3: Deprecation (Post-MVP)
1. Add deprecation warnings to REPL commands
2. Update documentation to recommend chat mode
3. Provide migration guide
4. Announce timeline for REPL removal

### Phase 4: Cleanup (Post-MVP + 1 month)
1. Remove REPL code
2. Remove disabled features
3. Finalize command structure
4. Release v1.0 with new CLI only

### Rollback Plan
If major issues discovered:
1. Keep old code in separate branch
2. Revert to old CLI binary
3. Fix issues in new implementation
4. Re-deploy when stable

## Resolved Questions

### Q1: Agent Installation Dependency ✅
**Decision**: Fallback through known agents, error if none found

**Rationale**: Auto-discovery improves UX - CLI works out of box if any compatible agent installed. Error message lists all compatible agents with install instructions.

### Q2: Context Size Configuration ✅
**Decision**: Configurable via `agent.context_size` in config file

**Rationale**: Default of 5 balances context vs tokens, but users can tune for their use case (quick queries vs deep research).

### Q3: Write Permissions ✅
**Decision**: Separate `cru chat` (read-only/plan) vs `cru act` (write mode)

**Rationale**: Prevents accidental file modifications while allowing intentional agent actions. Clear separation of concerns.

### Q4: Watch Mode Integration ✅
**Decision**: Auto-enable watch mode during chat sessions

**Rationale**: Ensures agent sees latest file changes during conversation. Can be disabled with `--no-watch` if needed.

### Q5: Migration Documentation ✅
**Decision**: No migration documentation needed

**Rationale**: Single developer/user, no backwards compatibility burden for MVP.

### Q6: Session Persistence ✅
**Decision**: No persistence for MVP (option B)

**Rationale**: Keep initial implementation simple. Post-MVP: sessions will be saved as markdown files in the kiln for RLHF and other post-processing workflows.

### Q7: Multi-Agent Support ✅
**Decision**: Single agent per session for MVP (option B)

**Rationale**: Simpler implementation. Long-term vision: hand-rolled agent system where agents and multi-agent workflows are defined via markdown files in the kiln.

### Q8: Error Handling Strategy ✅
**Decision**: Fail and let user restart manually (option B)

**Rationale**: Simpler error handling for MVP. Automatic restart can be added post-MVP if needed based on usage patterns.

## Open Questions

None remaining for MVP. All architectural decisions finalized.

## Success Metrics

### MVP Success Criteria
- [ ] `cru chat` successfully spawns agent and streams responses
- [ ] Context enrichment includes relevant semantic search results
- [ ] `cru process` completes on 1,000 file kiln in <30s
- [ ] `cru status` shows accurate metrics
- [ ] All commands use `NotePipeline` (no old processing code)
- [ ] Zero database lock errors during normal operation
- [ ] <2s startup time with background processing
- [ ] <1s semantic search response time

### Quality Gates
- [ ] 80%+ test coverage on new code
- [ ] All integration tests passing
- [ ] No panics in normal operation
- [ ] Memory usage <100MB idle, <500MB processing
- [ ] Works with claude-code, gemini-cli, codex

### User Experience Goals
- [ ] Natural language queries feel responsive
- [ ] Context enrichment improves answer quality
- [ ] Error messages are helpful and actionable
- [ ] Documentation covers common workflows
- [ ] Migration from REPL is clearly explained

## References

- **docs/ARCHITECTURE.md**: System architecture, ACP section (line 586-669)
- **docs/ACP-MVP.md**: ACP implementation examples
- **docs/PHILOSOPHY.md**: User-centric design principles
- **crates/crucible-pipeline**: New pipeline orchestrator
- **openspec/changes/complete-pipeline-integration**: Pipeline refactor context
- **agent-client-protocol crate**: https://crates.io/crates/agent-client-protocol
