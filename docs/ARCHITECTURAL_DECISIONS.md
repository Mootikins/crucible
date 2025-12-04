# Architectural Decisions

This document explains intentional architectural patterns and duplications in the Crucible codebase.

## Parser Implementations (Intentional Duplication)

### Multiple Markdown Parsers
Crucible supports multiple markdown parsers through feature flags:

- **markdown-it-parser** (default): Modern, extensible parser
- **pulldown-parser**: Alternative parser using pulldown-cmark

**Rationale**: Different parsers have different performance characteristics and feature sets. This allows users to choose based on their needs.

### Location
- `crates/crucible-parser/src/basic_markdown_it.rs` - Default implementation
- `crates/crucible-parser/src/pulldown.rs` - Alternative implementation

## Database Abstraction Layers

### Multiple Access Patterns
The storage system provides different abstraction layers:

- **RelationalDB**: SQL-like operations for structured queries
- **GraphDB**: Graph operations for relationship traversal
- **DocumentDB**: Document operations for unstructured data

**Rationale**: Different use cases require different access patterns. This provides flexibility while maintaining a single underlying storage engine.

### Location
- `crates/crucible-surrealdb/src/adapters.rs` - Implementation of all three interfaces

## Hashing Implementations

### Dual Hashing Systems
Two separate hashing implementations exist:

1. **Core Hashing** (`crates/crucible-core/src/hashing/`): Comprehensive hashing infrastructure
2. **Simple Block Hasher** (`crates/crucible-parser/src/block_hasher.rs`): BLAKE3-only implementation

**Rationale**: SimpleBlockHasher exists in the parser crate to avoid circular dependencies. Core defines types and traits, parser provides lightweight implementation.

## Embedding Provider Architecture

### Plugin System
Multiple embedding providers are supported through a common trait:

- **FastEmbed** (default): Local embedding model
- **OpenAI**: Cloud-based embedding service
- **Ollama**: Local Ollama server

**Rationale**: Different embedding models have different trade-offs between accuracy, speed, and privacy. This allows users to choose based on their requirements.

### Location
- `crates/crucible-llm/src/embeddings/` - Provider implementations
- `crates/crucible-core/src/enrichment/` - Core traits and types

## Parser/Core Separation

### Dependency Inversion
The codebase uses dependency inversion to avoid circular dependencies:

- `crucible-core` defines parser traits and types
- `crucible-parser` implements the traits
- Flow: Core ← Parser (unidirectional)

**Rationale**: This breaks circular dependencies and follows SOLID principles. Core can focus on business logic while parser handles implementation details.

## Configuration System

### Two-Tier Configuration
Configuration exists at multiple levels:

1. **Core Configuration** (`crucible-config`): Canonical types and validation
2. **CLI Configuration** (`crucible-cli`): Command-line specific extensions

**Rationale**: Separation of concerns allows different frontends (CLI, Tauri) to share configuration logic while having their own specific needs.

## Dead Code Attributes

### Future-Proofing Fields
Many struct fields use `#[allow(dead_code)]` with comments indicating they are "reserved for future use":

- `crucible-core/src/storage/deduplicator.rs` - Fields for future deduplication strategies
- `crucible-parser/src/block_extractor.rs` - Fields for enhanced heading tracking
- `crucible-core/src/storage/traits.rs` - Fields for future operation tracking

**Rationale**: These fields are part of public APIs and are reserved for future features to avoid breaking changes.

## Feature Flags

### Legacy Features
Some feature flags are kept for backward compatibility:

- `pulldown-parser`: Alternative parser (not default)
- `parallel-processing`: Enables Rayon-based parallel processing

**Rationale**: These features may be useful for specific workloads and are maintained as optional dependencies.

## Type Re-exports

### Canonical Type Locations
Parser types are canonically defined in `crucible-core` but re-exported by `crucible-parser` for convenience:

```rust
// crucible-core/src/parser/types/ - Re-exports from core
pub use crucible_core::parser::{
    ParsedNote, Wikilink, Tag, BlockHash, // ... etc
};
```

**Rationale**: This provides a clean import path while maintaining a single source of truth for type definitions.