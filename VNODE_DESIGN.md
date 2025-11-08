# VNode Design for Crucible
## Based on Oxen.ai Implementation

**Source:** https://www.oxen.ai/blog/merkle-tree-vnodes

---

## What is a VNode?

A **VNode (Virtual Node)** is an intermediate sharding layer in the merkle tree that distributes large collections of children across multiple nodes.

**Problem it solves:**
- Without VNodes: Adding 1 file to directory with 1.1M files → copy all 1.1M references
- With VNodes: Adding 1 file → only update the affected VNode shard (~10K references)

---

## VNode Configuration

### Oxen's Defaults
- **VNode size**: 10,000 children per shard
- **Configurable** via `.oxen/config.toml`

### Crucible's Approach
Given our use case (knowledge vaults, not massive datasets):
- **Default VNode size**: 100 children per shard
- **Rationale**:
  - Most folders have <100 files
  - Large folders (e.g., daily notes) might have 1000s of files
  - 100 is a good balance between overhead and granularity

---

## Structure

### Without VNodes (Small Directory)
```
Directory: "Projects/" (50 files)
├─ file1.md
├─ file2.md
└─ ... (48 more)
```

### With VNodes (Large Directory)
```
Directory: "Daily Notes/" (1,500 files)
├─ VNode 0 (hash: abc123...) [100 files]
│  ├─ 2023-01-01.md
│  ├─ 2023-01-05.md
│  └─ ... (98 more)
├─ VNode 1 (hash: def456...) [100 files]
│  ├─ 2023-02-03.md
│  └─ ... (99 more)
├─ VNode 2 (hash: ghi789...) [100 files]
└─ ... (12 more VNodes)

Total: 15 VNodes (ceil(1500 / 100))
```

---

## Hash-Based Distribution

**Key insight:** Children are distributed to VNodes using **hash-based bucketing**, NOT sequential chunking.

### Algorithm

```rust
// Number of VNodes needed
let num_vnodes = (total_children + VNODE_SIZE - 1) / VNODE_SIZE;

// Assign child to VNode
let path_hash = hash(child_path);
let vnode_idx = path_hash % num_vnodes;
```

### Example with vnode_size=4, 10 files

```
Files and their hash-based assignments:
- file_a.md → hash % 3 = 0 → VNode 0
- file_b.md → hash % 3 = 2 → VNode 2
- file_c.md → hash % 3 = 1 → VNode 1
- file_d.md → hash % 3 = 0 → VNode 0
- file_e.md → hash % 3 = 1 → VNode 1
- file_f.md → hash % 3 = 2 → VNode 2
- file_g.md → hash % 3 = 0 → VNode 0
- file_h.md → hash % 3 = 1 → VNode 1
- file_i.md → hash % 3 = 2 → VNode 2
- file_j.md → hash % 3 = 0 → VNode 0

Result:
VNode 0: [file_a.md, file_d.md, file_g.md, file_j.md]
VNode 1: [file_c.md, file_e.md, file_h.md]
VNode 2: [file_b.md, file_f.md, file_i.md]
```

**Note:** Distribution is approximately even but NOT guaranteed equal.

---

## Storage Schema

### Directory Node (with VNodes)

```rust
pub struct DirectoryNode {
    pub path: PathBuf,
    pub hash: BlockHash,  // Combined hash of all VNode hashes
    pub children: DirectoryChildren,
}

pub enum DirectoryChildren {
    /// Small directory - all children loaded
    Direct(Vec<MerkleNode>),

    /// Large directory - sharded into VNodes
    Sharded {
        total_count: usize,
        vnode_size: usize,
        vnodes: Vec<VNodeShard>,
    },
}

pub struct VNodeShard {
    /// Index of this VNode (0 to num_vnodes-1)
    index: usize,

    /// Hash of this shard (used as storage key)
    hash: BlockHash,

    /// Number of children in this shard
    count: usize,

    /// Lazily loaded children (sorted by path for binary search)
    children: Option<Vec<MerkleNode>>,
}
```

### SurrealDB Storage

```sql
DEFINE TABLE merkle_vnodes SCHEMAFULL;

-- VNode hash (storage key)
DEFINE FIELD hash ON TABLE merkle_vnodes
    TYPE string
    ASSERT string::len($value) == 64;

-- Parent directory hash
DEFINE FIELD parent_hash ON TABLE merkle_vnodes
    TYPE string;

-- VNode index within parent
DEFINE FIELD vnode_index ON TABLE merkle_vnodes
    TYPE int;

-- Serialized children (sorted Vec<MerkleNode>)
DEFINE FIELD children ON TABLE merkle_vnodes
    TYPE bytes;

-- Metadata
DEFINE FIELD child_count ON TABLE merkle_vnodes
    TYPE int;

DEFINE FIELD created_at ON TABLE merkle_vnodes
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX unique_hash ON TABLE merkle_vnodes COLUMNS hash UNIQUE;
DEFINE INDEX parent_vnode_idx ON TABLE merkle_vnodes COLUMNS parent_hash, vnode_index;
```

---

## Operations

### 1. Building a Directory with VNodes

```rust
async fn build_directory_node(
    path: PathBuf,
    hasher: &impl ContentHasher,
    storage: &impl MerkleStorage,
) -> Result<DirectoryNode> {
    let entries: Vec<PathBuf> = read_dir_sorted(&path)?;
    let child_count = entries.len();

    if child_count <= VNODE_SIZE {
        // Small directory - no VNodes needed
        let children = build_children(entries, hasher, storage).await?;
        let hash = compute_combined_hash(&children, hasher);

        return Ok(DirectoryNode {
            path,
            hash,
            children: DirectoryChildren::Direct(children),
        });
    }

    // Large directory - use VNode sharding
    let num_vnodes = (child_count + VNODE_SIZE - 1) / VNODE_SIZE;

    // Distribute children to VNodes using hash-based bucketing
    let mut vnode_buckets: Vec<Vec<PathBuf>> = vec![Vec::new(); num_vnodes];

    for entry_path in entries {
        let path_hash = hasher.hash_path(&entry_path);
        let vnode_idx = path_hash.as_u64() % (num_vnodes as u64);
        vnode_buckets[vnode_idx as usize].push(entry_path);
    }

    // Build VNode shards
    let mut vnodes = Vec::new();
    for (idx, bucket) in vnode_buckets.into_iter().enumerate() {
        let vnode = build_vnode_shard(idx, bucket, hasher, storage).await?;
        vnodes.push(vnode);
    }

    // Compute combined hash from all VNode hashes
    let vnode_hashes: Vec<&BlockHash> = vnodes.iter().map(|v| &v.hash).collect();
    let combined_hash = hasher.hash_combined(&vnode_hashes);

    Ok(DirectoryNode {
        path,
        hash: combined_hash,
        children: DirectoryChildren::Sharded {
            total_count: child_count,
            vnode_size: VNODE_SIZE,
            vnodes,
        },
    })
}

async fn build_vnode_shard(
    index: usize,
    paths: Vec<PathBuf>,
    hasher: &impl ContentHasher,
    storage: &impl MerkleStorage,
) -> Result<VNodeShard> {
    // Build children nodes
    let mut children = Vec::new();
    for path in paths {
        let child = build_node(path, hasher, storage).await?;
        children.push(child);
    }

    // Sort children by path for binary search
    children.sort_by(|a, b| a.path().cmp(b.path()));

    // Compute hash of this shard
    let child_hashes: Vec<&BlockHash> = children.iter().map(|c| c.hash()).collect();
    let shard_hash = hasher.hash_combined(&child_hashes);

    // Store children in database
    storage.store_vnode(&shard_hash, &children).await?;

    Ok(VNodeShard {
        index,
        hash: shard_hash,
        count: children.len(),
        children: None,  // Not loaded yet
    })
}
```

### 2. Looking Up a File

```rust
async fn lookup_file(
    directory: &mut DirectoryNode,
    filename: &str,
    storage: &impl MerkleStorage,
) -> Result<Option<&MerkleNode>> {
    match &mut directory.children {
        DirectoryChildren::Direct(children) => {
            // Simple linear/binary search
            Ok(children.iter().find(|c| c.filename() == filename))
        }

        DirectoryChildren::Sharded { vnodes, vnode_size, .. } => {
            // Determine which VNode contains this file
            let path_hash = hash_path(filename);
            let vnode_idx = (path_hash.as_u64() % (vnodes.len() as u64)) as usize;

            // Load that VNode if needed
            let vnode = &mut vnodes[vnode_idx];
            if vnode.children.is_none() {
                let children = storage.load_vnode(&vnode.hash).await?;
                vnode.children = Some(children);
            }

            // Binary search within the VNode (children are sorted)
            if let Some(children) = &vnode.children {
                Ok(children.binary_search_by_key(&filename, |c| c.filename()).ok()
                    .map(|idx| &children[idx]))
            } else {
                Ok(None)
            }
        }
    }
}
```

### 3. Diffing Directories

```rust
fn diff_directories(
    old: &DirectoryNode,
    new: &DirectoryNode,
) -> DirectoryDiff {
    match (&old.children, &new.children) {
        // Both small - direct comparison
        (DirectoryChildren::Direct(old_children), DirectoryChildren::Direct(new_children)) => {
            diff_children(old_children, new_children)
        }

        // Both sharded - compare VNode by VNode
        (
            DirectoryChildren::Sharded { vnodes: old_vnodes, .. },
            DirectoryChildren::Sharded { vnodes: new_vnodes, .. }
        ) => {
            let mut changes = Vec::new();

            for (old_vnode, new_vnode) in old_vnodes.iter().zip(new_vnodes.iter()) {
                if old_vnode.hash != new_vnode.hash {
                    // This VNode changed - need to load and diff children
                    // Only loads affected VNodes!
                    changes.push(VNodeChange {
                        index: old_vnode.index,
                        changed: true,
                    });
                } else {
                    // VNode hash matches - entire shard unchanged
                    // Early exit! Don't need to load or scan
                    changes.push(VNodeChange {
                        index: old_vnode.index,
                        changed: false,
                    });
                }
            }

            DirectoryDiff { vnode_changes: changes }
        }

        // Mixed - one sharded, one not (structural change)
        _ => {
            DirectoryDiff {
                structural_change: true,
                // Need full comparison
            }
        }
    }
}
```

---

## Performance Benefits

### 1. Write Operations
**Without VNodes:**
```
Add file to directory with 10,000 files
→ Copy 10,000 references
→ Write entire new directory node
```

**With VNodes (size=100):**
```
Add file to directory with 10,000 files
→ Determine target VNode via hash (O(1))
→ Copy 100 references in that VNode
→ Write 1 VNode + update parent hash
→ 99x less data moved!
```

### 2. Read Operations (Lookup)
**Without VNodes:**
```
Find file in directory with 10,000 files
→ Load 10,000 references
→ Binary search: O(log 10000) ≈ 13 comparisons
```

**With VNodes (size=100):**
```
Find file in directory with 10,000 files
→ Hash to determine VNode: O(1)
→ Load 100 references from that VNode
→ Binary search: O(log 100) ≈ 7 comparisons
→ 100x less data loaded!
```

### 3. Diff Operations
**Without VNodes:**
```
Diff two versions of directory (1 file changed out of 10,000)
→ Scan all 10,000 files to find the change
```

**With VNodes (size=100):**
```
Diff two versions of directory (1 file changed out of 10,000)
→ Compare 100 VNode hashes
→ Early exit on 99 VNodes (hash matches)
→ Only scan the 1 changed VNode (100 files)
→ 100x less data scanned!
```

---

## Implementation Notes

### Path Hashing for Distribution

```rust
impl ContentHasher for Blake3Hasher {
    fn hash_path(&self, path: &Path) -> BlockHash {
        // Use path string for distribution
        let path_str = path.to_string_lossy();
        self.hash_content(path_str.as_bytes())
    }
}

// Extract u64 from hash for modulo operation
impl BlockHash {
    pub fn as_u64(&self) -> u64 {
        // Use first 8 bytes of hash
        u64::from_le_bytes(self.0[0..8].try_into().unwrap())
    }
}
```

### Sorted Children for Binary Search

Within each VNode, children MUST be sorted by path to enable fast lookups:

```rust
// After building children
children.sort_by(|a, b| a.path().cmp(b.path()));

// Lookup uses binary search
let idx = children.binary_search_by_key(&target_path, |c| c.path())?;
```

### Directory Lookup Table (Optional Optimization)

Oxen maintains a reverse lookup table for O(1) directory access:

```sql
DEFINE TABLE directory_lookup SCHEMAFULL;

DEFINE FIELD path ON TABLE directory_lookup TYPE string;
DEFINE FIELD hash ON TABLE directory_lookup TYPE string;

DEFINE INDEX unique_path ON TABLE directory_lookup COLUMNS path UNIQUE;
```

This enables:
```rust
// O(1) lookup instead of tree traversal
let dir_hash = db.query("SELECT hash FROM directory_lookup WHERE path = $path").await?;
```

---

## Configuration for Crucible

### Recommended Settings

```toml
# .crucible/config.toml

[merkle_tree]
# VNode threshold (directory/file children count)
vnode_size = 100

# Cache size for VNode children (LRU)
vnode_cache_size = 1000

# Enable directory lookup table
enable_directory_lookup = true
```

### Tuning Guidelines

- **Small vaults (<1000 notes)**: `vnode_size = 100` (rarely triggered)
- **Medium vaults (1000-10000 notes)**: `vnode_size = 100` (good balance)
- **Large vaults (>10000 notes)**: `vnode_size = 500` (reduce VNode overhead)
- **Daily notes (time-series)**: `vnode_size = 100` (good distribution)

---

## Summary

**VNodes = Hash-Based Sharding for Large Containers**

Key characteristics:
1. ✅ **Threshold-based**: Only used when children > vnode_size
2. ✅ **Hash-based distribution**: `vnode_idx = hash(path) % num_vnodes`
3. ✅ **Lazy loading**: Only load affected VNodes
4. ✅ **Early exit**: Hash comparison avoids scanning unchanged VNodes
5. ✅ **Sorted children**: Binary search within each VNode
6. ✅ **Approximately even**: Distribution is not guaranteed equal
7. ✅ **Performance**: 100x improvement for large directories

**Application in Crucible:**
- Directories with >100 files → VNode sharding
- Files with >100 blocks → VNode sharding (same pattern!)
- Storage uses hash as key (raw bytes)
- Universal pattern for any container type
