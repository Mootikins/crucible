# Architecture Refactoring: Proper Module Placement for optimize-data-flow

## Problem Analysis

The current Phase 1 implementation places file hashing and change detection logic in `crucible-surrealdb`, violating SOLID principles:

**Current (Incorrect)**:
```
crucible-surrealdb/
├── kiln_scanner.rs          ❌ File I/O + hashing (not database concern)
├── kiln_processor.rs        ❌ File processing logic (not database concern)
├── hash_lookup.rs           ✅ Database queries (correct placement)
└── content_addressed_storage.rs ✅ Storage implementation (correct)
```

**Violations**:
- ❌ **Single Responsibility**: SurrealDB crate should only handle database operations
- ❌ **Dependency Inversion**: File scanning shouldn't depend on concrete database
- ❌ **Separation of Concerns**: File I/O mixed with database code

## Architectural Principles

### Dependency Hierarchy (Current)

```
crucible-cli
    ↓
crucible-surrealdb ← ❌ PROBLEM: Contains file scanning + DB logic
    ↓
crucible-core (traits only)
```

### Correct Dependency Hierarchy

```
crucible-cli (orchestration)
    ↓
crucible-watch (file watching + change detection)
    ↓
crucible-core (traits: ContentHasher, ChangeDetector)
    ↑
crucible-surrealdb (implements: Storage, ContentAddressedStorage)
```

**Key insight**: File operations and database operations should be **separate**, both depending on `crucible-core` traits.

## Module Responsibilities

### crucible-core (Foundation Layer)

**Responsibility**: Define abstractions, no concrete implementations

**Contents**:
- ✅ Traits: `ContentHasher`, `ChangeDetector`, `ContentAddressedStorage`
- ✅ Types: `FileHash`, `MerkleTree`, `BlockHash`, `ChangeSet`
- ✅ Algorithms: BLAKE3/SHA256 hashing implementations (pure functions)
- ❌ NO: File I/O, database access, specific storage backends

**Why**: Enables dependency inversion - higher layers depend on abstractions

### crucible-watch (File System Layer)

**Responsibility**: File system operations and change detection

**Contents**:
- ✅ File discovery and scanning
- ✅ File hashing (using `crucible-core` hashers)
- ✅ Change detection (comparing hashes)
- ✅ File watching with `notify` crate
- ✅ Event emission for file changes
- ❌ NO: Database queries, storage implementation details

**Why**: Single responsibility - everything related to files and filesystem

### crucible-surrealdb (Persistence Layer)

**Responsibility**: Database operations and storage implementations

**Contents**:
- ✅ Hash lookup queries (`hash_lookup.rs`)
- ✅ Content-addressed storage implementation
- ✅ Database schema and migrations
- ✅ Batch operations and caching
- ❌ NO: File I/O, hashing files, scanning directories

**Why**: Single responsibility - everything related to database persistence

### crucible-cli (Application Layer)

**Responsibility**: Orchestrate components, inject dependencies

**Contents**:
- ✅ Wire up file scanner + database storage
- ✅ Configuration loading
- ✅ Command handling
- ✅ Dependency injection
- ❌ NO: Business logic (delegate to lower layers)

**Why**: Thin orchestration layer following Hexagonal Architecture

## Refactoring Plan by Phase

### Phase 1: File-Level Change Detection (CURRENT - Needs Refactoring)

#### Move `kiln_scanner.rs` Logic

**From**: `crucible-surrealdb/src/kiln_scanner.rs`

**To**: Split into three modules:

1. **`crucible-core/src/hashing/file_hasher.rs`** (pure hashing logic)
```rust
pub struct FileHasher {
    algorithm: HashAlgorithm, // BLAKE3 or SHA256
}

impl FileHasher {
    /// Hash a file using streaming I/O
    pub async fn hash_file(&self, path: &Path) -> Result<FileHash>;

    /// Hash multiple files in parallel
    pub async fn hash_files_batch(&self, paths: &[PathBuf]) -> Result<Vec<FileHash>>;
}
```

2. **`crucible-watch/src/file_scanner.rs`** (file discovery + hashing)
```rust
pub struct FileScanner {
    config: ScannerConfig,
    hasher: Arc<dyn ContentHasher>, // Trait from crucible-core
}

impl FileScanner {
    /// Discover files in directory and compute hashes
    pub async fn scan_directory(&self, path: &Path) -> Result<Vec<FileInfo>>;

    /// Watch directory for changes
    pub async fn watch_directory(&self, path: &Path) -> WatchStream;
}

pub struct FileInfo {
    pub path: PathBuf,
    pub relative_path: String,
    pub content_hash: FileHash,
    pub size: u64,
    pub modified: SystemTime,
}
```

3. **`crucible-watch/src/change_detector.rs`** (compare hashes, detect changes)
```rust
pub struct ChangeDetector {
    storage: Arc<dyn HashLookupStorage>, // Trait from crucible-core
}

impl ChangeDetector {
    /// Detect which files have changed
    pub async fn detect_changes(&self,
        discovered_files: &[FileInfo]
    ) -> Result<ChangeSet>;
}

pub struct ChangeSet {
    pub unchanged: Vec<FileInfo>,
    pub changed: Vec<FileInfo>,
    pub new: Vec<FileInfo>,
    pub deleted: Vec<String>, // paths
}
```

#### Keep in `crucible-surrealdb`

**`hash_lookup.rs`** - Already correct placement
```rust
// Implements HashLookupStorage trait from crucible-core
pub struct SurrealHashLookup {
    client: SurrealClient,
}

impl HashLookupStorage for SurrealHashLookup {
    async fn lookup_hashes(&self, paths: &[String]) -> Result<HashMap<String, StoredHash>>;
    async fn store_hashes(&self, files: &[FileInfo]) -> Result<()>;
}
```

### Phase 2: AST Block-Based Hashing

#### crucible-parser (Already exists, extend it)

**`crucible-parser/src/block_extractor.rs`**
```rust
pub struct BlockExtractor;

impl BlockExtractor {
    /// Extract AST blocks from parsed document
    pub fn extract_blocks(&self, doc: &ParsedDocument) -> Vec<ASTBlock>;
}

pub struct ASTBlock {
    pub node_type: NodeType, // Heading, Paragraph, List, Code
    pub content: String,
    pub start_offset: usize,
    pub end_offset: usize,
}
```

#### crucible-core

**`crucible-core/src/hashing/block_hasher.rs`**
```rust
pub struct BlockHasher {
    algorithm: HashAlgorithm,
}

impl BlockHasher {
    /// Hash an AST block
    pub fn hash_block(&self, block: &ASTBlock) -> BlockHash;

    /// Build Merkle tree from block hashes
    pub fn build_merkle_tree(&self, blocks: &[ASTBlock]) -> MerkleTree;
}
```

#### crucible-surrealdb

**`crucible-surrealdb/src/block_storage.rs`**
```rust
// Implements ContentAddressedStorage trait
pub struct BlockStorage {
    client: SurrealClient,
}

impl ContentAddressedStorage for BlockStorage {
    async fn store_block(&self, hash: &BlockHash, data: &[u8]) -> Result<()>;
    async fn get_block(&self, hash: &BlockHash) -> Result<Option<Vec<u8>>>;
    async fn store_tree(&self, root: &str, tree: &MerkleTree) -> Result<()>;
}
```

### Phase 3: Content-Addressed Block Storage

#### crucible-core

**`crucible-core/src/storage/deduplicator.rs`**
```rust
pub struct Deduplicator {
    storage: Arc<dyn ContentAddressedStorage>,
}

impl Deduplicator {
    /// Find duplicate blocks across documents
    pub async fn find_duplicates(&self, blocks: &[BlockHash]) -> Result<DuplicateMap>;

    /// Compute deduplication statistics
    pub async fn compute_stats(&self) -> Result<DeduplicationStats>;
}
```

### Phase 4-7: Merkle Diffing, Embeddings, Search

Follow same pattern: **logic in `crucible-core` or `crucible-watch`, persistence in `crucible-surrealdb`**

## Migration Strategy

### Step 1: Create Traits in crucible-core

```rust
// crucible-core/src/traits/change_detection.rs

#[async_trait]
pub trait ContentHasher: Send + Sync {
    async fn hash_file(&self, path: &Path) -> Result<FileHash>;
    async fn hash_block(&self, block: &ASTBlock) -> Result<BlockHash>;
}

#[async_trait]
pub trait HashLookupStorage: Send + Sync {
    async fn lookup_hashes(&self, paths: &[String]) -> Result<HashMap<String, StoredHash>>;
    async fn store_hashes(&self, files: &[FileInfo]) -> Result<()>;
}

#[async_trait]
pub trait ChangeDetector: Send + Sync {
    async fn detect_changes(&self, files: &[FileInfo]) -> Result<ChangeSet>;
}
```

### Step 2: Move Code Incrementally

**Week 1**: Extract file hashing to `crucible-core/src/hashing/file_hasher.rs`
- Pure functions, no I/O
- Tests move with the code

**Week 2**: Move file scanning to `crucible-watch/src/file_scanner.rs`
- Use file_hasher from core
- Integrate with existing watch infrastructure

**Week 3**: Move change detection to `crucible-watch/src/change_detector.rs`
- Depend on `HashLookupStorage` trait
- SurrealDB implements the trait

**Week 4**: Update `crucible-cli` to wire dependencies
- Create FileScanner with FileHasher
- Create ChangeDetector with SurrealHashLookup
- Inject into processing pipeline

### Step 3: Update Tests

Each module should have its own tests:
- `crucible-core`: Unit tests for pure hashing functions
- `crucible-watch`: Integration tests with temp directories
- `crucible-surrealdb`: Database integration tests
- `crucible-cli`: End-to-end tests with real vault

## Benefits of This Architecture

### 1. **Testability**
- Mock hash lookup storage for testing change detection
- Test file scanning without database
- Test database queries without file I/O

### 2. **Reusability**
- File scanner can work with any storage backend (SurrealDB, PostgreSQL, SQLite)
- Change detector can work with any hash lookup implementation
- Hashing logic reusable across projects

### 3. **Maintainability**
- Clear module boundaries
- Each crate has single responsibility
- Easy to understand what goes where

### 4. **Flexibility**
- Swap out hash algorithm without touching database code
- Replace file watcher without touching storage
- Add new storage backends without changing file scanning

### 5. **SOLID Compliance**

**Single Responsibility**:
- crucible-watch: File operations
- crucible-surrealdb: Database operations
- crucible-core: Abstractions

**Open/Closed**:
- Extend behavior through traits
- Add new hash algorithms without modifying existing code

**Liskov Substitution**:
- Any `ContentHasher` implementation works
- Any `HashLookupStorage` implementation works

**Interface Segregation**:
- Small, focused traits
- Clients depend only on what they use

**Dependency Inversion**:
- High-level modules depend on abstractions
- Concrete implementations injected at runtime

## Updated Task Mapping

### Section 1: File-Level Change Detection

| Task | Current Location | Correct Location |
|------|-----------------|------------------|
| 1.1 Add content_hash to KilnFileInfo | ❌ crucible-surrealdb | ✅ crucible-watch |
| 1.2 BLAKE3 streaming hash | ❌ crucible-surrealdb | ✅ crucible-core |
| 1.3 Database schema | ✅ crucible-surrealdb | ✅ crucible-surrealdb |
| 1.4 Hash lookup queries | ✅ crucible-surrealdb | ✅ crucible-surrealdb |
| 1.5 Skip unchanged files | ❌ crucible-surrealdb | ✅ crucible-watch |

### Section 2: AST Block Hashing

| Task | Correct Location | Rationale |
|------|-----------------|-----------|
| 2.1 Extract AST blocks | crucible-parser | Already has ParsedDocument |
| 2.2 Block serialization | crucible-core | Pure logic, no I/O |
| 2.3 Block hashing | crucible-core | Uses ContentHasher trait |
| 2.4 Store block hashes in ParsedDocument | crucible-parser | Document structure |
| 2.5 Merkle tree construction | crucible-core | Already has MerkleTree type |
| 2.6 Store trees | crucible-surrealdb | Database persistence |

### Section 3: Content-Addressed Storage

| Task | Correct Location | Rationale |
|------|-----------------|-----------|
| 3.1 document_blocks table | crucible-surrealdb | Database schema |
| 3.2 Store blocks | crucible-surrealdb | Implements ContentAddressedStorage |
| 3.3 Block → document mapping | crucible-surrealdb | Database queries |
| 3.4 Find documents by hash | crucible-surrealdb | Database queries |
| 3.5 Deduplication tracking | crucible-core + crucible-surrealdb | Logic in core, storage in DB |

### Sections 4-7: Follow Same Pattern

**General rule**:
- **Logic** → `crucible-core` (traits + pure functions)
- **File I/O** → `crucible-watch` (scanning, watching)
- **Parsing** → `crucible-parser` (AST operations)
- **Database** → `crucible-surrealdb` (queries, storage)
- **Orchestration** → `crucible-cli` (wire it all together)

## Decision: Immediate vs. Incremental Refactoring

### Option A: Refactor Now (Recommended)

**Pros**:
- Correct architecture from the start
- Easier to add Phase 2+ features
- No technical debt accumulation

**Cons**:
- Delays Phase 1 completion
- Requires code movement and testing

**Estimated effort**: 2-3 days

### Option B: Continue Then Refactor

**Pros**:
- Phase 1 ships faster
- Can validate functionality first

**Cons**:
- Technical debt compounds
- Harder to refactor with more code
- Confusing for contributors

**Estimated effort**: 1 week later (more complex)

### Recommendation: **Option A**

Refactor now because:
1. Phase 1 is small enough to move easily
2. Sets correct pattern for Phases 2-7
3. Prevents confusion about where code belongs
4. Makes testing easier going forward

## Implementation Checklist

Phase 1 Refactoring (Do this now):
- [ ] Create `ContentHasher` trait in crucible-core
- [ ] Create `HashLookupStorage` trait in crucible-core
- [ ] Move file hashing to `crucible-core/src/hashing/file_hasher.rs`
- [ ] Move file scanning to `crucible-watch/src/file_scanner.rs`
- [ ] Move change detection to `crucible-watch/src/change_detector.rs`
- [ ] Keep hash_lookup.rs in crucible-surrealdb, implement trait
- [ ] Update crucible-cli to inject dependencies
- [ ] Move/update tests to appropriate modules
- [ ] Update documentation to reflect new structure
- [ ] Validate with `cargo test --all`

This refactoring will take ~2-3 days but will save weeks in the long run and set the correct architectural foundation for all future work.
