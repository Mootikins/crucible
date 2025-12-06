# CRDT Architecture Research for Multi-Device Sync

**Date**: 2025-12-05
**Context**: Crucible knowledge management system with content-addressed blocks, Merkle trees, and SurrealDB storage
**Scenario**: Single user, multiple devices (laptop, desktop, phone), occasional sync (not real-time)

## Executive Summary

**Recommended Approach**: Hybrid Merkle-CRDT architecture using **Loro** or **Automerge 3.0** with content-addressed block storage.

**Key Recommendation**: Use Merkle-DAG sync protocol for efficient delta detection, combined with CRDT semantics for conflict-free merges.

---

## 1. CRDT Types for This Scenario

### Best Fit: Operation-Based CRDTs with Merkle-DAG Sync

For markdown notes with content-addressed blocks, the optimal approach combines:

1. **YATA/Fugue for Text** (used by Yjs/Loro)
   - Superior handling of concurrent text edits
   - Minimal interleaving anomalies when merging
   - Better performance than RGA (used by Automerge 2.0)

2. **LWW-Map for Metadata** (Last-Write-Wins)
   - Frontmatter properties
   - Tags and metadata
   - Simple timestamp-based resolution

3. **Merkle-CRDT for Sync Protocol**
   - Content-addressed blocks naturally align with Merkle-DAG structure
   - Efficient delta sync using hash comparison
   - Leverages existing block hashing infrastructure

### Implementation Options

| Library | Algorithm | Language | Storage Overhead | Pros | Cons |
|---------|-----------|----------|------------------|------|------|
| **Loro** | Fugue + YATA | Rust/WASM/Swift | Best (30-40%) | Native Rust, excellent performance, rich text support | Newer (less battle-tested) |
| **Automerge 3.0** | RGA | Rust/WASM | Good (30% at rest) | Mature, 10x memory improvement in v3.0 | Larger in-memory footprint historically |
| **Yjs** | YATA | JavaScript/WASM | Good (53% overhead) | Very mature, widely used, fast | JavaScript-first (WASM bridge) |
| **go-ds-crdt** | Merkle-CRDT | Go | Excellent | Production-ready (IPFS Clusters), 100M+ keys | Go integration overhead |

---

## 2. Handling Offline Edits with Occasional Sync

### Sync Protocol Architecture

**Phase 1: Delta Detection (Using Merkle Trees)**
```
1. Exchange root hashes
2. If roots differ:
   - Traverse Merkle tree to find divergent branches
   - Identify changed blocks using existing BlockHash
3. Only transfer CRDT operations for changed blocks
```

**Phase 2: Conflict-Free Merge (Using CRDT)**
```
1. Apply CRDT operations from peer
2. Automatic convergence (no conflict resolution UI needed)
3. Update local Merkle tree
4. Persist merged state
```

### Key Patterns

**State-Based vs Operation-Based**:
- **State-Based**: Transfer entire document state periodically (convergent)
  - Simpler implementation
  - Higher bandwidth (send full state)
  - Better for occasional sync with large gaps

- **Operation-Based**: Transfer deltas/updates (commutative)
  - More efficient bandwidth
  - Requires reliable delivery or delta compaction
  - **Recommended for occasional sync**: Use delta-state CRDTs

**Delta-State CRDTs**:
- Best of both worlds: send only recent changes
- Natural fit for content-addressed blocks
- Can fall back to full state sync if delta log grows too large

### Sync Frequency Considerations

For **occasional sync** (not real-time):
- Use **epoch-based compaction** to reduce history size
- Maintain "live" operations + "compacted" baseline
- Allow clients to sync from any prior epoch
- Servers can bring outdated clients up-to-date without client involvement

---

## 3. Existing Implementations to Learn From

### Production Systems Using CRDTs for Notes

1. **Apple Notes**
   - Uses CRDTs for multi-device sync
   - Single-user, occasional sync (same scenario as Crucible)
   - Proprietary implementation

2. **Actual Budget**
   - Open source, uses hybrid CRDT + Merkle tree approach
   - Production-ready with SQLite backend
   - Good reference for occasional sync patterns

3. **TomTom GPS**
   - CRDTs for favorite locations across devices
   - Demonstrates offline-first with eventual sync

### Technical Implementations

#### **Merkle-CRDTs** (IPFS/Protocol Labs Research)
- **Paper**: [Merkle-CRDTs: Merkle-DAGs meet CRDTs](https://research.protocol.ai/publications/merkle-crdts-merkle-dags-meet-crdts/)
- **Implementation**: go-ds-crdt (production-ready)
- **Key Innovation**: Use Merkle-DAG as logical clock and transport layer
- **Benefits for Crucible**:
  - Already have content-addressed blocks and Merkle trees
  - Natural integration path
  - Efficient sync protocol using existing hash infrastructure

**Architecture**:
```rust
// Existing Crucible types can map directly
BlockHash -> ContentID (in Merkle-DAG)
MerkleTree -> Merkle-CRDT transport layer
HashedBlock -> CRDT operation payload
```

#### **DefraDB** (Source Network)
- Uses Merkle-CRDTs for document versioning
- Each update creates new node in Merkle-DAG
- Sync protocol:
  1. Exchange CIDs (content identifiers)
  2. Compare and request missing data
  3. Verify using hash recalculation
  4. Merge into local DAG

#### **Loro** (Most Promising for Rust)
- Native Rust implementation
- Fugue algorithm for rich text (minimal interleaving)
- JSON-compatible schema (works with existing data models)
- Built-in features:
  - Offline editing
  - Version control
  - Time travel
  - Undo/redo (tracks user operations separately)
- Export/import for incremental sync:
  ```rust
  // Export updates since version vector
  doc.export(ExportMode::updates(&vv))
  // Import on other device
  doc.import(updates)
  ```

#### **Automerge 3.0**
- Mature, well-documented
- Dramatic memory improvements (10x reduction in v3.0)
- Storage overhead: 30% at rest (excellent)
- Binary format optimized for compression
- History storage for version control

---

## 4. Trade-offs Analysis

### Storage Overhead

| Aspect | CRDT Overhead | Crucible Impact | Mitigation |
|--------|---------------|-----------------|------------|
| **Document Size** | 30-53% overhead | Modest (markdown is small) | Compaction, history truncation |
| **History Storage** | Full operation log | Already storing block history in Merkle tree | Use Merkle-CRDT to deduplicate |
| **Tombstones** | Deleted blocks marked, not removed | Conflicts with content-addressing | Epoch-based GC with consensus |

**Specific Numbers** (from benchmarks):
- Loro/Automerge 3.0: ~30-40% overhead on final document
- Yjs: 53% overhead (160KB for 106KB original)
- Memory: Loro uses ~2.1MB for 260K edits (very efficient)

**Storage Strategy**:
```
Option 1: Store full CRDT state
  - Pro: Simple, complete history
  - Con: Storage grows unbounded

Option 2: Periodic compaction with epoch markers
  - Pro: Bounded storage, still supports sync
  - Con: Can't sync with clients older than oldest epoch

Option 3 (Recommended): Hybrid
  - Keep recent operations (30 days)
  - Compact older history into baseline
  - Full state export for ancient clients
```

### Sync Complexity

**Merkle-CRDT Approach** (Recommended):
```
Complexity: Medium
Benefits:
  - Leverages existing Merkle tree infrastructure
  - Efficient delta detection (compare root hashes)
  - Only transfer changed blocks
  - Content-addressed blocks are natural CIDs

Implementation Steps:
  1. Wrap existing BlockHash in CRDT operations
  2. Use Merkle tree for efficient diff
  3. Transfer only divergent CRDT ops
  4. Apply ops and rebuild Merkle tree
```

**Pure CRDT Approach** (Alternative):
```
Complexity: Lower
Benefits:
  - Standard libraries handle everything
  - No custom Merkle integration

Drawbacks:
  - Doesn't leverage existing content-addressing
  - May transfer more data (no Merkle optimization)
```

### Conflict UX

**Key Insight**: CRDTs eliminate conflict dialogs for text editing.

**What Users See**:
- **Text conflicts**: Automatically merged (may have interleaving)
  - Fugue/YATA minimize interleaving anomalies
  - Users can use undo/redo if merge is unexpected

- **Metadata conflicts**: Last-write-wins
  - Timestamp-based resolution
  - Deterministic (all devices converge to same state)

- **Structural conflicts** (frontmatter, tags):
  - Use LWW-Register for scalar values
  - Use OR-Set for tag collections (additive)

**Trade-off**: Automatic merges may surprise users occasionally, but this is far better than sync errors or "conflict copies".

**Best Practice**: Provide version history and time-travel to review/revert merges.

---

## 5. Recommended Architecture for Crucible

### Hybrid Merkle-CRDT Design

**Core Components**:

1. **Block-Level CRDTs**
   - Each content block (heading, paragraph, code) is a CRDT
   - Use Loro's Fugue for text blocks
   - Use LWW-Map for metadata blocks

2. **Merkle-DAG for Sync**
   - Existing BlockHash becomes ContentID
   - Merkle tree structure enables efficient delta detection
   - Only transfer CRDT operations for changed branches

3. **SurrealDB Integration**
   - Store CRDT state alongside existing schema
   - Persist operation log for history
   - Use version vectors for sync coordination

**Sync Protocol**:
```rust
// Pseudo-code for sync handshake
async fn sync_with_peer(peer: &Device) -> Result<()> {
    // 1. Exchange Merkle root hashes
    let local_root = self.merkle_tree.root_hash();
    let peer_root = peer.get_root_hash().await?;

    if local_root == peer_root {
        return Ok(()); // Already in sync
    }

    // 2. Find divergent blocks using Merkle diff
    let changes = self.merkle_tree.diff(&peer.merkle_tree).await?;

    // 3. Exchange CRDT operations for changed blocks only
    for change in changes {
        match change {
            TreeChange::ModifiedBlock { index, .. } => {
                let ops = peer.get_crdt_ops_for_block(index).await?;
                self.apply_crdt_ops(index, ops)?;
            }
            TreeChange::AddedBlock { index, .. } => {
                let block_state = peer.get_block_state(index).await?;
                self.add_block(index, block_state)?;
            }
            TreeChange::DeletedBlock { index, .. } => {
                self.mark_tombstone(index)?;
            }
            _ => {}
        }
    }

    // 4. Rebuild Merkle tree from merged state
    self.rebuild_merkle_tree()?;

    Ok(())
}
```

### Technology Choice: Loro (Recommended)

**Rationale**:
1. **Native Rust**: No FFI overhead, works with existing Rust codebase
2. **Performance**: Best-in-class for text CRDTs (Fugue algorithm)
3. **Storage Efficiency**: ~30-40% overhead, comparable to Automerge 3.0
4. **Rich Features**: Time travel, undo/redo, version control built-in
5. **JSON Schema**: Works with existing markdown + frontmatter model
6. **Export/Import**: Clean API for incremental sync

**Integration Path**:
```rust
use loro::{LoroDoc, ExportMode};

// Wrap Crucible blocks in Loro documents
struct CrucibleBlock {
    block_hash: BlockHash,
    loro_doc: LoroDoc,
    content: String,
}

impl CrucibleBlock {
    fn export_updates(&self, since: &VersionVector) -> Vec<u8> {
        self.loro_doc.export(ExportMode::updates(since))
    }

    fn import_updates(&mut self, updates: &[u8]) -> Result<()> {
        self.loro_doc.import(updates)?;
        self.block_hash = self.recompute_hash()?;
        Ok(())
    }
}
```

**Alternative**: Automerge 3.0 if Loro proves too new/unstable.

---

## 6. Implementation Phases

### Phase 1: CRDT Foundation (Weeks 1-2)
- [ ] Integrate Loro library
- [ ] Wrap existing blocks in Loro documents
- [ ] Implement block-level CRDT operations
- [ ] Add version vector tracking

### Phase 2: Sync Protocol (Weeks 3-4)
- [ ] Extend Merkle tree diffing for CRDT ops
- [ ] Implement delta exchange protocol
- [ ] Add SurrealDB schema for operation log
- [ ] Build sync coordination layer

### Phase 3: Compaction & GC (Week 5)
- [ ] Implement epoch-based history compaction
- [ ] Add tombstone garbage collection
- [ ] Build baseline state snapshots
- [ ] Optimize storage for long-running kilns

### Phase 4: Multi-Device Support (Week 6+)
- [ ] Device identity and authentication
- [ ] Sync transport (local network, cloud optional)
- [ ] Conflict visualization (for review, not resolution)
- [ ] Time-travel UI for reviewing merges

---

## 7. Key Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Loro is new/immature** | Integration bugs, API changes | Start with feature branch, fallback to Automerge 3.0 |
| **Storage bloat** | Large kiln databases | Implement compaction early, monitor in tests |
| **Merge surprises** | Users confused by auto-merge | Good time-travel UI, clear merge indicators |
| **Sync complexity** | Hard to debug, edge cases | Comprehensive integration tests, property-based testing |
| **Migration pain** | Existing kilns must migrate | Build backward-compatible import, validate thoroughly |

---

## 8. Performance Expectations

**Based on Benchmarks**:
- **Sync Speed**: 400+ blocks/second (go-ds-crdt benchmark)
- **Storage**: 30-53% overhead (acceptable for markdown)
- **Memory**: ~2MB for 260K operations (very efficient)
- **Large Documents**: Yjs handles 10M+ characters (linear scaling)

**Crucible Specific**:
- Average markdown note: 1-5 KB (tiny)
- Blocks per note: 10-50 (headings, paragraphs)
- Storage overhead per note: ~500 bytes - 2.5 KB (negligible)
- Kiln with 1000 notes: ~1-3 MB overhead (acceptable)

---

## 9. References & Sources

### Research Papers
- [Merkle-CRDTs: Merkle-DAGs meet CRDTs](https://research.protocol.ai/publications/merkle-crdts-merkle-dags-meet-crdts/) - Protocol Labs
- [Peritext: A CRDT for Rich-Text Collaboration](https://www.inkandswitch.com/peritext/) - Ink & Switch

### Implementation Guides
- [Automerge Binary Format](https://automerge.org/automerge-binary-format-spec/)
- [Automerge Storage](https://automerge.org/docs/reference/under-the-hood/storage/)
- [Loro Documentation](https://loro.dev/docs)
- [Yjs Documentation](https://docs.yjs.dev/)

### Performance Analysis
- [Are CRDTs Suitable for Shared Editing](https://blog.kevinjahns.de/are-crdts-suitable-for-shared-editing) - Kevin Jahns (Yjs author)
- [CRDT Benchmarks Repository](https://github.com/dmonad/crdt-benchmarks)
- [Loro Performance Benchmarks](https://www.loro.dev/docs/performance)
- [CRDTs Go Brrr](https://josephg.com/blog/crdts-go-brrr/) - Joseph Gentle

### Architecture Examples
- [DefraDB: Merkle CRDTs for Data Consistency](https://open.source.network/blog/how-defradb-uses-merkle-crdts-to-maintain-data-consistency-and-conflict-free)
- [IPFS go-ds-crdt](https://github.com/ipfs/go-ds-crdt) - Production implementation
- [Local-First, Forever](https://tonsky.me/blog/crdt-filesync/) - File sync with CRDTs

### Comparisons & Analysis
- [Automerge vs Yjs Discussion](https://github.com/yjs/yjs/issues/145)
- [Main Takeaways from Yjs and Automerge](https://news.ycombinator.com/item?id=29507948) - Hacker News
- [In Practice Projects Use Yjs](https://news.ycombinator.com/item?id=41012895) - Hacker News

### Storage & Optimization
- [Automerge 2.0 Release](https://automerge.org/blog/automerge-2/)
- [Automerge 3.0 Memory Improvements](https://automerge.org/blog/automerge-3/)
- [Loro Document Size Analysis](https://www.loro.dev/docs/performance/docsize)
- [CRDT Compaction & GC Ideas](https://github.com/csw/crdt-flix/wiki/GC-ideas)

### Production Examples
- [Decipad: Collaborative Editing with CRDTs](https://www.decipad.com/blog/decipads-innovative-method-collaborative-and-offline-editing-using-crdts)
- [Offline Sync Using CRDTs](https://dev.to/ebuckley/offline-eventually-consistent-synchronization-using-crdts-2826)
- [SyncedStore: CRDT Real-Time Sync](https://syncedstore.org/docs/)

### General Resources
- [CRDT.tech](https://crdt.tech/) - Comprehensive resource directory
- [Awesome CRDT](https://github.com/alangibson/awesome-crdt) - Curated list
- [Redis: Diving into CRDTs](https://redis.io/blog/diving-into-crdts/)
- [What are CRDTs - Loro](https://www.loro.dev/docs/concepts/crdt)

---

## 10. Conclusion

**Recommended Stack**: Loro + Merkle-DAG sync protocol

**Why This Works for Crucible**:
1. Already have content-addressed blocks (BlockHash)
2. Already have Merkle trees for integrity
3. Occasional sync fits delta-state CRDT model perfectly
4. Single-user, multi-device is ideal for CRDTs (simpler than multi-user)
5. Rust implementation (Loro) integrates cleanly

**Next Steps**:
1. Prototype Loro integration with single block
2. Extend Merkle diff algorithm for CRDT ops
3. Build sync protocol POC (two local devices)
4. Performance testing with realistic kiln sizes
5. Design compaction strategy based on real usage patterns

**Success Criteria**:
- Sync 1000 notes in <5 seconds
- <50% storage overhead with compaction
- Zero user-visible conflicts (auto-merge)
- Support 30-day operation window before compaction required
- Graceful degradation for very old clients (full state sync fallback)
