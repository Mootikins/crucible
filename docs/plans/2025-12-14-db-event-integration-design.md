# DB Event Integration Design

**Date:** 2025-12-14
**Status:** Approved
**Implementation:** `thoughts/plans/db-event-integration/TASKS.md`

## Summary

Full event-driven architecture where Watch, Parser, Storage, and Embeddings communicate via a unified SessionEvent bus.

## Goals

1. **Real-time indexing** - File changes auto-update DB + embeddings
2. **Reactive pipelines** - DB changes trigger downstream processing
3. **Cross-system sync** - Events flow bidirectionally for consistency

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         EventBus (SessionEvent)                      │
│                                                                      │
│  Producers:                      Consumers:                         │
│  ├─ WatchManager                 ├─ StorageHandler                  │
│  ├─ ParserHandler                ├─ EmbeddingHandler                │
│  ├─ EAVGraphStore               ├─ TagHandler                      │
│  ├─ EmbeddingService            ├─ LoggingHandler                  │
│  └─ MCP Server                   └─ Rune handlers                   │
└─────────────────────────────────────────────────────────────────────┘
```

## Event Flow

```
FileChanged → NoteParsed → EntityStored → BlocksUpdated → EmbeddingRequested → EmbeddingGenerated
```

## Key Decisions

1. **SessionEvent as universal type** - All systems emit/consume SessionEvent (not domain-specific buses)
2. **Traits in crucible-core** - EventEmitter/EventSubscriber traits avoid circular dependencies
3. **Pipeline as event cascade** - No explicit orchestrator; each component emits after completing work
4. **Rune compatibility** - Phase 6 adds protocol support for Rune handlers
5. **Fail-open semantics** - Handler errors logged but don't stop processing

## New SessionEvent Variants

- File: `FileChanged`, `FileDeleted`, `FileMoved`
- Storage: `EntityStored`, `EntityDeleted`, `BlocksUpdated`, `RelationCreated`, `TagAssociated`
- Embeddings: `EmbeddingRequested`, `EmbeddingGenerated`, `EmbeddingBatchComplete`

## Phases

| Phase | Focus | Parallel? |
|-------|-------|-----------|
| 1 | Core Infrastructure (traits, SessionEvent in core) | - |
| 2 | Storage Handlers (crucible-surrealdb) | Yes |
| 3 | Watch Integration (crucible-watch) | Yes |
| 4 | Embedding Handler (crucible-llm) | Yes |
| 5 | MCP Migration (crucible-tools) | Yes |
| 6 | Rune Protocol Support | After 1 |
| 7 | Runtime Wiring (crucible-cli) | After all |

## References

- Full implementation plan: `thoughts/plans/db-event-integration/TASKS.md`
- Previous event cleanup: `thoughts/plans/event-cleanup/TASKS.md`
