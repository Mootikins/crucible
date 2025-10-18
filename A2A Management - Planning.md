Implementation Plan: Crucible A2A with Rune Context Management

Phase 1: Core Types & Infrastructure (TDD)

1. Create crucible-a2a crate with proper module structure
2. **DOCS**: Write vault doc for Core Types architecture → STOP FOR REVIEW
3. Write tests for MessageMetadata, ContextWindow, MessageMetadataStore
4. Implement types to pass tests
5. **DOCS**: Write vault doc for MessageMetadataStore design → STOP FOR REVIEW
6. Write tests for entity tracking and message indexing
7. Implement entity extraction and indexing

Phase 2: Rune Context API (TDD)

1. **DOCS**: Write vault doc for Rune Context API design → STOP FOR REVIEW
2. Write tests for thread-local context state
3. Implement PruningContextState with thread-local storage
4. **DOCS**: Write vault doc for API modules (context::, agent::, decision::) → STOP FOR REVIEW
5. Write tests for Rune Message type and methods
6. Implement Message type with context lookups
7. Write tests for context API (context::, agent::, decision::, config::)
8. Implement Rune API modules

Phase 3: Rune Strategy Engine (High-Level VM)

**NOTE**: Research `outlines.txt` philosophy - structured input/output types with short prompts
- Goal: Extremely low token workflows (sed/awk/grep style)
- Strategies may use LLM for compression decisions (structured prompts)
- Context is library of small, focused notes with metadata indexing
- Keep windows open for important things, auto-compress small tasks

1. **DOCS**: Write vault doc for Rune VM architecture → STOP FOR REVIEW
2. Write tests for RuneStrategyEngine initialization and basic VM setup
3. Implement high-level VM engine with Rune context integration
4. **DOCS**: Write vault doc for strategy interface patterns → STOP FOR REVIEW
5. Write tests for basic strategy interface (load, compile, execute)
6. Implement basic strategy loading and execution interface
7. Defer: Full API modules and example strategies (later iteration)

Phase 4: Context Coordinator (TDD)

1. **DOCS**: Write vault doc for Context Arena architecture → STOP FOR REVIEW
2. Write tests for ContextArena (multi-agent context management)
3. Implement arena with per-agent windows
4. **DOCS**: Write vault doc for Context Coordinator design → STOP FOR REVIEW
5. Write tests for ContextCoordinator orchestration
6. Implement coordinator with strategy routing
7. **DOCS**: Write vault doc for Agent Collaboration Graph → STOP FOR REVIEW
8. Write tests for AgentCollaborationGraph
9. Implement graph tracking

Phase 5: Documentation & Follow-up Tasks

**NOTE**: All docs created in `/home/moot/Documents/crucible-testing/Projects/A2A Context/`

1. **DOCS**: Create vault summary: "A2A Context Architecture - Implementation" → STOP FOR REVIEW
2. **DOCS**: Create task document: "Extract Rune Tools to crucible-rune Crate" → STOP FOR REVIEW
3. **DOCS**: Create observability document: "Stats Dashboard Design" → STOP FOR REVIEW

Deliverables

- Working crucible-a2a crate with Rune-based context management
- 50+ tests covering all major components
- Example Rune strategies demonstrating different pruning approaches
- Vault documentation of architecture and design decisions
- Follow-up task list for Rune tool extraction

Test-First Approach

Every component will have tests written BEFORE implementation, ensuring correctness and guiding API design.
