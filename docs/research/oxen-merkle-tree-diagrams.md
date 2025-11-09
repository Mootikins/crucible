# Oxen Merkle Tree - Visual Diagrams

This document provides visual representations of Oxen's Merkle tree architecture to complement the main analysis.

---

## 1. Overall Tree Structure

```
┌─────────────────────────────────────────────────────────────┐
│                       CommitNode                            │
│  hash: 0xabc123                                             │
│  parent: 0x789def                                           │
│  message: "Add cat images"                                  │
│  timestamp: 2025-11-08 10:30:00                             │
└───────────────────────┬─────────────────────────────────────┘
                        │
                        │ (root_dir)
                        ▼
        ┌───────────────────────────────────┐
        │         DirNode: data/            │
        │  hash: 0xdef456                   │
        │  num_files: 12,543                │
        │  num_bytes: 5.2 GB                │
        └────────┬────────────┬─────────────┘
                 │            │
        ┌────────▼────┐  ┌───▼──────────┐
        │ DirNode:    │  │ DirNode:     │
        │ images/     │  │ documents/   │
        │ hash: 0x111 │  │ hash: 0x222  │
        └──────┬──────┘  └──────┬───────┘
               │                │
               │                │
        ┌──────▼──────┐  ┌──────▼──────┐
        │ DirNode:    │  │ DirNode:    │
        │ cats/       │  │ reports/    │
        │ hash: 0x333 │  │ hash: 0x444 │
        │ 10,234 imgs │  │ 1,245 docs  │
        └──────┬──────┘  └──────┬──────┘
               │                │
        ┌──────▼──────────┬──────▼──────────┐
        │ VNode 0         │ VNode 1         │
        │ hash: 0x555     │ hash: 0x666     │
        │ 5,117 files     │ 5,117 files     │
        └──────┬──────────┴──────┬──────────┘
               │                 │
        ┌──────▼──────┐   ┌──────▼──────┐
        │ FileNode    │   │ FileNode    │
        │ 001.jpg     │   │ 002.jpg     │
        │ hash: 0x777 │   │ hash: 0x888 │
        │ 2.3 MB      │   │ 1.8 MB      │
        └─────────────┘   └─────────────┘
```

---

## 2. VNode Distribution Strategy

### Without VNodes (Problematic for Large Directories)

```
DirNode: images/
├─ FileNode: 000001.jpg
├─ FileNode: 000002.jpg
├─ FileNode: 000003.jpg
├─ ...
└─ FileNode: 100000.jpg  ← 100,000 direct children!

Memory: ~20 MB for DirNode
Traversal: O(100,000) to load directory
```

### With VNodes (Oxen's Approach)

```
DirNode: images/
├─ VNode 0 (bucket 0)
│  ├─ FileNode: 000007.jpg  ← hash("000007.jpg") % 10 = 0
│  ├─ FileNode: 000013.jpg  ← hash("000013.jpg") % 10 = 0
│  └─ ... (~10,000 files)
├─ VNode 1 (bucket 1)
│  ├─ FileNode: 000001.jpg  ← hash("000001.jpg") % 10 = 1
│  └─ ... (~10,000 files)
├─ VNode 2 (bucket 2)
│  └─ ... (~10,000 files)
├─ ...
└─ VNode 9 (bucket 9)
   └─ ... (~10,000 files)

Memory: ~200 KB for DirNode (10 VNode refs)
Traversal: O(10) to load directory structure
Lazy load: Only load VNode 0 if accessing files in bucket 0
```

**Distribution Algorithm**:
```
total_files = 100,000
vnode_size = 10,000
num_vnodes = ceil(100,000 / 10,000) = 10

for each file in directory:
    bucket = hash(file.path) % 10
    vnodes[bucket].add_file(file)
```

---

## 3. Change Detection Flow

### Scenario: User modifies `/data/images/cats/001.jpg`

```
Step 1: Detect file change
─────────────────────────
Working dir: /data/images/cats/001.jpg modified
Action: Hash file content
Result: New hash 0xNEW (was 0x777)

Step 2: Update FileNode
─────────────────────────
FileNode: 001.jpg
  Old hash: 0x777
  New hash: 0xNEW
Action: Create new FileNode

Step 3: Update VNode
─────────────────────────
VNode 0:
  Old hash: 0x555
  New hash: 0xNEW_VNODE  ← Rehashed (includes new file hash)
Action: Generate new UUID, recompute VNode hash

Step 4: Update DirNode (cats/)
─────────────────────────
DirNode: cats/
  Old hash: 0x333
  New hash: 0xNEW_CATS  ← Rehashed (VNode 0 changed)
Action: Recompute directory hash from children

Step 5: Update DirNode (images/)
─────────────────────────
DirNode: images/
  Old hash: 0x111
  New hash: 0xNEW_IMAGES  ← Rehashed (cats/ changed)
Action: Propagate change upward

Step 6: Update DirNode (data/)
─────────────────────────
DirNode: data/
  Old hash: 0xdef456
  New hash: 0xNEW_DATA  ← Rehashed (images/ changed)
Action: Propagate change upward

Step 7: Create new CommitNode
─────────────────────────
CommitNode:
  Old hash: 0xabc123
  New hash: 0xNEW_COMMIT  ← New commit
  root_dir: 0xNEW_DATA
  parent: 0xabc123
  message: "Update cat image 001"
```

### Nodes NOT Rehashed

```
✗ FileNode: 002.jpg ... 100000.jpg (unchanged files)
✗ VNode 1 ... VNode 9 (other buckets)
✗ DirNode: dogs/ (sibling directory)
✗ DirNode: documents/ (unrelated directory)
```

**Efficiency**: Only **7 nodes** rehashed out of potentially 100,000+ nodes!

---

## 4. Hash Computation Details

### FileNode Hash

```rust
┌─────────────────────────────────────────┐
│         File Content                    │
│  ┌──────────────────────────────────┐   │
│  │ Binary data: <image bytes>       │   │
│  └──────────────────────────────────┘   │
│               │                          │
│               ▼                          │
│         xxHash3()                        │
│               │                          │
│               ▼                          │
│     content_hash: 0x777                  │
└─────────────────────────────────────────┘

FileNode {
    name: "001.jpg",
    hash: 0x777,  ← Used in tree
    num_bytes: 2,400,000,
    last_modified: 2025-11-08 10:15:32,
    data_type: "image",
    mime_type: "image/jpeg",
    ...
}
```

### VNode Hash

```rust
┌────────────────────────────────────────────────┐
│         VNode Hash Inputs                      │
├────────────────────────────────────────────────┤
│  1. "vnode" (prefix)                           │
│  2. Directory path: "/data/images/cats"        │
│  3. File hashes (sorted by name):              │
│     - "001.jpg" → 0x777                        │
│     - "002.jpg" → 0x888                        │
│     - "003.jpg" → 0x999                        │
│     - ... (all files in bucket)                │
│  4. UUID (if modified): "550e8400-e29b-..."    │
└────────────────┬───────────────────────────────┘
                 │
                 ▼
           xxHash3([prefix, path, hashes, uuid])
                 │
                 ▼
         VNode hash: 0x555
```

**Key Property**: UUID forces new hash even if file set unchanged (for modified VNodes).

### DirNode Hash

```rust
┌────────────────────────────────────────────────┐
│         DirNode Hash Inputs                    │
├────────────────────────────────────────────────┤
│  1. Directory name: "cats"                     │
│  2. Child names and hashes (sorted):           │
│     - "vnode_0" → 0x555                        │
│     - "vnode_1" → 0x666                        │
│     - "subfolder" → 0xaaa                      │
└────────────────┬───────────────────────────────┘
                 │
                 ▼
      xxHash3([name, child_names, child_hashes])
                 │
                 ▼
         DirNode hash: 0x333
```

---

## 5. Lazy Loading Pattern

### Full Tree Load (Inefficient)

```
Client wants: List files in /data/images/cats/

❌ Old approach:
1. Load CommitNode          → Network: 500 bytes
2. Load DirNode: data/      → Network: 1 KB + recursively load all children
3. Load DirNode: images/    → Network: 1 KB + recursively load all children
4. Load DirNode: cats/      → Network: 1 KB + recursively load all children
   └─ Load VNode 0...9      → Network: 10 * 100 KB = 1 MB
      └─ Load all files     → Network: 10,000 * 200 bytes = 2 MB
                              ─────────────────────────────────
                              Total: ~3 MB transferred

Time: 5-10 seconds on slow connection
```

### Oxen's Lazy Loading (Efficient)

```
Client wants: List files in /data/images/cats/

✅ Oxen approach:
1. Load CommitNode                  → Network: 500 bytes
2. Query dir_hashes for "data/images/cats"
   └─ Direct hash lookup: 0x333     → Network: 16 bytes (just hash)
3. Load DirNode: cats/ (depth=1)    → Network: 1 KB (just this node)
   └─ children: [VNode 0...9]       → Network: 10 * 100 bytes = 1 KB
                                     ─────────────────────────────
                                     Total: ~3 KB transferred

Time: <100ms on slow connection

Then load files on-demand:
4. User clicks VNode 0              → Network: 100 KB (VNode data)
   └─ FileNode list                 → Network: 10,000 * 200 bytes = 2 MB
                                     Only if user actually opens bucket!
```

**Speedup**: **1000x** for initial directory listing!

---

## 6. Content Addressing & Deduplication

### Without Content Addressing

```
Commit 1: Added cat-001.jpg
─────────────────────────
.oxen/commits/commit1/
  └─ files/
     └─ cat-001.jpg  (2.3 MB)

Commit 2: Added same image as dog-001.jpg
─────────────────────────
.oxen/commits/commit2/
  └─ files/
     └─ cat-001.jpg  (2.3 MB)  ← Duplicate!
     └─ dog-001.jpg  (2.3 MB)  ← Same file, different name

Total storage: 6.9 MB
```

### With Content Addressing (Oxen)

```
Commit 1: Added cat-001.jpg
─────────────────────────
.oxen/versions/objects/77/7abc...
  └─ <file content>  (2.3 MB)

FileNode:
  name: "cat-001.jpg"
  hash: 0x777abc...
  content_ref: .oxen/versions/objects/77/7abc...

Commit 2: Added same image as dog-001.jpg
─────────────────────────
.oxen/versions/objects/77/7abc...
  └─ <file content>  (2.3 MB)  ← Same blob, reused!

FileNode (commit 1):
  name: "cat-001.jpg"
  hash: 0x777abc...  ────┐
                         │
FileNode (commit 2):     │  Both point to same content!
  name: "dog-001.jpg"    │
  hash: 0x777abc...  ────┘

Total storage: 2.3 MB + metadata overhead
Savings: 66%
```

---

## 7. Comparison: Change Detection Algorithms

### Git's Approach

```
1. Scan working directory
2. Compare each file against index:
   - Compute hash of file content
   - Compare with staged hash
3. For changed files, update index
4. Build tree objects bottom-up
5. Create commit object

Time complexity: O(n) where n = total files
Problem: Always scans every file
```

### Oxen's Approach

```
1. Scan working directory
2. Filter by modification time:
   - Skip files older than last commit
3. Hash only potentially changed files
4. Compare hashes with previous tree
5. Rehash only affected VNodes and DirNodes
6. Create commit object

Time complexity: O(changes * log(depth))
Optimization: Skip unchanged files and subtrees
```

### Example: 100,000 files, 10 changed

| Approach | Files Hashed | Nodes Created | Time |
|----------|--------------|---------------|------|
| **Git** | 100,000 | ~1,000 (tree objects) | ~60s |
| **Oxen** | 10 | ~30 (VNodes + DirNodes) | ~0.5s |
| **Speedup** | 10,000x | 33x | **120x** |

---

## 8. Merkle Tree for Crucible Knowledge Base

### Proposed Structure

```
WorkspaceNode (root)
  hash: 0xWORKSPACE
  ├─ DirNode: research/
  │  hash: 0xRESEARCH
  │  ├─ DocumentNode: ai-trends.md
  │  │  content_hash: 0xCONTENT_AI
  │  │  metadata_hash: 0xMETA_AI
  │  │  combined_hash: 0xDOC_AI
  │  │  tags: ["AI", "research"]
  │  │  frontmatter: { title: "AI Trends 2025" }
  │  │
  │  └─ DocumentNode: ml-papers.md
  │     content_hash: 0xCONTENT_ML
  │     metadata_hash: 0xMETA_ML
  │     combined_hash: 0xDOC_ML
  │
  ├─ DirNode: ideas/
  │  hash: 0xIDEAS
  │  └─ DocumentNode: startup-ideas.md
  │     content_hash: 0xCONTENT_STARTUP
  │     metadata_hash: 0xMETA_STARTUP
  │
  └─ DirNode: daily-notes/
     hash: 0xDAILY
     num_documents: 5,234  ← Large directory!
     │
     ├─ VNode 0 (bucket 0)
     │  ├─ DocumentNode: 2025-01-15.md
     │  └─ ... (~500 notes)
     │
     ├─ VNode 1 (bucket 1)
     │  └─ ... (~500 notes)
     │
     └─ ... (VNodes 2-10)
```

### Change Detection Example

```
User edits: research/ai-trends.md
  Changes: Updated content, added tag "deep-learning"

Step 1: Parse markdown
─────────────────────────
Old frontmatter hash: 0xMETA_AI
New frontmatter hash: 0xMETA_AI_NEW  ← Tags changed

Old content hash: 0xCONTENT_AI
New content hash: 0xCONTENT_AI_NEW  ← Content changed

Step 2: Update DocumentNode
─────────────────────────
DocumentNode: ai-trends.md
  content_hash: 0xCONTENT_AI_NEW
  metadata_hash: 0xMETA_AI_NEW
  combined_hash: 0xDOC_AI_NEW

Step 3: Update DirNode
─────────────────────────
DirNode: research/
  Old hash: 0xRESEARCH
  New hash: 0xRESEARCH_NEW

Step 4: Update WorkspaceNode
─────────────────────────
WorkspaceNode:
  Old hash: 0xWORKSPACE
  New hash: 0xWORKSPACE_NEW

Step 5: Trigger downstream actions
─────────────────────────
✓ content_hash changed → Recompute embeddings
✓ metadata_hash changed → Update tag index
✓ workspace_hash changed → Broadcast to other clients
```

---

## 9. Real-time Sync Protocol

### Optimistic Sync (Root Hash Comparison)

```
┌─────────────┐                     ┌─────────────┐
│  Client A   │                     │  Client B   │
└──────┬──────┘                     └──────┬──────┘
       │                                   │
       │ File edited                       │
       │ Local hash: 0xNEW_A               │
       │                                   │
       │  WebSocket: broadcast_hash        │
       ├──────────────────────────────────►│
       │  { root_hash: "0xNEW_A" }         │
       │                                   │
       │                                   │ Compare hashes
       │                                   │ Local: 0xOLD_B
       │                                   │ Remote: 0xNEW_A
       │                                   │ → Different! Need sync
       │                                   │
       │  HTTP: GET /sync/diff             │
       │◄──────────────────────────────────┤
       │  ?from=0xOLD_B&to=0xNEW_A         │
       │                                   │
       │ Compute changed paths:            │
       │ - research/ai-trends.md           │
       │                                   │
       │  Response: { changes: [...] }     │
       ├──────────────────────────────────►│
       │                                   │
       │  HTTP: GET /documents/...         │
       │◄──────────────────────────────────┤
       │  ...?hash=0xNEW_A                 │
       │                                   │
       │  Document content                 │
       ├──────────────────────────────────►│
       │                                   │
       │                                   │ Apply changes
       │                                   │ Update local tree
       │                                   │ New hash: 0xNEW_A
       │                                   │
       │                                   │ ✓ In sync!
```

### Conflict Detection

```
Scenario: Both clients edit same file

Client A:                        Client B:
  ai-trends.md                     ai-trends.md
  (base hash: 0xBASE)              (base hash: 0xBASE)
         │                                │
         │ Edit content                   │ Edit content
         ▼                                ▼
  New hash: 0xEDIT_A               New hash: 0xEDIT_B
         │                                │
         │                                │
         ├────────► Server ◄──────────────┤
         │                                │
         │ Conflict detected!             │
         │ (both based on 0xBASE)         │
         │                                │
         │◄────── Conflict info ──────────►│
         │ { base: 0xBASE,                │
         │   yours: 0xEDIT_A,             │
         │   theirs: 0xEDIT_B }           │
         │                                │
         │ Show merge UI                  │ Show merge UI
```

---

## 10. Performance Visualization

### Scalability: Tree Depth vs. Directory Size

```
Flat structure (all files in root):
───────────────────────────────────
Root
└─ 100,000 files

Depth: 1
Traversal: O(100,000)
Change detection: O(100,000) - must check every file


Hierarchical structure with VNodes:
───────────────────────────────────
Root
├─ Dir A (10,000 files)
│  ├─ VNode 0 (5,000 files)
│  └─ VNode 1 (5,000 files)
├─ Dir B (10,000 files)
│  ├─ VNode 0 (5,000 files)
│  └─ VNode 1 (5,000 files)
└─ ... (10 directories)

Depth: 3 (Root → Dir → VNode → File)
Traversal: O(10 + 2 + 5,000) = O(5,012) - only load needed VNode
Change detection: O(10 + 2 + 1) = O(13) - skip unchanged subtrees
```

### Memory Usage

```
Without VNodes:
───────────────────────────────────
DirNode: 100,000 FileNode pointers
Memory: 100,000 * 8 bytes = 800 KB

FileNodes: 100,000 * 200 bytes = 20 MB

Total: ~21 MB


With VNodes (10,000 capacity):
───────────────────────────────────
DirNode: 10 VNode pointers
Memory: 10 * 8 bytes = 80 bytes

VNodes (loaded lazily):
  - Loaded: 1 VNode * 100 KB = 100 KB
  - Not loaded: 9 VNodes (0 KB)

FileNodes (in loaded VNode):
  - Loaded: 10,000 * 200 bytes = 2 MB
  - Not loaded: 90,000 * 0 bytes = 0 MB

Total: ~2.1 MB (10x reduction!)
```

---

## Summary

These diagrams illustrate the key concepts of Oxen's Merkle tree implementation:

1. **VNodes batch files** into manageable groups, preventing node explosion
2. **Hash-based change detection** enables O(changes) performance
3. **Lazy loading** minimizes memory usage and network transfer
4. **Content addressing** enables efficient deduplication
5. **Hierarchical hashing** allows skipping unchanged subtrees

For Crucible, adopting these patterns would enable **efficient knowledge base versioning, real-time sync, and scalable change detection**.

---

**Visual guide created by**: Claude (Anthropic)
**Date**: 2025-11-08
**Related**: `oxen-merkle-tree-analysis.md`
