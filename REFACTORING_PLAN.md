# Crucible Refactoring Plan
## Focus: EPR Schema, Hybrid Merkle Trees, and DB Layer Cleanup

**Date:** 2025-11-07
**Goal:** Clean up technical debt using concrete recommendations from exploration document
**Reference:** `EXTRACTION_SUMMARY.md` (771 lines of detailed design)

---

## Executive Summary

Based on the comprehensive analysis in the exploration document, we have **two concrete goals**:

### Goal 1: Implement EPR Schema 🎯
Migrate from current schema to **Entity-Property-Relation** model for plugin extensibility without migrations.

### Goal 2: Hybrid Merkle Trees 🌲
Implement **two-level hybrid approach**: n-ary trees at section level + binary trees for blocks.

---

## Current State

### Problems
1. **26,372 lines of CLI tests** - mostly obsolete from prototyping
2. **28 source files in crucible-surrealdb** - overlapping abstractions
3. **Schema not extensible** - rigid table structure requires migrations for new features
4. **Merkle trees are binary-only** - doesn't match document mental model (sections)
5. **No plugin system** - hardcoded features instead of extensible architecture

### Strengths (Keep These!)
- ✅ SurrealDB with RocksDB backend (validated by analysis)
- ✅ BLAKE3 hashing (fast and proven)
- ✅ Block-level storage exists (`document_blocks` table)
- ✅ AST parser working (just needs better integration)

---

## Goal 1: EPR Schema Migration

### Why EPR?

From the exploration document:
> "The core insight: **separation of structure (entities, relations) from semantics (properties)**"

**Benefits:**
- Plugins can add properties without schema migrations
- Flexible metadata (any JSON structure)
- Graph queries work naturally
- Future-proof extensibility

### Current Schema → EPR Mapping

**Current:**
```sql
-- Rigid structure
DEFINE TABLE notes SCHEMAFULL;
DEFINE FIELD path ON TABLE notes TYPE string;
DEFINE FIELD title ON TABLE notes TYPE string;
DEFINE FIELD content ON TABLE notes TYPE string;
-- ... 15+ more fields
```

**EPR:**
```sql
-- Flexible structure
DEFINE TABLE entities SCHEMAFULL;
DEFINE FIELD id ON TABLE entities TYPE string;
DEFINE FIELD entity_type ON TABLE entities TYPE string;  -- "note", "block", "tag"

DEFINE TABLE properties SCHEMAFULL;
DEFINE FIELD entity_id ON TABLE properties TYPE record<entities>;
DEFINE FIELD key ON TABLE properties TYPE string;       -- "path", "title", "content"
DEFINE FIELD value ON TABLE properties TYPE string;     -- JSON-encoded
DEFINE FIELD value_type ON TABLE properties TYPE string; -- "text", "json", "number"
```

### New Schema Design

Based on extraction summary (lines 84-300), here's the complete EPR schema:

```sql
-- ============================================================================
-- CORE EPR TABLES
-- ============================================================================

-- 1. ENTITIES (universal base)
DEFINE TABLE entities SCHEMAFULL;

DEFINE FIELD id ON TABLE entities
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD entity_type ON TABLE entities
    TYPE string
    ASSERT $value IN ["note", "block", "tag", "section", "media"];

DEFINE FIELD created_at ON TABLE entities
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE entities
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD content_hash ON TABLE entities
    TYPE option<string>;  -- BLAKE3 hash for change detection

DEFINE FIELD version ON TABLE entities
    TYPE int
    DEFAULT 1;  -- Optimistic locking

DEFINE INDEX unique_entity_id ON TABLE entities COLUMNS id UNIQUE;
DEFINE INDEX entity_type_idx ON TABLE entities COLUMNS entity_type;
DEFINE INDEX content_hash_idx ON TABLE entities COLUMNS content_hash;

-- 2. PROPERTIES (flexible metadata)
DEFINE TABLE properties SCHEMAFULL;

DEFINE FIELD entity_id ON TABLE properties
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD namespace ON TABLE properties
    TYPE string
    DEFAULT "core";  -- "core", "user", "plugin:task-manager", etc.

DEFINE FIELD key ON TABLE properties
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD value ON TABLE properties
    TYPE string;  -- JSON-encoded for flexibility

DEFINE FIELD value_type ON TABLE properties
    TYPE string
    DEFAULT "text";  -- "text", "json", "number", "boolean", "date"

-- Typed columns for efficient queries
DEFINE FIELD value_text ON TABLE properties TYPE option<string>;
DEFINE FIELD value_number ON TABLE properties TYPE option<float>;
DEFINE FIELD value_bool ON TABLE properties TYPE option<bool>;
DEFINE FIELD value_date ON TABLE properties TYPE option<datetime>;

DEFINE FIELD created_at ON TABLE properties
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE properties
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX entity_key_idx ON TABLE properties COLUMNS entity_id, namespace, key;
DEFINE INDEX namespace_key_idx ON TABLE properties COLUMNS namespace, key;

-- 3. RELATIONS (typed graph edges)
DEFINE TABLE relations SCHEMAFULL
    TYPE RELATION
    FROM entities TO entities;

DEFINE FIELD relation_type ON TABLE relations
    TYPE string
    ASSERT $value != NONE;  -- "wikilink", "embeds", "child_of", "tagged_with", etc.

DEFINE FIELD weight ON TABLE relations
    TYPE float
    DEFAULT 1.0;  -- For weighted graph algorithms

DEFINE FIELD confidence ON TABLE relations
    TYPE float
    DEFAULT 1.0;  -- For ML-generated relations

DEFINE FIELD source ON TABLE relations
    TYPE string
    DEFAULT "user";  -- "user", "parser", "ml", "plugin:name"

DEFINE FIELD metadata ON TABLE relations
    TYPE object
    DEFAULT {};  -- Flexible per-relation metadata

DEFINE FIELD created_at ON TABLE relations
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX relation_type_idx ON TABLE relations COLUMNS relation_type;
DEFINE INDEX source_idx ON TABLE relations COLUMNS source;

-- 4. BLOCKS (AST nodes for merkle trees)
DEFINE TABLE blocks SCHEMAFULL;

DEFINE FIELD id ON TABLE blocks
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD entity_id ON TABLE blocks
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD block_index ON TABLE blocks
    TYPE int
    ASSERT $value >= 0;

DEFINE FIELD block_type ON TABLE blocks
    TYPE string
    ASSERT $value != NONE;  -- "heading", "paragraph", "list_item", "code_block"

DEFINE FIELD content ON TABLE blocks
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD content_hash ON TABLE blocks
    TYPE string
    ASSERT $value != NONE AND string::len($value) == 64;

-- Positioning
DEFINE FIELD start_offset ON TABLE blocks TYPE int;
DEFINE FIELD end_offset ON TABLE blocks TYPE int;
DEFINE FIELD start_line ON TABLE blocks TYPE int;
DEFINE FIELD end_line ON TABLE blocks TYPE int;

-- Hierarchy (for nested structures)
DEFINE FIELD parent_block_id ON TABLE blocks TYPE option<record<blocks>>;
DEFINE FIELD depth ON TABLE blocks TYPE int DEFAULT 0;

-- Metadata
DEFINE FIELD metadata ON TABLE blocks
    TYPE object
    DEFAULT {};  -- AST metadata (heading level, language, etc.)

DEFINE FIELD created_at ON TABLE blocks
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE blocks
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX entity_block_idx ON TABLE blocks COLUMNS entity_id, block_index UNIQUE;
DEFINE INDEX block_hash_idx ON TABLE blocks COLUMNS content_hash;
DEFINE INDEX block_type_idx ON TABLE blocks COLUMNS block_type;

-- 5. EMBEDDINGS (vector search)
DEFINE TABLE embeddings SCHEMAFULL;

DEFINE FIELD entity_id ON TABLE embeddings
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD block_id ON TABLE embeddings
    TYPE option<record<blocks>>;  -- Optional: block-level embeddings

DEFINE FIELD embedding ON TABLE embeddings
    TYPE array<float>
    ASSERT $value != NONE;

DEFINE FIELD model ON TABLE embeddings
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD dimensions ON TABLE embeddings
    TYPE int
    ASSERT $value > 0;

DEFINE FIELD created_at ON TABLE embeddings
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX unique_entity_embedding ON TABLE embeddings COLUMNS entity_id, model UNIQUE;
DEFINE INDEX embedding_vector_idx ON TABLE embeddings
    COLUMNS embedding
    MTREE DIMENSION 384 DISTANCE COSINE;  -- Adjust dimension as needed

-- 6. PLUGIN SCHEMAS (extensibility)
DEFINE TABLE plugin_schemas SCHEMAFULL;

DEFINE FIELD plugin_id ON TABLE plugin_schemas
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD schema_version ON TABLE plugin_schemas
    TYPE string;

DEFINE FIELD property_definitions ON TABLE plugin_schemas
    TYPE array<object>;  -- JSON schema for plugin properties

DEFINE FIELD relation_definitions ON TABLE plugin_schemas
    TYPE array<object>;  -- Relation types this plugin adds

DEFINE FIELD created_at ON TABLE plugin_schemas
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX unique_plugin_id ON TABLE plugin_schemas COLUMNS plugin_id UNIQUE;
```

### Migration Strategy

**Phase 1: Parallel Schema (Zero Downtime)**
1. Create new EPR tables alongside existing `notes` table
2. Write to both schemas during transition
3. Read from EPR, fall back to `notes` if missing

**Phase 2: Data Migration**
```rust
async fn migrate_notes_to_epr(db: &Database) -> Result<()> {
    let notes = db.query("SELECT * FROM notes").await?;

    for note in notes {
        // 1. Create entity
        let entity_id = format!("note:{}", note.id);
        db.create("entities")
            .content(Entity {
                id: entity_id.clone(),
                entity_type: "note".into(),
                content_hash: Some(note.file_hash),
                ..Default::default()
            })
            .await?;

        // 2. Create properties
        create_property(&db, &entity_id, "core", "path", &note.path).await?;
        create_property(&db, &entity_id, "core", "title", &note.title).await?;
        create_property(&db, &entity_id, "core", "content", &note.content).await?;

        // 3. Migrate tags to relations
        for tag in note.tags {
            create_relation(&db, &entity_id, &format!("tag:{}", tag), "tagged_with").await?;
        }

        // 4. Migrate blocks
        // ... (from existing document_blocks table)
    }

    Ok(())
}
```

**Phase 3: Cutover**
1. Stop writing to old schema
2. Verify data integrity
3. Drop old tables (after backup!)

---

## Goal 2: Filesystem-Mirroring N-ary Merkle Tree

### Core Concept

The merkle tree is a **1:1 mirror of the filesystem structure**:
- Directories → Directory nodes (n-ary)
- Files → File nodes (contain AST block hashes)
- VNodes applied dynamically to ANY large container

**No fixed levels - tree depth = filesystem depth**

### Structure

```
Kiln Root
├─ Projects/                           ← Directory Node
│  ├─ crucible/                        ← Directory Node
│  │  ├─ architecture.md               ← File Node
│  │  │  └─ [block₁, block₂, ...]     ← AST blocks (binary tree for hashing)
│  │  └─ roadmap.md                    ← File Node
│  │     └─ [block₁, block₂, ...]
│  └─ personal/                        ← Directory Node (VNode if 1000s of files)
│     └─ ...
└─ Daily Notes/                        ← Directory Node
   ├─ 2025/                            ← Directory Node
   │  └─ 01/                           ← Directory Node
   │     └─ 2025-01-15.md              ← File Node (VNode if 10000s of blocks)
   └─ ...
```

**VNodes Applied Dynamically:**
- Folder with 5000 files? → VNode (lazy load)
- Document with 10000 blocks? → VNode (lazy load)
- Small folder/file? → Regular node (load all)

### Benefits

✅ **Matches mental model**: Tree structure = filesystem structure
✅ **Semantic queries**: "Which folder changed?" "Which file?" "Which block?"
✅ **Efficient**: Only load what you need via VNodes
✅ **Flexible**: VNode threshold applies universally
✅ **Simple**: No artificial layers or groupings

### Implementation

**File: `crates/crucible-core/src/merkle/hybrid.rs`** (new)

```rust
use crate::types::BlockHash;
use crate::parser::AstNode;

/// Hybrid merkle tree: sections at top level, binary trees for blocks
pub struct HybridMerkleTree {
    pub root_hash: BlockHash,
    pub sections: Vec<SectionNode>,
    pub total_blocks: usize,
}

/// Section-level node (n-ary tree based on document structure)
pub struct SectionNode {
    pub section_hash: BlockHash,
    pub heading: Option<AstNode>,  // The heading that starts this section
    pub depth: usize,              // Heading level (1-6)
    pub blocks: Vec<AstNode>,      // Direct children blocks
    pub binary_tree: BinaryMerkleTree,  // Efficient hashing of blocks
    pub children: Vec<SectionNode>,     // Nested subsections
}

/// Binary merkle tree for blocks within a section
pub struct BinaryMerkleTree {
    pub root_hash: BlockHash,
    pub nodes: Vec<MerkleNode>,
    pub height: usize,
}

pub enum MerkleNode {
    Leaf { hash: BlockHash, block_index: usize },
    Internal { hash: BlockHash, left: usize, right: usize },
}

impl HybridMerkleTree {
    /// Build tree from parsed AST
    pub fn from_ast(document_id: &str, ast: &[AstNode]) -> Self {
        let sections = Self::group_into_sections(ast);
        let root_hash = Self::compute_root_hash(&sections);

        Self {
            root_hash,
            sections,
            total_blocks: ast.len(),
        }
    }

    /// Group AST nodes into hierarchical sections based on headings
    fn group_into_sections(ast: &[AstNode]) -> Vec<SectionNode> {
        let mut sections = Vec::new();
        let mut current_section: Option<SectionNode> = None;
        let mut current_blocks = Vec::new();

        for node in ast {
            match node.node_type {
                AstNodeType::Heading(level) => {
                    // Finish previous section
                    if let Some(section) = current_section.take() {
                        let binary_tree = BinaryMerkleTree::from_blocks(&current_blocks);
                        sections.push(SectionNode {
                            section_hash: binary_tree.root_hash.clone(),
                            heading: section.heading,
                            depth: section.depth,
                            blocks: current_blocks.clone(),
                            binary_tree,
                            children: vec![],  // TODO: Handle nested sections
                        });
                        current_blocks.clear();
                    }

                    // Start new section
                    current_section = Some(SectionNode {
                        section_hash: BlockHash::default(),
                        heading: Some(node.clone()),
                        depth: level,
                        blocks: vec![],
                        binary_tree: BinaryMerkleTree::default(),
                        children: vec![],
                    });
                }
                _ => {
                    current_blocks.push(node.clone());
                }
            }
        }

        // Finish last section
        if let Some(_) = current_section.take() {
            let binary_tree = BinaryMerkleTree::from_blocks(&current_blocks);
            sections.push(SectionNode {
                section_hash: binary_tree.root_hash.clone(),
                heading: None,
                depth: 0,
                blocks: current_blocks,
                binary_tree,
                children: vec![],
            });
        }

        sections
    }

    /// Compute root hash from all section hashes
    fn compute_root_hash(sections: &[SectionNode]) -> BlockHash {
        let section_hashes: Vec<&BlockHash> = sections
            .iter()
            .map(|s| &s.section_hash)
            .collect();

        // Use binary tree to combine section hashes
        BinaryMerkleTree::combine_hashes(&section_hashes)
    }

    /// Diff two hybrid trees to find changed sections
    pub fn diff(&self, other: &HybridMerkleTree) -> HybridDiff {
        let mut changed_sections = Vec::new();

        for (i, (old_section, new_section)) in self.sections.iter().zip(other.sections.iter()).enumerate() {
            if old_section.section_hash != new_section.section_hash {
                // Section changed - diff at block level
                let block_diff = old_section.binary_tree.diff(&new_section.binary_tree);
                changed_sections.push(SectionChange {
                    section_index: i,
                    heading: new_section.heading.clone(),
                    block_changes: block_diff,
                });
            }
        }

        HybridDiff {
            root_hash_changed: self.root_hash != other.root_hash,
            changed_sections,
            added_sections: other.sections.len().saturating_sub(self.sections.len()),
            removed_sections: self.sections.len().saturating_sub(other.sections.len()),
        }
    }
}

pub struct HybridDiff {
    pub root_hash_changed: bool,
    pub changed_sections: Vec<SectionChange>,
    pub added_sections: usize,
    pub removed_sections: usize,
}

pub struct SectionChange {
    pub section_index: usize,
    pub heading: Option<AstNode>,
    pub block_changes: BlockDiff,
}

pub struct BlockDiff {
    pub changed_blocks: Vec<usize>,  // Indices of changed blocks
}

impl BinaryMerkleTree {
    /// Build binary tree from list of blocks
    pub fn from_blocks(blocks: &[AstNode]) -> Self {
        // Standard binary merkle tree construction
        // ... implementation
        todo!()
    }

    /// Combine list of hashes into single root hash
    pub fn combine_hashes(hashes: &[&BlockHash]) -> BlockHash {
        // Binary tree combination
        // ... implementation
        todo!()
    }

    /// Diff two binary trees
    pub fn diff(&self, other: &BinaryMerkleTree) -> BlockDiff {
        // ... implementation
        todo!()
    }
}

impl Default for BinaryMerkleTree {
    fn default() -> Self {
        Self {
            root_hash: BlockHash::default(),
            nodes: vec![],
            height: 0,
        }
    }
}
```

### VNode Optimization for Large Documents

**File: `crates/crucible-core/src/merkle/vnode.rs`** (new)

```rust
/// Virtual node for lazy-loading large document sections
pub struct VirtualNode {
    pub node_id: String,
    pub block_count: usize,
    pub root_hash: BlockHash,
    pub sections: LazyLoad<Vec<SectionNode>>,
}

pub enum LazyLoad<T> {
    NotLoaded { storage_key: String },
    Loading,
    Loaded(T),
}

impl VirtualNode {
    /// Create VNode for documents >1000 blocks
    pub fn new(node_id: String, block_count: usize, root_hash: BlockHash) -> Self {
        Self {
            node_id: node_id.clone(),
            block_count,
            root_hash,
            sections: LazyLoad::NotLoaded {
                storage_key: format!("vnode:{}", node_id),
            },
        }
    }

    /// Load sections on demand
    pub async fn load_sections(&mut self, db: &Database) -> Result<&Vec<SectionNode>> {
        match &self.sections {
            LazyLoad::Loaded(sections) => Ok(sections),
            LazyLoad::NotLoaded { storage_key } => {
                self.sections = LazyLoad::Loading;
                let sections = db.load_sections(storage_key).await?;
                self.sections = LazyLoad::Loaded(sections);

                if let LazyLoad::Loaded(sections) = &self.sections {
                    Ok(sections)
                } else {
                    unreachable!()
                }
            }
            LazyLoad::Loading => {
                Err(anyhow::anyhow!("Already loading"))
            }
        }
    }
}
```

---

## Implementation Plan

### Phase 1: Delete All Tests (5 minutes)
✅ **Concrete outcome:** Zero test failures - clean slate across entire workspace

**Rationale:**
- Tests were written during prototyping, don't match final design
- Many tests force unnecessary code to exist
- Better to refactor core, then write tests that match new design
- Prevents test failures from slowing down refactoring
- Unit tests in `#[cfg(test)]` modules written as we go

**Steps:**
```bash
# Delete all integration tests across the workspace
find crates/ -type d -name tests -exec rm -rf {} +

# Verify no tests remain
find crates/ -type d -name tests

# Verify workspace builds
cargo build

# Verify no tests run
cargo test --workspace
# Should output: "0 tests, 0 passed"
```

**What gets deleted:**
- `crates/crucible-cli/tests/` (26,372 lines)
- `crates/crucible-surrealdb/tests/`
- `crates/crucible-core/tests/`
- Any other crate test directories

**Success criteria:**
- [ ] All `tests/` directories deleted
- [ ] Workspace builds successfully
- [ ] `cargo test --workspace` shows 0 tests
- [ ] Ready to refactor without test interference

---

### Phase 2: EPR Schema Migration (2-3 days)
✅ **Concrete outcome:** New EPR tables + migration script

**Steps:**

**Day 1: Schema Creation**
1. Create new schema file `crates/crucible-surrealdb/src/schema_epr.surql`
2. Copy EPR table definitions from `EPR_SCHEMA_SUMMARY.md`
3. Test schema in isolated database
   ```bash
   surreal start --log trace memory
   # Import schema
   ```

4. Create Rust types in `crates/crucible-surrealdb/src/epr_types.rs`
   - Reference: `EPR_SCHEMA_SUMMARY.md` for complete types

**Day 2: Core EPR Operations**
1. Implement basic CRUD for entities, properties, relations
2. Add query builders for common patterns
3. Write unit tests inline (`#[cfg(test)]`) as we go
4. NO migration yet - focus on clean implementation

**Day 3: Migration & Integration**
1. Create migration script to convert existing data
2. Test migration on copy of test vault
3. Verify data integrity
4. Update public APIs to use EPR

**Success criteria:**
- [ ] EPR tables created and tested
- [ ] Core operations work
- [ ] Migration completes successfully
- [ ] Only essential tests written

---

### Phase 3: Filesystem-Mirroring Merkle Trees (2-3 days)
✅ **Concrete outcome:** Working hybrid tree implementation

**Steps:**

**Day 1: Core Implementation**
1. Create `crates/crucible-core/src/merkle/hybrid.rs`
2. Implement `HybridMerkleTree::from_ast()`
3. Implement section grouping logic
4. Unit tests for tree construction

**Day 2: Binary Tree Updates**
1. Refactor existing binary tree as `BinaryMerkleTree`
2. Implement `from_blocks()` method
3. Add `combine_hashes()` method
4. Unit tests for binary operations

**Day 3: Diffing & Integration**
1. Implement `HybridMerkleTree::diff()`
2. Add `SectionChange` detection
3. Integrate with parser pipeline
4. Integration tests with real documents

**Success criteria:**
- [ ] Trees build from AST correctly
- [ ] Section-level diffing works
- [ ] Block-level diffing works within sections
- [ ] Performance benchmarks pass
- [ ] Integration tests pass

---

### Phase 4: VNode Optimization (1 day)
✅ **Concrete outcome:** Lazy loading for large documents

**Steps:**
1. Create `crates/crucible-core/src/merkle/vnode.rs`
2. Implement `VirtualNode` with lazy loading
3. Add threshold detection (>1000 blocks)
4. Test with large documents

**Success criteria:**
- [ ] VNodes created for large documents
- [ ] Lazy loading works
- [ ] Memory usage reduced for large vaults
- [ ] Performance improvement verified

---

### Phase 5: DB Layer Cleanup (2-3 days)
✅ **Concrete outcome:** 28 files → 11 files

Based on earlier analysis, consolidate to:
1. `lib.rs` - Public API
2. `database.rs` - DB client + batching
3. `schema.rs` - EPR types
4. `kiln.rs` - Kiln processing
5. `blocks.rs` - Block storage
6. `merkle.rs` - Merkle operations
7. `embeddings.rs` - Embedding pipeline
8. `transactions.rs` - Transaction handling
9. `migration.rs` - Schema migrations
10. `query.rs` - Query builders
11. `metrics.rs` - Observability

**Steps:**
1. Create new consolidated files
2. Move code with clear ownership
3. Remove duplicates
4. Update imports
5. Run tests continuously
6. Document changes

**Success criteria:**
- [ ] File count reduced to 11
- [ ] No duplicate logic
- [ ] Clear module boundaries
- [ ] All tests pass
- [ ] Documentation updated

---

### Phase 6: ACP Integration (1-2 days)
✅ **Concrete outcome:** Working chat with context injection

**Steps:**
1. Add `agent-client-protocol` dependency
2. Create `crates/crucible-acp/` crate
3. Implement `Client` trait
4. Add `cru chat` command
5. Test with claude-code

**Success criteria:**
- [ ] ACP client works
- [ ] Context injection works
- [ ] Chat loop functional
- [ ] Streaming responses work

---

## Timeline

| Phase | Duration | Days |
|-------|----------|------|
| 1. Delete All Tests | 5 minutes | 0.01 |
| 2. EPR Schema Migration | 2-3 days | 2.5 |
| 3. Filesystem Merkle Trees | 2-3 days | 2.5 |
| 4. VNode Hash-Based Sharding | 1-2 days | 1.5 |
| 5. DB Layer Cleanup | 2-3 days | 2.5 |
| 6. ACP Integration | 1-2 days | 1.5 |

**Total: ~10.5 days** (with buffer for debugging)

**Note:** Phase 1 deletes ALL tests upfront - no test failures during refactoring!

---

## Success Metrics

### Code Health
- [ ] Tests deleted from all crates (clean slate)
- [ ] DB layer: 28 → 11 files (61% reduction)
- [ ] Schema is extensible (plugins don't need migrations)
- [ ] Merkle trees mirror filesystem exactly
- [ ] Only essential tests written inline

### Functionality
- [ ] All existing features work
- [ ] Chat with context works
- [ ] Section-level diffing works
- [ ] Performance acceptable

### Architecture
- [ ] EPR model implemented
- [ ] Hybrid merkle trees working
- [ ] Plugin foundation ready
- [ ] Clean module boundaries

---

## Next Steps

1. **Review this plan** ✓ (you're reading it!)
2. **Create refactoring branch**: `git checkout -b refactor/epr-and-hybrid-merkle`
3. **Start Phase 1**: Archive CLI tests
4. **Work through phases** systematically
5. **Document learnings** as we go

---

## Notes

- Each phase should be a separate commit
- Write tests first when possible (TDD)
- Keep `main` branch stable
- Celebrate progress! 🎉

**Reference Documents:**
- `EXTRACTION_SUMMARY.md` - Full design details (771 lines)
- `ACP-MVP.md` - MVP requirements
- `claude-exploration-merkle-trees-and-Rune-p1.md` - Original analysis
