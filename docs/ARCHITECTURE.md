# Crucible Architecture

> **Version**: 2025-11-15
> **Status**: Living document - reflects short-term implementation, mid-term roadmap, and long-term vision

This document defines Crucible's architecture across three time horizons: **short-term** (ACP MVP), **mid-term** (desktop & tooling), and **long-term** (speculative future). It serves as the authoritative reference for architectural decisions.

---

## Guiding Principles

1. **Plaintext-First**: Markdown files are the source of truth. The database is optional infrastructure for rich queries, not a requirement.
2. **Trait-Based Extensibility**: Core APIs exposed via traits to enable testing, alternative backends, and interface flexibility.
3. **SOLID + Dependency Injection**: Small modules with explicit responsibilities, swappable implementations.
4. **Editor Agnostic**: Works with any text editor (VS Code, Obsidian, Neovim, etc.) - desktop app is long-term.
5. **Human-in-the-Loop**: Interactive workflows, not fire-and-forget automation. Performance targets optimized for back-and-forth interaction.

---

## Three-Horizon Roadmap

### Short-Term: ACP MVP (Current - 2025 Q4)
**Goal**: Get Agent Context Protocol (ACP) + CLI chat interface working with EPR schema.

**Scope**:
- ✅ EPR schema in SurrealDB (entities/properties/relations)
- ✅ Hash-based change detection using hybrid Merkle trees
- ✅ Block-aligned chunks for embeddings
- ✅ Hybrid Merkle tree persistence for section-level diffs (completed 2025-11-15)
- 🔄 ACP integration (Zed's implementation)
- 🔄 CLI chat shell (natural language queries, not SQL REPL)

**Out of Scope**:
- ❌ CRDT (deferred indefinitely - Merkle trees handle single-user multi-device sync)
- ❌ Direct LLM integration (ACP is sufficient for MVP)
- ❌ Desktop UI (CLI + editor agnostic approach first)
- ❌ Plugin system (mid-term)

### Mid-Term: Tooling & Extensibility (2025 Q1-Q2)
**Goal**: Add scripting layer for integrated tools, expand ACP capabilities.

**Scope**:
- Scripting layer (Rune or Lua) for MCP-style tools
- Tool registry with semantic search (RapidAPI-inspired)
- Query optimization and caching improvements
- Expanded ACP services (graph traversal, context assembly)
- Potential desktop UI (Tauri) - validates "unified core" architecture

**Capabilities**:
- External API integration via scripts (e.g., fetch web content, call REST APIs)
- Internal DB operations exposed to scripts (e.g., custom queries, data transformations)
- Tool searchability for agents (vector embeddings of tool descriptions)
- Both human and agent access to tools

### Long-Term: Distributed Agents & Enterprise (Speculative)
**Vision**: Federated agent interaction, multiplayer editing, enterprise features.

**Potential Features**:
- **A2A Protocol**: Distributed/federated agent interaction (e.g., Tailscale network with shared inference boxes)
  - Separate from ACP - different use case
  - Community/enterprise network setups
  - May inform workflow definition decisions
- **CRDT Sync**: Multiplayer editing for enterprise users (Yjs/Yrs)
  - Very long-term, only if needed
  - Merkle trees sufficient for most use cases
- **Desktop App**: Built-in editor (substantial changes required)
- **Plugin Ecosystem**: Community-contributed extensions

---

## System Architecture

### Layer Overview

```
┌─────────────────────────────────────────────────────────┐
│                    Interface Layer                       │
│  ┌──────────────┐  ┌──────────────┐                     │
│  │  CLI Chat    │  │ Desktop (LT) │                     │
│  │  (ACP Client)│  │              │                     │
│  └──────┬───────┘  └──────┬───────┘                     │
└─────────┼──────────────────┼───────────────────────────┘
          │                  │
          │ spawns           │
          ▼ subprocess       │
┌──────────────────┐         │
│  External Agent  │         │
│  (claude-code,   │         │
│   gemini-cli)    │         │
└──────────────────┘         │
                             │
          ┌──────────────────┴──────────────────┐
          │  Both interfaces use Core Façade    │
          └──────────────────┬──────────────────┘
                             │
┌─────────────────────────────▼───────────────────────────┐
│                     Core Layer (crucible-core)           │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Façade API (unified entry point for all UIs)   │   │
│  └─────────────────────────────────────────────────┘   │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Parser     │  │  Storage     │  │   Change     │  │
│  │   (MD→AST)   │  │   Traits     │  │  Detection   │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │  Embedding   │  │    Merkle    │  │   Config     │  │
│  │   Abstraction│  │    Trees     │  │  Provider    │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  │
│                                                          │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Scripting Layer (Mid-term - Rune/Lua traits)   │   │
│  └─────────────────────────────────────────────────┘   │
└──────────────────────────┬───────────────────────────────┘
                           │
┌──────────────────────────▼───────────────────────────────┐
│              Infrastructure Layer                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │  SurrealDB   │  │  Fastembed   │  │  File Watch  │   │
│  │  (EPR schema)│  │  (default)   │  │  (notify-rs) │   │
│  └──────────────┘  └──────────────┘  └──────────────┘   │
└──────────────────────────────────────────────────────────┘
                           │
                           ▼
                    ┌──────────────┐
                    │  Filesystem  │
                    │  (Markdown)  │
                    └──────────────┘

Note: ACP is NOT a layer - it's how CLI spawns/communicates with external agents
```

### Data Flow

**File → Database** (Refined Pipeline Architecture):
```
1. File watcher detects change (notify-rs)
2. Pipeline Phase 1: Quick filter (file hash, modification time)
3. Pipeline Phase 2: Parser converts Markdown → AST → EPR entities
4. Pipeline Phase 3: Merkle tree diff identifies changed sections
5. Pipeline Phase 4a: Content enrichment (block selection + embedding generation)
6. Pipeline Phase 4b: Metadata enrichment (metadata extraction + relationship inference)
7. Pipeline Phase 5: Storage layer persists enriched data:
   - EPR entities/properties/relations
   - Vector embeddings
   - Merkle tree structure
```

**Clean Architecture Principles**:
- **Pipeline**: Configuration and coordination layer - controls "what and when", manages resources
- **Parser**: Pure transformation (Markdown → AST → EPR) - no side effects
- **ContentEnrichService**: Block selection and embedding generation - controlled by pipeline
- **MetadataEnrichService**: Metadata extraction and relationship inference - controlled by pipeline
- **Storage**: Pure I/O layer - no knowledge of enrichment generation or tree computation
- **Configuration**: Centralized in config crate for ALL system components

**Pipeline Resource Control**:
- **"Diameter of the pipe"**: Pipeline controls batch sizes, parallelism, memory limits
- **Adaptive resource management**: Adjusts throughput based on system conditions
- **Strategy selection**: Incremental vs full enrichment approaches
- **Error handling**: Simple error bubbling with clear boundaries

**Query → Response**:
```
1. User query via CLI chat or ACP client
2. ACP layer translates to core API calls
3. Core routes via storage traits
4. SurrealDB performs hybrid search (semantic + graph + fuzzy)
5. Results returned through trait → ACP → interface
```

---

## Core Components

### 1. Parser (Markdown → AST → EPR)

**Responsibility**: Convert Markdown files into structured entities with graph relationships.

**AST → EPR Mapping**:
```
Markdown Element           → EPR Entity/Relation
────────────────────────────────────────────────────────────
Document                   → entities:note:* (with frontmatter properties)
Heading (h1-h6)            → entities:block:* (type: heading, level: 1-6)
Paragraph                  → entities:block:* (type: paragraph)
List (ul/ol)               → entities:block:* (type: list, contains list_item tree)
Code Block                 → entities:block:* (type: code, language: *)
Callout (!!! syntax)       → entities:block:* (type: callout, variant: *)
Blockquote                 → entities:block:* (type: blockquote)
Table                      → entities:block:* (type: table)

Wikilink ([[note]])        → relations:wikilink (from_block → to_note)
Tag (#tag)                 → relations:tagged (block → tag entity)
Inline link                → relations:link (block → URL, stored as metadata)
Footnote reference         → relations:footnote (block → footnote block)
Embedded image             → relations:embedded (block → asset entity)
```

**Key Decisions**:
- **Lists**: Internal tree of list items, embedding generated for the **list as a whole** (not per item)
- **Inline elements**: Bold, italic, inline code stored as Markdown text in block content (not separate entities)
- **Nested structures**: Represented via parent-child relations (e.g., list items within lists)
- **Inline links**: Maintain relation metadata in the block while preserving link in content

**Obsidian-Flavored Markdown Support**:
- ✅ Wikilinks (`[[Note Name]]`, `[[Note|Alias]]`)
- ✅ Tags (`#tag`, `#nested/tag`)
- ✅ Callouts (`> [!note]`, `> [!warning]`, etc.) - [spec](https://help.obsidian.md/callouts)
- ✅ Frontmatter (YAML metadata)
- ✅ Footnotes (`[^1]`)
- ✅ Embedded images (`![[image.png]]`)
- 🔄 DataView queries (future - may require scripting layer)
- 🔄 Custom extensions (TBD - potentially via plugins)

**Full Spec**: [Obsidian Flavored Markdown](https://help.obsidian.md/obsidian-flavored-markdown)

**Open Question**: Does markup in embeddings affect model output? Worth exploring normalized vs raw content.

---

### 2. Storage Layer (Entity-Attribute-Value + Graph Schema)

**Responsibility**: Persist parsed graph, metadata, and embeddings. **Does NOT store raw Markdown content.**

**Schema Pattern**: Hybrid **EAV (Entity-Attribute-Value)** + **Property Graph**
- **Entities**: Documents, blocks, tags, assets (nodes in the knowledge graph)
- **Properties/Attributes**: Metadata attached to entities via namespace/key/value triples
  - **Portable metadata** (`namespace: "frontmatter"`): Synced to/from YAML frontmatter in file
  - **Derived metadata** (`namespace: "system"` or `"computed"`): Generated, can be rebuilt
- **Relations**: Typed, directed edges between entities (wikilinks, tags, footnotes, embeddings)

**Why EAV + Graph?**
- **EAV**: Flexible schema for heterogeneous metadata (every note has different frontmatter)
- **Property Graph**: Native graph traversal for wikilinks, backlinks, tag hierarchies
- **Best of Both**: Flexibility of NoSQL + power of graph queries

**Schema Design** (inspired by [Oxen AI](https://github.com/Oxen-AI/Oxen)):
```surql
-- Entities (nodes)
DEFINE TABLE entities SCHEMAFUL;
DEFINE FIELD type ON entities TYPE string ASSERT $value IN ['note', 'block', 'tag', 'section', 'media', 'person'];
DEFINE FIELD id ON entities TYPE string;
DEFINE FIELD created_at ON entities TYPE datetime;
DEFINE FIELD data ON entities TYPE option<object>;  -- Entity-specific data

-- Properties (metadata with namespace for extensibility)
DEFINE TABLE properties SCHEMAFUL;
DEFINE FIELD entity_id ON properties TYPE record<entities>;
DEFINE FIELD namespace ON properties TYPE string DEFAULT "core";  -- "frontmatter", "system", "computed"
DEFINE FIELD key ON properties TYPE string;
DEFINE FIELD value_type ON properties TYPE string ASSERT $value IN ['text', 'number', 'boolean', 'date', 'json'];
DEFINE FIELD value_text ON properties TYPE option<string>;
DEFINE FIELD value_number ON properties TYPE option<float>;
DEFINE FIELD value_bool ON properties TYPE option<bool>;
DEFINE FIELD value_date ON properties TYPE option<datetime>;
DEFINE FIELD value_json ON properties TYPE option<object>;  -- Complex nested metadata
DEFINE FIELD source ON properties TYPE string DEFAULT "parser";
DEFINE FIELD confidence ON properties TYPE float DEFAULT 1.0;

-- Relations (edges)
DEFINE TABLE relations SCHEMAFUL TYPE RELATION FROM entities TO entities;
DEFINE FIELD relation_type ON relations TYPE string;
DEFINE FIELD weight ON relations TYPE float DEFAULT 1.0;
DEFINE FIELD directed ON relations TYPE bool DEFAULT true;
DEFINE FIELD confidence ON relations TYPE float DEFAULT 1.0;
DEFINE FIELD source ON relations TYPE string DEFAULT "parser";
DEFINE FIELD position ON relations TYPE option<int>;
DEFINE FIELD context ON relations TYPE option<string>;  -- Breadcrumbs or hash references
DEFINE FIELD metadata ON relations TYPE object DEFAULT {};

-- Blocks (AST nodes)
DEFINE TABLE blocks SCHEMAFUL;
DEFINE FIELD entity_id ON blocks TYPE record<entities>;
DEFINE FIELD block_type ON blocks TYPE string;
DEFINE FIELD content ON blocks TYPE string;
DEFINE FIELD content_hash ON blocks TYPE string;
DEFINE FIELD parent_block_id ON blocks TYPE option<record<blocks>>;

-- Embeddings (vector search)
DEFINE TABLE embeddings SCHEMAFUL;
DEFINE FIELD entity_id ON embeddings TYPE record<entities>;
DEFINE FIELD block_id ON embeddings TYPE option<record<blocks>>;
DEFINE FIELD embedding ON embeddings TYPE array<float>;
DEFINE FIELD model ON embeddings TYPE string;
DEFINE FIELD dimensions ON embeddings TYPE int;

-- Tags (hierarchical)
DEFINE TABLE tags SCHEMAFUL;
DEFINE FIELD name ON tags TYPE string;
DEFINE FIELD parent_id ON tags TYPE option<record<tags>>;
DEFINE FIELD path ON tags TYPE string;  -- Materialized path: "/project/crucible"
DEFINE FIELD depth ON tags TYPE int DEFAULT 0;

-- Entity-Tag Relations
DEFINE TABLE entity_tags SCHEMAFUL;
DEFINE FIELD entity_id ON entity_tags TYPE record<entities>;
DEFINE FIELD tag_id ON entity_tags TYPE record<tags>;
DEFINE FIELD source ON entity_tags TYPE string DEFAULT "parser";
DEFINE FIELD created_at ON entity_tags TYPE datetime DEFAULT time::now();
```

**Storage Traits** (for backend flexibility):
```rust
pub trait EntityStore {
    async fn create_entity(&self, entity: Entity) -> Result<EntityId>;
    async fn get_entity(&self, id: EntityId) -> Result<Option<Entity>>;
    async fn update_entity(&self, id: EntityId, updates: HashMap<String, Value>) -> Result<()>;
    async fn delete_entity(&self, id: EntityId) -> Result<()>;
}

pub trait RelationStore {
    async fn create_relation(&self, relation: Relation) -> Result<RelationId>;
    async fn get_relations(&self, from: EntityId, rel_type: Option<RelationType>) -> Result<Vec<Relation>>;
    async fn delete_relation(&self, id: RelationId) -> Result<()>;
}

pub trait EmbeddingStore {
    async fn store_embedding(&self, block_id: EntityId, vector: Vec<f32>, model: String) -> Result<()>;
    async fn semantic_search(&self, query_vector: Vec<f32>, limit: usize) -> Result<Vec<(EntityId, f32)>>;
    async fn delete_embeddings(&self, block_id: EntityId) -> Result<()>;
}

pub trait GraphStore: EntityStore + RelationStore {
    async fn traverse(&self, start: EntityId, traversal: GraphTraversal) -> Result<Vec<Entity>>;
    async fn shortest_path(&self, from: EntityId, to: EntityId) -> Result<Option<Vec<EntityId>>>;
}
```

**Backend Flexibility**: Traits designed to support:
- ✅ SurrealDB (current implementation)
- 🔄 PostgreSQL + pgvector (future alternative)
- 🔄 SQLite + vector extension (lightweight alternative)
- 🔄 In-memory mock (testing)

**Key Principle**: Database is the **cache layer** for metadata/graph/embeddings. Raw Markdown lives on filesystem for portability.

**Source of Truth**: Filesystem markdown files. Database can be rebuilt from files at any time.

**Frontmatter Handling**:

Frontmatter properties are **bidirectionally synced** between file and database:

```yaml
---
tags: [project, ai]
type: template
status: draft
date_created: 2025-11-08
date_modified: 2025-11-08
author: username
---
```

**Storage Strategy**:
- All frontmatter stored in `properties` table with `namespace: "frontmatter"`
- Parser extracts frontmatter → creates property records
- Property updates can trigger frontmatter rewrites (bidirectional sync)
- Always include `date_created` and `date_modified` in frontmatter for portability

**Namespace Philosophy**:
- `namespace: "frontmatter"` - Portable, user-editable, persisted in file
  - Examples: tags, type, status, date_created, date_modified, author
  - Survives database rebuild
  - Can be manually edited by users
- `namespace: "system"` - System-generated, ephemeral
  - Examples: file_size, word_count, link_count, last_embedded_at
  - Can be recomputed from file
- `namespace: "computed"` - Derived from analysis
  - Examples: readability_score, sentiment, topic_cluster
  - Can be expensive to compute, cached in DB

**Entity `type` vs Frontmatter `type`**:
- **Entity `type` field**: Database classification ("note", "block", "tag", etc.)
- **Frontmatter `type` property**: User-defined note classification (stored in `properties` table)
  - Examples: "template", "article", "person", "daily-note", "meeting"
  - Enables custom workflows and filtering

---

### 3. Merkle Trees (Change Detection)

**Status**: ✅ **Production Ready** (as of 2025-11-15)

**Responsibility**: Efficient **knowledge base-wide** detection of changes for incremental re-parsing and re-embedding.

**Implementation** (based on [Oxen AI's Merkle Tree](https://github.com/Oxen-AI/Oxen/tree/2eaf17867152e9fdfba4ef9813ba5f6289a210ef/oxen-rust/src/lib/src/model/merkle_tree)):
```
Knowledge Base-Wide Hybrid Structure:
┌─────────────────────────────────────────────────────────┐
│              Workspace Root (Knowledge Base)             │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Root Hash (entire knowledge base)                │  │
│  └───────────────────┬───────────────────────────────┘  │
│                      │                                   │
│      ┌───────────────┴────────────────┐                 │
│      ▼                                ▼                 │
│  Directory 1 Hash               Directory 2 Hash        │
│  (/projects/)                   (/archive/)             │
└─────────────────────────────────────────────────────────┘
         │                                │
         ▼                                ▼
┌──────────────────────────┐    ┌──────────────────────────┐
│   Directory 1            │    │   Directory 2            │
│   VNode (if >100 files)  │    │   VNode (if >100 files)  │
│   ┌──────────────────┐   │    │   ┌──────────────────┐   │
│   │  File 1 Hash     │   │    │   │  File N Hash     │   │
│   │  File 2 Hash     │   │    │   │  File N+1 Hash   │   │
│   └──────────────────┘   │    │   └──────────────────┘   │
└────────────┬─────────────┘    └──────────────────────────┘
             │
             ▼
    ┌───────────────────┐
    │   Document Tree   │
    │   (per file)      │
    │   ┌───────────┐   │
    │   │ Section 1 │   │  ← Top-level heading + blocks
    │   │ Section 2 │   │
    │   │ Section 3 │   │
    │   └───────────┘   │
    └───────────────────┘
             │
             ▼
    ┌───────────────────┐
    │  Section Tree     │
    │  (binary Merkle)  │
    │   ┌──────────┐    │
    │   │ Block 1  │    │  ← Paragraph, list, code block, etc.
    │   │ Block 2  │    │
    │   │ Block 3  │    │
    │   └──────────┘    │
    └───────────────────┘
```

**Key Differences from Oxen AI**:
- **Scope**: Knowledge base-wide (not just file-level) - detects moves/renames, folder-level changes
- **VNode capacity**: 100 (vs Oxen's 10,000) - unlikely to have >100 files per directory in knowledge bases
- **VNode placement**: Directory level only (file-level VNodes not needed for markdown)
- **Purpose**: Multi-device sync + change detection, not version control

**Algorithm**:
1. Build workspace tree: directories → files → sections → blocks
2. Compute hashes bottom-up (blocks → sections → files → directories → workspace root)
3. Use VNodes for directories with >100 files (consistent hashing distribution)
4. Persist all hashes in EPR entities (metadata properties)
5. On change detection:
   - Compare workspace root hash (O(1) check if anything changed)
   - If different: recursively compare subtrees to identify changed paths
   - Re-parse + re-embed only changed blocks
   - Performance: O(changes) not O(total files)

**Change Detection Benefits**:
- **File modifications**: Section-level granularity
- **File moves/renames**: Detected via hash matching (same content, different path)
- **Folder-level changes**: Directory hash changes propagate to root
- **Deletions**: Missing nodes in new tree vs old tree

**Sync Scenario** (multi-device, single-user):
1. Device A edits file, updates Merkle tree
2. Sync tool (rsync, Syncthing, Dropbox) propagates file change to Device B
3. Device B compares root hash (O(1) sync check)
4. Merkle tree diff identifies changed files/sections
5. Re-parse + re-embed only changed sections
6. Database updated incrementally

**Performance** (from Oxen AI research):
- Small KB (<1,000 docs): 50-100ms full build, 1-2ms change detection
- Medium KB (1,000-10,000 docs): 500ms-1s full build, 5-10ms change detection
- Large KB (10,000+ docs): 2-5s full build, 10-20ms change detection

**CRDT Alternative**: Merkle trees sufficient for single-user multi-device sync. CRDT only needed for **multiplayer editing** (enterprise use case, very long-term).

**Implementation Status** (Completed 2025-11-15):

*Phase 1 - Hash Infrastructure*:
- ✅ Dual-hash strategy: `BlockHash` (32-byte content), `NodeHash` (16-byte structure)
- ✅ Efficient hash combining using `blake3::hash()` for structural hashes
- ✅ Type safety with dedicated hash types (prevents mixing content/structure hashes)

*Phase 2 - Storage Abstraction*:
- ✅ `MerkleStore` trait in `crucible-core` for swappable backends
- ✅ `InMemoryMerkleStore` implementation for testing
- ✅ SurrealDB persistence layer (`MerklePersistence`) for production

*Phase 3 - Production Features*:
- ✅ Thread safety with `Arc<RwLock<HybridMerkleTree>>` wrapper
- ✅ Binary serialization with versioning (`VersionedSection`, format v1)
- ✅ Complete CRUD operations (store, retrieve, update, delete trees)
- ✅ Section virtualization for large documents (>100 sections)

*Phase 4 - Verification*:
- ✅ Comprehensive integration tests (10 test cases)
- ✅ SurrealDB persistence verification
- ✅ Expert code review (⭐⭐⭐⭐ 4.4/5 rating)
- ✅ QueryResult API fixes for SurrealDB compatibility
- ✅ Feature gating for optional embedding dependencies

*Key Design Patterns from Oxen AI*:
- Virtual sections for memory-efficient large document handling
- Path-based tree organization (workspace → directory → file → section → block)
- Bottom-up hash computation for efficient change detection
- Separate storage trait for backend flexibility

**Storage Backend**:
- `crates/crucible-surrealdb/src/merkle_persistence.rs` - Full persistence layer
- Tables: `hybrid_tree`, `section`, `virtual_section`
- Thread-safe concurrent access via SurrealDB connection pooling

---

### 4. Embedding Service

**Responsibility**: Generate and manage vector embeddings for semantic search.

**Abstraction** (trait-based for provider flexibility):
```rust
pub trait EmbeddingProvider {
    async fn embed_text(&self, text: String) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>>;
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> usize;
}

// Implementations:
// - FastembedProvider (default - local, no API calls)
// - OpenAIProvider (cloud - gpt-4-embeddings)
// - CustomProvider (user-defined via scripting layer)
```

**Default Provider**: [Fastembed](https://github.com/Anush008/fastembed-rs) (local embeddings, no cloud dependency)

**Embedding Strategy**:
- **Granularity**: Per-block (headings, paragraphs, lists, code blocks, callouts)
- **Exclusions**: Very short blocks (<5 words) - no embedding generated
  - **Open Question**: How to preserve semantic data for short items? (Index by parent heading? Skip entirely?)
- **Content**: Raw markdown text (initially) - may explore normalized versions
- **Hierarchical Context**: Child blocks do **not** include parent context in embedding content (e.g., paragraph under heading doesn't repeat heading text)
- **Deduplication**: Rare enough to not design for, but low-effort optimizations welcome

**Embedding Lifecycle**:
1. Block created → embedding generated → stored in `embeddings` table
2. Block updated → old embedding deleted → new embedding generated
3. Block deleted → embedding deleted
4. Document deleted → all block embeddings deleted

**Storage**: Embeddings stored in SurrealDB via `EmbeddingStore` trait (supports vector search).

---

### 5. File Watching & Event System

**Responsibility**: Detect file changes and trigger incremental re-processing.

**Scope**: Core feature (all interfaces use it, not CLI-specific).

**Implementation**:
- **Library**: [notify-rs](https://github.com/notify-rs/notify) (cross-platform file watcher)
- **Event Model**: Core defines event types, interfaces subscribe to events

**Event Types**:
```rust
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

pub trait FileEventHandler {
    async fn on_file_event(&self, event: FileEvent) -> Result<()>;
}
```

**Use Cases**:
- **External editors**: Watch files edited in VS Code, Obsidian, Neovim, etc.
- **Agent workflows**: Re-parse files created/modified by agents
- **Desktop integration** (long-term): Built-in editor (requires substantial changes)

**Placement**: Core layer defines traits, infrastructure layer implements watcher.

---

### 6. ACP Integration (Agent Context Protocol)

**Responsibility**: Enable CLI to communicate with external AI agents (Claude Code, Gemini CLI, etc.) for natural language interaction with the knowledge base.

**What ACP Is**: A subprocess-based protocol (like LSP) that allows editors/CLIs to spawn and communicate with AI agents via JSON-RPC over stdin/stdout.

**What ACP Is NOT**: Not a layer between UI and core. Not a server. Not embedded in the core façade.

**Implementation**: [`agent-client-protocol`](https://github.com/agentclientprotocol/agent-client-protocol) Rust crate (external dependency)

**Correct Architecture**:
```
┌──────────────────────────────────────┐
│      Crucible CLI (ACP Client)       │
│                                      │
│  ┌────────────────────────────────┐ │
│  │  ClientSideConnection          │ │
│  │  (implements Client trait)     │ │
│  │                                │ │
│  │  - Spawn agent subprocess      │ │
│  │  - Send prompts with context   │ │
│  │  - Handle permission requests  │ │
│  │  - Stream agent responses      │ │
│  └────────────────┬───────────────┘ │
│                   │ subprocess       │
│                   │ JSON-RPC         │
└───────────────────┼──────────────────┘
                    │ stdin/stdout
┌───────────────────▼──────────────────┐
│   External Agent (Separate Process)  │
│   (claude-code, gemini-cli, etc.)    │
│                                      │
│  ┌────────────────────────────────┐ │
│  │  AgentSideConnection           │ │
│  │  (implements Agent trait)      │ │
│  │                                │ │
│  │  - Receive prompts             │ │
│  │  - Call LLM API                │ │
│  │  - Execute tools via MCP       │ │
│  │  - Stream responses            │ │
│  └────────────────────────────────┘ │
└──────────────────────────────────────┘
```

**CLI's Responsibilities** (as ACP Client):
1. Implement `Client` trait from `agent-client-protocol` crate
2. Spawn agent subprocess when user starts chat
3. Gather context from knowledge base:
   - Relevant notes (based on query)
   - Graph structure (wikilinks, tags)
   - Metadata (frontmatter, timestamps)
   - Semantic search results
4. Send context with prompts via `session/prompt`
5. Handle permission requests (file operations, etc.)
6. Stream agent responses back to user

**Context Gathering**:
```rust
// CLI assembles context from core traits
let context = vec![
    ContentBlock::TextContent {
        text: format!("# Related Notes\n\n{}", related_notes),
    },
    ContentBlock::TextContent {
        text: format!("# Graph Structure\n\n{}", graph_viz),
    },
    ContentBlock::ResourceLink {
        uri: "kiln://note/123".to_string(),
    },
];

// Send to agent
client.prompt(PromptRequest {
    session_id,
    prompt: user_query,
    context: Some(context),
}).await?;
```

**MCP Integration** (Future):
- Later, Crucible may expose MCP tools (search, create note, traverse graph)
- Agents can invoke these tools during conversation
- ACP mediates permission requests

**Key Insight**: ACP is how the CLI **talks to external agents**, not a service layer between UI and core. The CLI remains the interface to Crucible's core functionality.

**LLM Integration**: No direct LLM integration in Crucible. Agents (claude-code, etc.) handle LLM communication. Long-term may explore custom A2A-inspired protocol for distributed agent interaction.

---

### 7. Scripting Layer (Mid-Term)

**Responsibility**: Integrated tools and extensibility via sandboxed scripts.

**Use Cases**:
- External API calls (e.g., fetch web content, call REST APIs)
- Internal DB operations (e.g., custom queries, data transformations)
- Custom parsers (e.g., new Markdown extensions)
- Agent-usable tools (searchable via semantic search)

**Candidates**:
1. **Rune**: Dynamic, Rust-native, supports attribute macros for schema generation
2. **Lua**: Mature ecosystem, but limited metaprogramming (trait abstraction harder)

**Abstraction** (trait-based for swappability):
```rust
pub trait ScriptRuntime {
    async fn execute(&self, script: String, context: ScriptContext) -> Result<ScriptResult>;
    fn register_tool(&self, tool: ToolDefinition) -> Result<()>;
    fn list_tools(&self) -> Vec<ToolDefinition>;
}

// Implementations:
// - RuneRuntime (preferred - attribute macros, Rust integration)
// - LuaRuntime (alternative - mature, less Rust-native)
```

**Tool Registry**:
- Tools have descriptions (natural language)
- Descriptions embedded via vector search
- Agents query: "find tool for X" → semantic search → tool list
- Inspired by [DeepAgent's RapidAPI semantic search](https://github.com/deepagent/rapidapi-semantic-search)

**Security**: In-process execution initially (limited userbase). Sandboxing becomes priority with broader adoption.

**Integration**: Scripts access DB via exposed core traits (not direct Surreal access).

---

## Performance & Scaling

### Performance Targets
**Philosophy**: Human-in-the-loop interaction, not fire-and-forget automation.

**Target Response Times** (informal, not strict SLAs):
- Semantic search: <1 second (interactive)
- Graph traversal: <100ms (near-instant)
- File processing: <500ms per file (incremental)
- Full vault re-index: <1 minute for 1000 notes (rare operation)

### Scaling Assumptions
- **Vault size**: ~1,000 - 10,000 notes (typical personal knowledge base)
- **Blocks per note**: 10 - 100 (average)
- **Total blocks**: ~10,000 - 1,000,000 (sparse graph)
- **Embeddings**: ~10,000 - 1,000,000 vectors (fastembed local inference)

### Caching Strategy
**Database IS the cache layer**:
- All metadata, graph, embeddings stored in SurrealDB (embedded, same binary)
- No cloud dependency (offline-first)
- Merkle tree diffing minimizes re-processing overhead
- Incremental updates keep database fresh without full re-index

**Optimization Techniques**:
- Pre-computed embeddings (generated once, queried many times)
- In-memory graph cache (hot paths)
- Lazy loading of Merkle tree VNodes (100-capacity chunks)
- Index tuning (SurrealDB vector search optimizations)

---

## Interface Layer

### CLI Chat Shell (Short-Term)
**Current Interface**: Primary interaction method during ACP MVP.

**Features**:
- Natural language queries (not SQL REPL)
- File processing on startup (incremental, hash-based)
- Status and diff commands
- Routes all queries through ACP → core traits

**NOT a REPL**: Phasing out direct SurrealQL access in favor of ACP chat interface.

### Desktop App (Mid/Long-Term)
**Scope**: Tauri-based desktop app, validates "unified core" architecture.

**Features**:
- Visual graph navigation
- Richer UX (sidebar, split panes, preview)
- Same core traits as CLI (proves abstraction works)
- Potential ACP support (TBD)

**Timeline**: Long-term goal. Editor-agnostic approach (external editors) prioritized for early testers.

### Editor Integration (Long-Term)
**External Editors** (short/mid-term):
- File watching detects changes in VS Code, Obsidian, Neovim, etc.
- Incremental re-processing on save

**Built-In Editor** (long-term):
- Desktop app with integrated editor
- Requires substantial changes (new trait or implementation)
- Far off - current focus is external editor support

---

## Open Questions & Future Research

### Short-Term
1. **Embedding content normalization**: Does markup affect embedding quality? (Test raw vs normalized)
2. **Short block semantics**: How to preserve semantic data for <5 word blocks? (Index by parent? Skip?)
3. **ACP wrapper**: Do we need `crucible-acp` crate, or use Zed's implementation directly?

### Mid-Term
4. **Scripting language choice**: Rune vs Lua? (Rune preferred for Rust integration, but Lua more mature)
5. **Tool registry UX**: How do users discover/manage tools? (CLI list? Visual registry in desktop?)
6. **Desktop architecture**: Direct core access or ACP layer? (May depend on UI framework)

### Long-Term
7. **A2A protocol design**: Federated agent interaction - what's the contract? (Speculative, may not happen)
8. **CRDT integration**: If needed for enterprise multiplayer, how does it interact with Merkle trees? (Complementary or replacement?)
9. **Custom Markdown extensions**: Plugin-defined syntax - how to avoid fragmentation? (Standard library of extensions?)

---

## References

### External Inspirations
- **Oxen AI**: [Merkle tree implementation](https://github.com/Oxen-AI/Oxen/tree/2eaf17867152e9fdfba4ef9813ba5f6289a210ef/oxen-rust/src/lib/src/model/merkle_tree), EPR schema design
- **Obsidian**: [Obsidian Flavored Markdown](https://help.obsidian.md/obsidian-flavored-markdown), [Callouts](https://help.obsidian.md/callouts)
- **Agent Client Protocol**: [Official Protocol Site](https://agentclientprotocol.com), [Rust SDK](https://github.com/agentclientprotocol/agent-client-protocol)
- **DeepAgent**: RapidAPI semantic tool search
- **A2A Protocol**: [Anytype's Agent-to-Agent protocol](https://tech.anytype.io/a2a-protocol) (long-term inspiration)

### Internal Documentation
- **[STATUS.md](../STATUS.md)**: Current refactor status, guiding principles, next work
- **[README.md](../README.md)**: Project overview, quick start, feature highlights
- **[AGENTS.md](../AGENTS.md)**: AI agent guide for contributing to codebase
- **[OpenSpec](../openspec/AGENTS.md)**: Change proposal workflow

### Research Documents
- **[Oxen AI Merkle Tree Research](./research/README.md)**: Comprehensive analysis of Oxen's implementation
  - [Technical Analysis](./research/oxen-merkle-tree-analysis.md): VNode architecture, change detection algorithms
  - [Diagrams](./research/oxen-merkle-tree-diagrams.md): Visual representations of tree structure
  - [Implementation Guide](./research/crucible-merkle-tree-implementation-guide.md): Phase-by-phase roadmap for Crucible
- **[ACP Research Report](./ACP-RESEARCH-REPORT.md)**: Deep dive into Agent Client Protocol
  - Protocol overview and architecture
  - Integration patterns for CLI applications
  - Code examples and best practices

---

*This architecture document is a living reference. Update it as decisions are made and implementation evolves.*
