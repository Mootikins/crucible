# Crucible Merkle Tree Implementation Guide

**Based on**: Oxen AI Merkle Tree Research
**Target**: Filesystem-wide change detection for Crucible knowledge base
**Status**: Design proposal

---

## Overview

This guide provides a practical roadmap for implementing a **filesystem-wide Merkle tree** in Crucible, inspired by Oxen AI's architecture. The implementation will enable:

- **Fast change detection**: O(changes) instead of O(total files)
- **Incremental processing**: Only process changed documents
- **Real-time sync**: Efficient multi-device synchronization
- **Version tracking**: Historical snapshots of knowledge base state

---

## Architecture Decision: Simplified vs. Full VNode Implementation

### Option A: Simplified (Recommended for MVP)

**Target**: Knowledge bases with <10,000 documents per directory

**Structure**:
```
WorkspaceNode
└─ DirectoryNode
   └─ DocumentNode (direct children)
```

**Pros**:
- Simpler to implement
- Easier to debug
- Adequate for most use cases
- Can add VNodes later if needed

**Cons**:
- May struggle with >10k docs in single directory
- Higher memory usage for large directories

### Option B: Full VNode Implementation

**Target**: Knowledge bases with 10,000+ documents per directory

**Structure**:
```
WorkspaceNode
└─ DirectoryNode
   └─ VNode (buckets of ~500 docs)
      └─ DocumentNode
```

**Pros**:
- Scales to millions of documents
- Lower memory footprint
- Matches Oxen's proven architecture

**Cons**:
- More complex implementation
- Additional abstraction layer
- Overkill for most knowledge bases

**Recommendation**: **Start with Option A**, monitor directory sizes, migrate to Option B only if needed.

---

## Phase 1: Core Data Structures

### 1.1 Node Types

Location: `crates/crucible-core/src/merkle/node.rs`

```rust
use blake3::Hash;

/// A node in the knowledge base Merkle tree
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeNode {
    /// A markdown document
    Document(DocumentNode),
    /// A directory containing documents/subdirectories
    Directory(DirectoryNode),
    /// Root of the workspace
    Workspace(WorkspaceNode),
}

impl KnowledgeNode {
    /// Get the node's hash
    pub fn hash(&self) -> &MerkleHash {
        match self {
            Self::Document(d) => &d.combined_hash,
            Self::Directory(d) => &d.hash,
            Self::Workspace(w) => &w.hash,
        }
    }

    /// Check if this is a leaf node
    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::Document(_))
    }
}

/// 256-bit BLAKE3 hash
pub type MerkleHash = blake3::Hash;
```

### 1.2 DocumentNode

```rust
/// Represents a markdown document in the knowledge base
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentNode {
    /// File name (e.g., "ai-research.md")
    pub name: String,

    /// Relative path from workspace root
    pub path: PathBuf,

    /// Hash of document content (body only, no frontmatter)
    pub content_hash: MerkleHash,

    /// Hash of frontmatter metadata
    pub metadata_hash: MerkleHash,

    /// Combined hash (for tree integrity)
    pub combined_hash: MerkleHash,

    /// File size in bytes
    pub size_bytes: u64,

    /// Last modification time
    pub last_modified: SystemTime,

    /// Parsed frontmatter tags
    pub tags: Vec<String>,

    /// Document title (from frontmatter or first heading)
    pub title: Option<String>,
}

impl DocumentNode {
    /// Create from a markdown file path
    pub fn from_file(workspace_root: &Path, file_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(file_path)?;
        let (frontmatter, body) = parse_markdown(&content)?;

        let content_hash = blake3::hash(body.as_bytes());
        let metadata_hash = blake3::hash(frontmatter.as_bytes());
        let combined_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(content_hash.as_bytes());
            hasher.update(metadata_hash.as_bytes());
            hasher.finalize()
        };

        let metadata = fs::metadata(file_path)?;
        let path = file_path.strip_prefix(workspace_root)?.to_path_buf();

        Ok(Self {
            name: file_path.file_name().unwrap().to_string_lossy().to_string(),
            path,
            content_hash,
            metadata_hash,
            combined_hash,
            size_bytes: metadata.len(),
            last_modified: metadata.modified()?,
            tags: extract_tags(&frontmatter),
            title: extract_title(&frontmatter, &body),
        })
    }

    /// Check if content changed (ignoring metadata)
    pub fn content_changed(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
    }

    /// Check if metadata changed (ignoring content)
    pub fn metadata_changed(&self, other: &Self) -> bool {
        self.metadata_hash != other.metadata_hash
    }
}
```

### 1.3 DirectoryNode

```rust
/// Represents a directory in the knowledge base
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryNode {
    /// Directory name
    pub name: String,

    /// Relative path from workspace root
    pub path: PathBuf,

    /// Hash computed from children
    pub hash: MerkleHash,

    /// Child nodes (documents and subdirectories)
    pub children: Vec<KnowledgeNode>,

    /// Total size of all documents in this directory (recursive)
    pub total_bytes: u64,

    /// Number of documents (recursive)
    pub num_documents: u64,

    /// Last modification time (most recent child)
    pub last_modified: SystemTime,
}

impl DirectoryNode {
    /// Create from a directory path (non-recursive)
    pub fn from_path(workspace_root: &Path, dir_path: &Path) -> Result<Self> {
        let name = dir_path.file_name()
            .unwrap_or_else(|| OsStr::new(""))
            .to_string_lossy()
            .to_string();

        let path = dir_path.strip_prefix(workspace_root)?.to_path_buf();

        Ok(Self {
            name,
            path,
            hash: MerkleHash::from([0u8; 32]), // Placeholder, computed later
            children: Vec::new(),
            total_bytes: 0,
            num_documents: 0,
            last_modified: SystemTime::UNIX_EPOCH,
        })
    }

    /// Add a child node
    pub fn add_child(&mut self, child: KnowledgeNode) {
        // Update metadata
        match &child {
            KnowledgeNode::Document(doc) => {
                self.total_bytes += doc.size_bytes;
                self.num_documents += 1;
                if doc.last_modified > self.last_modified {
                    self.last_modified = doc.last_modified;
                }
            }
            KnowledgeNode::Directory(dir) => {
                self.total_bytes += dir.total_bytes;
                self.num_documents += dir.num_documents;
                if dir.last_modified > self.last_modified {
                    self.last_modified = dir.last_modified;
                }
            }
            _ => {}
        }

        self.children.push(child);
    }

    /// Compute hash from children
    pub fn compute_hash(&mut self) {
        // Sort children by name for deterministic hashing
        self.children.sort_by(|a, b| {
            let name_a = match a {
                KnowledgeNode::Document(d) => &d.name,
                KnowledgeNode::Directory(d) => &d.name,
                KnowledgeNode::Workspace(w) => &w.name,
            };
            let name_b = match b {
                KnowledgeNode::Document(d) => &d.name,
                KnowledgeNode::Directory(d) => &d.name,
                KnowledgeNode::Workspace(w) => &w.name,
            };
            name_a.cmp(name_b)
        });

        let mut hasher = blake3::Hasher::new();

        // Hash directory name
        hasher.update(self.name.as_bytes());

        // Hash all children
        for child in &self.children {
            let child_name = match child {
                KnowledgeNode::Document(d) => &d.name,
                KnowledgeNode::Directory(d) => &d.name,
                KnowledgeNode::Workspace(w) => &w.name,
            };
            let child_hash = child.hash();

            hasher.update(child_name.as_bytes());
            hasher.update(child_hash.as_bytes());
        }

        self.hash = hasher.finalize();
    }
}
```

### 1.4 WorkspaceNode

```rust
/// Root node representing the entire workspace
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceNode {
    /// Workspace name
    pub name: String,

    /// Absolute path to workspace root
    pub root_path: PathBuf,

    /// Hash computed from children
    pub hash: MerkleHash,

    /// Top-level directories
    pub children: Vec<KnowledgeNode>,

    /// Total size of workspace
    pub total_bytes: u64,

    /// Total number of documents
    pub num_documents: u64,

    /// Last build timestamp
    pub last_built: SystemTime,
}

impl WorkspaceNode {
    /// Create new workspace node
    pub fn new(name: String, root_path: PathBuf) -> Self {
        Self {
            name,
            root_path,
            hash: MerkleHash::from([0u8; 32]),
            children: Vec::new(),
            total_bytes: 0,
            num_documents: 0,
            last_built: SystemTime::now(),
        }
    }

    /// Add top-level directory
    pub fn add_child(&mut self, child: KnowledgeNode) {
        match &child {
            KnowledgeNode::Directory(dir) => {
                self.total_bytes += dir.total_bytes;
                self.num_documents += dir.num_documents;
            }
            KnowledgeNode::Document(doc) => {
                self.total_bytes += doc.size_bytes;
                self.num_documents += 1;
            }
            _ => {}
        }
        self.children.push(child);
    }

    /// Compute hash from children
    pub fn compute_hash(&mut self) {
        let mut hasher = blake3::Hasher::new();

        hasher.update(self.name.as_bytes());

        for child in &self.children {
            hasher.update(child.hash().as_bytes());
        }

        self.hash = hasher.finalize();
    }
}
```

---

## Phase 2: Tree Builder

Location: `crates/crucible-core/src/merkle/builder.rs`

```rust
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct MerkleTreeBuilder {
    workspace_root: PathBuf,
    ignore_patterns: Vec<String>,
}

impl MerkleTreeBuilder {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            workspace_root,
            ignore_patterns: vec![
                ".git".to_string(),
                ".obsidian".to_string(),
                "node_modules".to_string(),
            ],
        }
    }

    /// Build full tree from filesystem
    pub fn build(&self) -> Result<WorkspaceNode> {
        let workspace_name = self.workspace_root
            .file_name()
            .unwrap_or_else(|| OsStr::new("workspace"))
            .to_string_lossy()
            .to_string();

        let mut root = WorkspaceNode::new(workspace_name, self.workspace_root.clone());

        // Build directory tree recursively
        let root_node = self.build_directory(&self.workspace_root)?;

        // Add as children of workspace
        if let KnowledgeNode::Directory(dir) = root_node {
            for child in dir.children {
                root.add_child(child);
            }
        }

        root.compute_hash();

        Ok(root)
    }

    /// Build directory node recursively
    fn build_directory(&self, dir_path: &Path) -> Result<KnowledgeNode> {
        let mut dir_node = DirectoryNode::from_path(&self.workspace_root, dir_path)?;

        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();

            // Skip ignored patterns
            if self.should_ignore(&path) {
                continue;
            }

            if path.is_dir() {
                // Recursively build subdirectory
                let child = self.build_directory(&path)?;
                dir_node.add_child(child);
            } else if path.extension().map_or(false, |ext| ext == "md") {
                // Build document node
                let doc = DocumentNode::from_file(&self.workspace_root, &path)?;
                dir_node.add_child(KnowledgeNode::Document(doc));
            }
        }

        dir_node.compute_hash();

        Ok(KnowledgeNode::Directory(dir_node))
    }

    fn should_ignore(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        self.ignore_patterns.iter().any(|pattern| path_str.contains(pattern))
    }
}
```

---

## Phase 3: Change Detector

Location: `crates/crucible-core/src/merkle/change_detector.rs`

```rust
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Document was added
    Added { path: PathBuf, hash: MerkleHash },

    /// Document content was modified
    Modified {
        path: PathBuf,
        old_hash: MerkleHash,
        new_hash: MerkleHash,
        content_changed: bool,
        metadata_changed: bool,
    },

    /// Document was deleted
    Deleted { path: PathBuf, hash: MerkleHash },
}

pub struct ChangeDetector;

impl ChangeDetector {
    /// Detect changes between two workspace trees
    pub fn detect(
        previous: &WorkspaceNode,
        current: &WorkspaceNode,
    ) -> Vec<Change> {
        if previous.hash == current.hash {
            return Vec::new(); // No changes
        }

        let mut changes = Vec::new();

        // Compare children recursively
        Self::compare_nodes(&previous.children, &current.children, &mut changes);

        changes
    }

    fn compare_nodes(
        previous: &[KnowledgeNode],
        current: &[KnowledgeNode],
        changes: &mut Vec<Change>,
    ) {
        // Build lookup maps
        let prev_map: HashMap<PathBuf, &KnowledgeNode> = previous
            .iter()
            .map(|node| (Self::node_path(node), node))
            .collect();

        let curr_map: HashMap<PathBuf, &KnowledgeNode> = current
            .iter()
            .map(|node| (Self::node_path(node), node))
            .collect();

        // Find added and modified
        for (path, curr_node) in &curr_map {
            match prev_map.get(path) {
                None => {
                    // Node added
                    changes.push(Change::Added {
                        path: path.clone(),
                        hash: *curr_node.hash(),
                    });
                }
                Some(prev_node) => {
                    // Node exists, check if changed
                    if prev_node.hash() != curr_node.hash() {
                        match (prev_node, curr_node) {
                            (
                                KnowledgeNode::Document(prev_doc),
                                KnowledgeNode::Document(curr_doc),
                            ) => {
                                // Document modified
                                changes.push(Change::Modified {
                                    path: path.clone(),
                                    old_hash: prev_doc.combined_hash,
                                    new_hash: curr_doc.combined_hash,
                                    content_changed: prev_doc.content_changed(curr_doc),
                                    metadata_changed: prev_doc.metadata_changed(curr_doc),
                                });
                            }
                            (
                                KnowledgeNode::Directory(prev_dir),
                                KnowledgeNode::Directory(curr_dir),
                            ) => {
                                // Directory changed, recurse
                                Self::compare_nodes(
                                    &prev_dir.children,
                                    &curr_dir.children,
                                    changes,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Find deleted
        for (path, prev_node) in &prev_map {
            if !curr_map.contains_key(path) {
                changes.push(Change::Deleted {
                    path: path.clone(),
                    hash: *prev_node.hash(),
                });
            }
        }
    }

    fn node_path(node: &KnowledgeNode) -> PathBuf {
        match node {
            KnowledgeNode::Document(d) => d.path.clone(),
            KnowledgeNode::Directory(d) => d.path.clone(),
            KnowledgeNode::Workspace(w) => PathBuf::from(""),
        }
    }
}
```

---

## Phase 4: Incremental Builder (Optimization)

Location: `crates/crucible-core/src/merkle/incremental.rs`

```rust
use std::time::SystemTime;

pub struct IncrementalTreeBuilder {
    builder: MerkleTreeBuilder,
    last_tree: Option<WorkspaceNode>,
    last_scan: SystemTime,
}

impl IncrementalTreeBuilder {
    pub fn new(workspace_root: PathBuf) -> Self {
        Self {
            builder: MerkleTreeBuilder::new(workspace_root),
            last_tree: None,
            last_scan: SystemTime::UNIX_EPOCH,
        }
    }

    /// Build tree incrementally (only rehash changed files)
    pub fn build_incremental(&mut self) -> Result<WorkspaceNode> {
        let now = SystemTime::now();

        // Full build on first run
        if self.last_tree.is_none() {
            let tree = self.builder.build()?;
            self.last_tree = Some(tree.clone());
            self.last_scan = now;
            return Ok(tree);
        }

        // Find files modified since last scan
        let modified_files = self.find_modified_files()?;

        if modified_files.is_empty() {
            // No changes, return cached tree
            return Ok(self.last_tree.clone().unwrap());
        }

        // Rebuild only affected subtrees
        let tree = self.rebuild_affected_paths(&modified_files)?;

        self.last_tree = Some(tree.clone());
        self.last_scan = now;

        Ok(tree)
    }

    fn find_modified_files(&self) -> Result<Vec<PathBuf>> {
        let mut modified = Vec::new();

        for entry in WalkDir::new(&self.builder.workspace_root) {
            let entry = entry?;

            if !entry.path().extension().map_or(false, |ext| ext == "md") {
                continue;
            }

            let metadata = entry.metadata()?;
            let mtime = metadata.modified()?;

            if mtime > self.last_scan {
                modified.push(entry.path().to_path_buf());
            }
        }

        Ok(modified)
    }

    fn rebuild_affected_paths(&self, modified_files: &[PathBuf]) -> Result<WorkspaceNode> {
        // For simplicity, rebuild entire tree but only hash changed files
        // A more sophisticated implementation would rebuild only affected subtrees

        // This is a placeholder - full implementation would:
        // 1. Clone last tree
        // 2. For each modified file:
        //    a. Rehash the file
        //    b. Update the DocumentNode
        //    c. Recompute hashes up to root
        // 3. Return updated tree

        self.builder.build()
    }
}
```

---

## Phase 5: Integration with Existing Crucible Systems

### 5.1 Integration with File Scanner

Update: `crates/crucible-cli/src/common/file_scanner.rs`

```rust
use crucible_core::merkle::{MerkleTreeBuilder, ChangeDetector};

pub struct FileScanner {
    // ... existing fields ...
    tree_builder: MerkleTreeBuilder,
    last_workspace_tree: Option<WorkspaceNode>,
}

impl FileScanner {
    pub fn scan_for_changes(&mut self) -> Result<Vec<Change>> {
        // Build current tree
        let current_tree = self.tree_builder.build()?;

        // Detect changes
        let changes = if let Some(ref last_tree) = self.last_workspace_tree {
            ChangeDetector::detect(last_tree, &current_tree)
        } else {
            // First scan, all files are "added"
            self.extract_all_files(&current_tree)
        };

        // Update cache
        self.last_workspace_tree = Some(current_tree);

        Ok(changes)
    }
}
```

### 5.2 Integration with EPR Schema

Update: `crates/crucible-surrealdb/src/epr/schema.rs`

```rust
/// Add merkle hash to document entities
#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentEntity {
    pub id: String,
    pub content: String,
    pub metadata: serde_json::Value,

    // NEW: Merkle tree integration
    pub merkle_hash: Option<String>,  // Hex-encoded MerkleHash
    pub content_hash: Option<String>,
    pub metadata_hash: Option<String>,
    pub last_tree_update: Option<DateTime<Utc>>,
}

/// Store workspace tree snapshots
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceTreeSnapshot {
    pub id: String,
    pub workspace_id: String,
    pub root_hash: String,  // Hex-encoded MerkleHash
    pub tree_data: serde_json::Value,  // Serialized WorkspaceNode
    pub created_at: DateTime<Utc>,
    pub num_documents: u64,
    pub total_bytes: u64,
}
```

### 5.3 Integration with Kiln Processor

Update: `crates/crucible-cli/src/common/kiln_processor.rs`

```rust
impl KilnProcessor {
    pub async fn process_changes(&mut self, changes: Vec<Change>) -> Result<()> {
        for change in changes {
            match change {
                Change::Added { path, hash } => {
                    info!("Document added: {:?} (hash: {})", path, hash);
                    self.process_new_document(&path).await?;
                }

                Change::Modified {
                    path,
                    old_hash,
                    new_hash,
                    content_changed,
                    metadata_changed,
                } => {
                    info!("Document modified: {:?}", path);

                    if content_changed {
                        // Recompute embeddings
                        info!("  → Content changed, recomputing embeddings");
                        self.recompute_embeddings(&path).await?;
                    }

                    if metadata_changed {
                        // Update metadata index only
                        info!("  → Metadata changed, updating index");
                        self.update_metadata_index(&path).await?;
                    }
                }

                Change::Deleted { path, hash } => {
                    info!("Document deleted: {:?}", path);
                    self.remove_document(&path).await?;
                }
            }
        }

        Ok(())
    }
}
```

---

## Phase 6: Testing Strategy

### 6.1 Unit Tests

Location: `crates/crucible-core/src/merkle/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_node_hash() {
        // Test that same content produces same hash
        let doc1 = create_test_document("test.md", "# Hello\n\nWorld");
        let doc2 = create_test_document("test.md", "# Hello\n\nWorld");

        assert_eq!(doc1.content_hash, doc2.content_hash);
        assert_eq!(doc1.combined_hash, doc2.combined_hash);
    }

    #[test]
    fn test_change_detection_added() {
        let mut prev_tree = create_empty_workspace();
        let mut curr_tree = create_empty_workspace();

        // Add document to current tree
        curr_tree.add_child(create_test_document_node("new.md"));
        curr_tree.compute_hash();

        let changes = ChangeDetector::detect(&prev_tree, &curr_tree);

        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], Change::Added { .. }));
    }

    #[test]
    fn test_unchanged_subtree_skipped() {
        // Create tree with two directories
        let mut prev_tree = create_workspace_with_dirs(vec!["dir1", "dir2"]);
        let mut curr_tree = prev_tree.clone();

        // Modify only dir1
        modify_directory(&mut curr_tree, "dir1", "new-doc.md");

        let changes = ChangeDetector::detect(&prev_tree, &curr_tree);

        // Should only detect changes in dir1, not dir2
        assert!(changes.iter().all(|c| {
            let path = match c {
                Change::Added { path, .. } => path,
                Change::Modified { path, .. } => path,
                Change::Deleted { path, .. } => path,
            };
            path.starts_with("dir1")
        }));
    }
}
```

### 6.2 Integration Tests

Location: `tests/merkle_integration_tests.rs`

```rust
#[tokio::test]
async fn test_end_to_end_change_detection() {
    let temp_dir = create_temp_workspace();

    // Initial build
    let builder = MerkleTreeBuilder::new(temp_dir.path().to_path_buf());
    let tree1 = builder.build().unwrap();

    // Modify a file
    fs::write(temp_dir.path().join("test.md"), "# Modified content").unwrap();

    // Rebuild
    let tree2 = builder.build().unwrap();

    // Detect changes
    let changes = ChangeDetector::detect(&tree1, &tree2);

    assert_eq!(changes.len(), 1);
    assert!(matches!(changes[0], Change::Modified { .. }));
}

#[tokio::test]
async fn test_incremental_builder_performance() {
    let temp_dir = create_large_workspace(1000); // 1000 documents

    let mut builder = IncrementalTreeBuilder::new(temp_dir.path().to_path_buf());

    // First build (full)
    let start = Instant::now();
    let tree1 = builder.build_incremental().unwrap();
    let full_build_time = start.elapsed();

    // Modify one file
    modify_random_file(&temp_dir);

    // Incremental build
    let start = Instant::now();
    let tree2 = builder.build_incremental().unwrap();
    let incremental_build_time = start.elapsed();

    // Incremental should be much faster
    assert!(incremental_build_time < full_build_time / 10);
}
```

---

## Phase 7: Performance Monitoring

### 7.1 Metrics to Track

```rust
#[derive(Debug, Clone)]
pub struct MerkleTreeMetrics {
    /// Time to build full tree
    pub full_build_duration: Duration,

    /// Time to detect changes
    pub change_detection_duration: Duration,

    /// Number of nodes in tree
    pub total_nodes: usize,

    /// Number of files hashed
    pub files_hashed: usize,

    /// Number of nodes reused (not rehashed)
    pub nodes_reused: usize,

    /// Memory usage
    pub memory_bytes: usize,
}

impl MerkleTreeMetrics {
    pub fn log(&self) {
        info!("Merkle Tree Metrics:");
        info!("  Full build: {:?}", self.full_build_duration);
        info!("  Change detection: {:?}", self.change_detection_duration);
        info!("  Total nodes: {}", self.total_nodes);
        info!("  Files hashed: {}", self.files_hashed);
        info!("  Nodes reused: {}", self.nodes_reused);
        info!("  Memory usage: {} KB", self.memory_bytes / 1024);
    }
}
```

### 7.2 Benchmarks

Location: `benches/merkle_benchmarks.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_tree_building(c: &mut Criterion) {
    c.bench_function("build tree (100 docs)", |b| {
        let workspace = create_test_workspace(100);
        let builder = MerkleTreeBuilder::new(workspace.path().to_path_buf());

        b.iter(|| {
            builder.build().unwrap()
        });
    });

    c.bench_function("build tree (1000 docs)", |b| {
        let workspace = create_test_workspace(1000);
        let builder = MerkleTreeBuilder::new(workspace.path().to_path_buf());

        b.iter(|| {
            builder.build().unwrap()
        });
    });
}

fn benchmark_change_detection(c: &mut Criterion) {
    c.bench_function("detect changes (1% modified)", |b| {
        let (tree1, tree2) = create_test_trees_with_changes(1000, 10);

        b.iter(|| {
            ChangeDetector::detect(&tree1, &tree2)
        });
    });
}

criterion_group!(benches, benchmark_tree_building, benchmark_change_detection);
criterion_main!(benches);
```

---

## Expected Performance Characteristics

### Small Knowledge Base (<1,000 docs)

| Operation | Time | Notes |
|-----------|------|-------|
| Full build | 50-100ms | Hash all files |
| Incremental build | 5-10ms | Only changed files |
| Change detection | 1-2ms | O(changes) |
| Memory usage | 1-5 MB | Entire tree in memory |

### Medium Knowledge Base (1,000-10,000 docs)

| Operation | Time | Notes |
|-----------|------|-------|
| Full build | 500ms-1s | Hash all files |
| Incremental build | 10-50ms | Only changed files |
| Change detection | 5-10ms | O(changes) |
| Memory usage | 10-50 MB | Entire tree in memory |

### Large Knowledge Base (10,000+ docs)

| Operation | Time | Notes |
|-----------|------|-------|
| Full build | 2-5s | Hash all files |
| Incremental build | 50-100ms | Only changed files |
| Change detection | 10-20ms | O(changes) |
| Memory usage | 50-200 MB | Consider VNodes |

---

## Migration Path: Adding VNodes Later

If you start with the simplified approach and later need VNodes:

### Step 1: Add VNode Type

```rust
pub enum KnowledgeNode {
    Document(DocumentNode),
    Directory(DirectoryNode),
    VNode(VNodeContainer),  // NEW
    Workspace(WorkspaceNode),
}

pub struct VNodeContainer {
    pub bucket_id: u32,
    pub documents: Vec<DocumentNode>,
    pub hash: MerkleHash,
}
```

### Step 2: Update Directory Builder

```rust
impl DirectoryNode {
    fn add_child_with_vnodes(&mut self, child: KnowledgeNode, vnode_size: usize) {
        // If directory has >vnode_size documents, split into VNodes
        if self.num_documents > vnode_size as u64 {
            self.reorganize_with_vnodes(vnode_size);
        }

        // Add to appropriate VNode bucket
        // ... (implementation details)
    }
}
```

### Step 3: Migrate Existing Trees

```rust
pub fn migrate_to_vnodes(tree: WorkspaceNode, vnode_size: usize) -> WorkspaceNode {
    // Walk tree, reorganize large directories
    // ... (implementation details)
}
```

---

## Conclusion

This implementation guide provides a clear path from simple Merkle tree (MVP) to full-featured version control system with VNodes. Key principles:

1. **Start simple**: Don't add complexity until you need it
2. **Measure first**: Monitor directory sizes and performance
3. **Incremental optimization**: Add VNodes only when necessary
4. **Backward compatibility**: Design for future migration

Following this guide, Crucible will have efficient, scalable change detection suitable for knowledge bases of any size.

---

**Implementation guide by**: Claude (Anthropic)
**Date**: 2025-11-08
**Status**: Design proposal for review
**Next steps**: Review, validate approach, implement Phase 1
