# CRDT Architecture for Real-Time Collaboration in Crucible

**Research Date:** 2025-12-05
**Context:** Knowledge management system with Markdown notes, content-addressed blocks, Merkle trees, and SurrealDB storage
**Goal:** Google Docs-style real-time collaboration for multi-user vault editing

---

## Executive Summary

**Recommended Architecture:** Hybrid CRDT with **Yjs** for text editing + **custom CRDT** for block-level metadata synchronization, integrated with existing Merkle tree infrastructure.

**Key Trade-offs:**
- **Latency:** <50ms for local edits, <200ms for remote peer sync
- **Storage:** ~2x overhead for tombstones (mitigated by periodic GC)
- **Complexity:** Medium - leverage Yjs ecosystem, extend for block metadata

---

## 1. CRDT Type Recommendations

### 1.1 Text Editing: Yjs (Recommended Primary)

**Why Yjs:**
- **Battle-tested:** Powers Tiptap, ProseMirror, CodeMirror editors in production
- **Performance:** Flat array storage with run-length encoding = 14x memory savings vs naive CRDT
- **Ecosystem:** Rich bindings for editors, awareness protocol built-in
- **Language:** JavaScript/TypeScript with WebAssembly optimizations

**Architecture:**
```
User Edit → Yjs Document → Delta Encoding → WebSocket/WebRTC → Remote Peers
                ↓
         Periodic Snapshot → SurrealDB
```

**Pros:**
- Mature awareness protocol for cursors/presence (see section 2)
- Garbage collection built-in (tombstone pruning)
- Works with Markdown via CodeMirror/Monaco bindings
- Active community, well-documented

**Cons:**
- JavaScript-first (requires FFI bridge from Rust or separate service)
- Not ideal for structured metadata (frontmatter, tags) - use separate CRDT

**Implementation Path:**
1. Run Yjs server as Node.js microservice (y-websocket or y-webrtc)
2. Rust CLI/desktop app connects via WebSocket
3. Bridge Yjs updates to ParsedNote structure

---

### 1.2 Alternative: Diamond Types (Rust Native)

**Why Diamond Types:**
- **Performance:** 5000x faster than Automerge in benchmarks
- **Native Rust:** Direct integration, no FFI overhead
- **Memory:** B-tree indexing with O(log n) operations
- **Architecture:** (client_id, sequence_number) identifier system

**Architecture:**
```
User Edit → Diamond Types OpLog → Compress to Deltas → Broadcast
                ↓
         Apply to Rope → Update ParsedNote
```

**Pros:**
- Zero FFI overhead, pure Rust
- Exceptionally fast (56ms vs 291s Automerge for reference trace)
- Smaller memory footprint than Yjs

**Cons:**
- **WIP status:** Still in development, API may change
- **No awareness built-in:** Must implement cursor tracking separately
- **Limited ecosystem:** No editor bindings yet
- **New architecture:** Less production validation

**Verdict:** Monitor for future adoption when API stabilizes. Use for Phase 2 if Yjs proves too heavyweight.

---

### 1.3 Block Metadata: Custom Last-Write-Wins (LWW) CRDT

For non-text data (tags, frontmatter, block hashes):

**Why Custom LWW:**
- **Simple semantics:** Timestamp + node ID determines winner
- **Fits Crucible model:** Frontmatter fields are key-value pairs
- **Low overhead:** No tombstones for metadata updates

**Data Structure:**
```rust
struct MetadataEntry {
    value: serde_json::Value,
    timestamp: u64,        // Lamport/hybrid logical clock
    node_id: NodeId,       // Disambiguate concurrent edits
    merkle_hash: String,   // Link to Merkle tree block
}
```

**Merge Logic:**
```rust
fn merge(local: MetadataEntry, remote: MetadataEntry) -> MetadataEntry {
    if remote.timestamp > local.timestamp {
        remote
    } else if remote.timestamp == local.timestamp {
        // Deterministic tie-break by node_id
        if remote.node_id > local.node_id { remote } else { local }
    } else {
        local
    }
}
```

**Pros:**
- Trivial implementation (~200 LOC)
- No tombstones for metadata
- Works seamlessly with SurrealDB versioning

**Cons:**
- Last-write-wins = potential data loss if concurrent edits to same field
- Mitigate: Use fine-grained keys (tag.0, tag.1 instead of tags array)

---

## 2. Cursor & Awareness Implementation

### 2.1 Yjs Awareness Protocol (Recommended)

**How It Works:**
- Separate state-based CRDT from main document
- Propagates JSON objects (user name, color, cursor position)
- Auto-deletes on disconnect
- No persistence required

**Data Format:**
```typescript
{
  user: { name: "Alice", color: "#FF6B6B", id: "uuid-1234" },
  cursor: {
    anchor: 142,     // Yjs position (stable across edits)
    head: 156,       // Selection end
    blockHash: "abc123" // Crucible block reference
  },
  lastActivity: 1701820800
}
```

**Position Stability:**
Yjs positions are **relative identifiers** - when text is inserted before position 37, the position automatically adjusts to point to the same character. This solves the "cursor drift" problem elegantly.

**Implementation:**
```javascript
// Server (y-websocket)
const awareness = doc.getAwareness();
awareness.on('change', ({ added, updated, removed }) => {
  broadcastToClients({ added, updated, removed });
});

// Client
awareness.setLocalStateField('cursor', { anchor: 142, head: 156 });
```

**Rust Integration:**
- Run awareness as separate WebSocket channel
- Broadcast updates via existing WebSocket connection
- Render in Tauri/desktop UI using awareness state

---

### 2.2 Custom Awareness for Diamond Types

If using Diamond Types, implement awareness manually:

**Data Structure:**
```rust
struct AwarenessState {
    users: HashMap<NodeId, UserPresence>,
    last_gc: Instant,
}

struct UserPresence {
    name: String,
    color: Color,
    cursor: CursorPosition,
    last_seen: Instant,
}

struct CursorPosition {
    // Map to Diamond Types position
    client_id: u32,
    sequence: u32,
    block_index: usize,  // Crucible block reference
}
```

**Broadcast Strategy:**
- Send awareness updates every 50-100ms (debounced)
- Use separate WebSocket message type (not CRDT ops)
- Expire entries after 30s of inactivity

**Cursor Position Mapping:**
```rust
// Convert Diamond Types position to visual cursor
fn map_to_cursor(op_id: (u32, u32), doc: &DiamondDoc) -> usize {
    doc.position_at(op_id)  // O(log n) lookup via B-tree
}
```

---

## 3. Existing Implementations to Learn From

### 3.1 Production References

| System | CRDT Type | Language | Key Lesson |
|--------|-----------|----------|------------|
| **Notion** | Custom OT/CRDT hybrid | Go/TypeScript | Use server authority for conflicts |
| **Figma** | Custom CRDT | Rust/C++ | Optimize for small delta sizes |
| **Tiptap Collab** | Yjs | TypeScript | Awareness is essential UX |
| **Zed Editor** | Custom CRDT | Rust | Low-latency requires careful buffer design |

### 3.2 Open Source Codebases

**Yjs Ecosystem:**
- **y-websocket:** WebSocket provider (server authority)
- **y-webrtc:** Peer-to-peer (no server)
- **y-crdt (Rust):** Rust port of Yjs (experimental)

**Study These:**
```bash
# Awareness implementation
https://github.com/yjs/y-protocols/blob/master/awareness.js

# Cursor position tracking
https://github.com/ueberdosis/tiptap/tree/main/packages/extension-collaboration-cursor

# Delta compression
https://github.com/yjs/yjs/blob/main/src/utils/encoding.js
```

**Diamond Types:**
```bash
# Position mapping
https://github.com/josephg/diamond-types/blob/main/src/list/mod.rs

# Operation log structure
https://github.com/josephg/diamond-types/blob/main/src/list/oplog.rs
```

---

## 4. Trade-offs & Operational Concerns

### 4.1 Latency Requirements

**Target Latency:**
- **Local edits:** <16ms (60 FPS feel)
- **Remote sync (LAN):** <50ms
- **Remote sync (WAN):** <200ms
- **Conflict resolution:** <10ms

**Strategies:**
- **Optimistic UI:** Apply edits immediately, reconcile async
- **Delta compression:** Send only changed operations (Yjs does this)
- **Batching:** Group operations every 50ms before broadcast

**Crucible-Specific:**
- **Block-level sync:** Only sync changed blocks via Merkle diff
- **Lazy frontmatter sync:** Don't block text edits on metadata

---

### 4.2 Server Architecture

**Option A: Centralized (Recommended for MVP)**
```
┌─────────┐         ┌─────────────┐         ┌─────────┐
│ Client 1│←WebSocket→│ Yjs Server  │←WebSocket→│ Client 2│
└─────────┘         │ (Node.js)   │         └─────────┘
                    │ + SurrealDB │
                    └─────────────┘
```

**Pros:**
- Simple conflict resolution (server is authority)
- Easy to add persistence snapshots
- Can enforce access control

**Cons:**
- Single point of failure
- Latency depends on server location

---

**Option B: Peer-to-Peer (Future Enhancement)**
```
┌─────────┐         ┌─────────┐
│ Client 1│←WebRTC──→│ Client 2│
└─────────┘         └─────────┘
     │                   │
     └────WebRTC─────────┘
          (Mesh)
```

**Pros:**
- Lower latency (direct peer connection)
- No server dependency for editing
- Better for offline-first

**Cons:**
- Complex NAT traversal
- Harder to debug
- Need signaling server anyway

**Verdict:** Start with centralized, add P2P as opt-in feature later.

---

### 4.3 Tombstone Growth Mitigation

**Problem:** CRDTs for lists (text) keep tombstones for all deleted characters. A Wikipedia article with 553 lines can accumulate 1.6M tombstones.

**Solutions:**

**1. Periodic Garbage Collection (Yjs Built-in)**
```javascript
// Yjs GC: removes tombstones older than N edits
doc.gc = true;  // Enable auto GC
```

**How it works:**
- Wait until all clients have synced past a certain version
- Remove tombstones no longer needed for merge
- Requires weak synchronization (not on critical path)

**Crucible Integration:**
- Run GC when file is closed by all users
- Store pre-GC snapshot in SurrealDB for history
- Trigger GC after N operations (e.g., 10,000)

---

**2. Separate Text & Tombstone Storage (Xi Editor Pattern)**
```rust
struct Document {
    visible_text: Rope,        // Fast access, no tombstones
    tombstones: BTreeSet<OpId>, // Compact deleted ranges
}
```

**Crucible Implementation:**
```rust
struct CollaborativeNote {
    // Visible content (no tombstones)
    content: String,

    // CRDT operation log (includes deletes)
    operation_log: Vec<CrdtOp>,

    // Compact tombstone representation
    deleted_ranges: Vec<(usize, usize)>,  // Run-length encoded
}
```

---

**3. Block-Level Tombstones (Leverage Existing Merkle Tree)**

**Key Insight:** Crucible already has block-level hashing! Use this for coarse-grained GC.

```rust
struct BlockTombstone {
    block_hash: BlockHash,
    deleted_at: Timestamp,
    deleted_by: NodeId,
}

// Only keep character-level tombstones for active blocks
// Discard tombstones when entire block is deleted
```

**Benefit:** Reduces tombstone growth by 10-100x (depends on block size).

---

**4. Snapshot + Compact (Aggressive GC)**

Every N hours:
1. Create snapshot of current document state
2. Discard all operation history before snapshot
3. New operations reference snapshot as base

**Trade-off:** Lose fine-grained history, but storage stays bounded.

**Crucible Use Case:** Perfect for archived notes (export snapshot to static Markdown).

---

## 5. Recommended Architecture for Crucible

### 5.1 Phase 1: Text-Only Collaboration (MVP)

**Components:**
1. **Yjs Server** (Node.js)
   - y-websocket provider
   - Snapshot to SurrealDB every 5 minutes
   - Awareness protocol for cursors

2. **Crucible Client** (Rust)
   - WebSocket client to Yjs server
   - Render Yjs updates via CodeMirror/Monaco editor
   - Convert Yjs snapshots to `ParsedNote` on save

3. **Storage**
   - **Hot path:** Yjs in-memory document
   - **Cold path:** SurrealDB snapshots (every 5 min or on close)
   - **Merkle tree:** Recompute from Yjs snapshot on save

**Flow:**
```
User Types
    ↓
Yjs.applyUpdate(delta)
    ↓
Broadcast to peers (WebSocket)
    ↓
[Every 5 min] Snapshot to SurrealDB
    ↓
[On close] Recompute Merkle tree + store ParsedNote
```

---

### 5.2 Phase 2: Block-Level Sync with Merkle Diffing

**Enhancement:** Use existing Merkle tree to sync only changed blocks.

**Protocol:**
1. Client opens note → sends Merkle root hash
2. Server compares with latest root
3. If match → no sync needed (fast path!)
4. If mismatch → traverse Merkle tree to find changed blocks
5. Sync only deltas for changed blocks via Yjs

**Benefits:**
- Huge bandwidth savings for large notes
- Works perfectly with existing `block_hashes` field in `ParsedNote`
- Enables efficient "lazy loading" (download blocks on scroll)

**Implementation:**
```rust
// Server
fn sync_blocks(client_root: &MerkleRoot, server_tree: &MerkleTree) -> Vec<BlockDelta> {
    let changes = server_tree.compare_with(&client_tree);
    changes.iter()
        .filter_map(|change| match change {
            TreeChange::ModifiedBlock { index, .. } => {
                Some(BlockDelta { index, yjs_updates: ... })
            }
            _ => None
        })
        .collect()
}
```

---

### 5.3 Phase 3: Metadata CRDT

**Add LWW-CRDT for:**
- Frontmatter fields (title, tags, dates)
- Block-level metadata (headings, code language)
- Wikilink targets (resolve conflicts deterministically)

**Storage:**
```rust
struct MetadataCrdt {
    entries: HashMap<String, MetadataEntry>,
    clock: HybridLogicalClock,
}

// Merge on sync
fn merge_metadata(&mut self, remote: MetadataCrdt) {
    for (key, remote_entry) in remote.entries {
        let local_entry = self.entries.entry(key).or_default();
        *local_entry = local_entry.merge(remote_entry);
    }
}
```

**Crucible Integration:**
- Sync metadata separately from text (different WebSocket channel)
- Update `ParsedNote.frontmatter` on merge
- Re-extract tags/wikilinks on conflict resolution

---

## 6. Proof of Concept: Minimal Viable Prototype

### 6.1 Tech Stack

**Server:**
- Node.js 20+
- y-websocket (npm)
- ws (WebSocket library)
- SurrealDB client (for snapshots)

**Client:**
- Rust + Tauri (or keep CLI for testing)
- tungstenite (WebSocket)
- CodeMirror 6 (via Tauri webview)
- yrs (Rust Yjs port - experimental)

---

### 6.2 Minimal Implementation (300 LOC)

**Server (server.js):**
```javascript
const Y = require('yjs');
const { WebSocketServer } = require('ws');
const { setupWSConnection } = require('y-websocket/bin/utils');

const wss = new WebSocketServer({ port: 1234 });

// In-memory document store
const docs = new Map();

wss.on('connection', (ws, req) => {
  const docName = new URL(req.url, 'http://localhost').searchParams.get('doc');
  setupWSConnection(ws, req, { docName, docs });

  // Snapshot to SurrealDB every 5 min (TODO)
});

console.log('Yjs server running on ws://localhost:1234');
```

**Client (Rust):**
```rust
use tungstenite::{connect, Message};
use yrs::{Doc, Text, Transact};

fn main() {
    // Connect to Yjs server
    let (mut socket, _) = connect("ws://localhost:1234?doc=test-note").unwrap();

    // Create Yjs document
    let doc = Doc::new();
    let text = doc.get_or_insert_text("content");

    // Apply local edit
    {
        let mut txn = doc.transact_mut();
        text.insert(&mut txn, 0, "Hello, CRDT!");
    }

    // Send update to server
    let update = doc.encode_state_as_update_v1(&yrs::StateVector::default());
    socket.write_message(Message::Binary(update)).unwrap();

    // Receive updates from server
    loop {
        let msg = socket.read_message().unwrap();
        if let Message::Binary(data) = msg {
            let mut txn = doc.transact_mut();
            yrs::Update::decode_v1(&data).unwrap().apply(&mut txn);
            println!("Synced text: {}", text.to_string(&txn));
        }
    }
}
```

---

### 6.3 Testing Strategy

**Unit Tests:**
- Concurrent edits from 2 clients converge
- Tombstone GC reduces memory
- Merkle tree diff matches Yjs changes

**Integration Tests:**
- 10 concurrent clients editing same note
- Network partition simulation (split-brain)
- Snapshot restore matches live state

**Performance Benchmarks:**
- Latency: local edit → remote update
- Throughput: edits/sec per client
- Memory: tombstone growth over 1 hour

---

## 7. Key Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| **Yjs requires Node.js** | Adds deployment complexity | Use `yrs` (Rust port) or run as sidecar |
| **Tombstone growth** | Memory bloat over time | Periodic GC + block-level compaction |
| **Network partitions** | Divergent state | Server authority + conflict markers |
| **Race condition: Merkle vs CRDT** | Hash mismatch after merge | Recompute Merkle AFTER Yjs sync completes |
| **FFI overhead (Rust ↔ JS)** | Latency spike | Use pure Rust `yrs` or separate service |

---

## 8. Decision Matrix

| Criterion | Yjs | Diamond Types | Custom CRDT |
|-----------|-----|---------------|-------------|
| **Production Ready** | ✅ Yes | ⚠️ WIP | ❌ No |
| **Rust Native** | ⚠️ Via yrs | ✅ Yes | ✅ Yes |
| **Editor Support** | ✅ Excellent | ❌ None | ❌ Manual |
| **Awareness Built-in** | ✅ Yes | ❌ No | ❌ No |
| **Performance** | 🟢 Good | 🟢 Excellent | 🟡 Depends |
| **Tombstone GC** | ✅ Built-in | ⚠️ Manual | ❌ Manual |
| **Learning Curve** | 🟢 Low | 🟡 Medium | 🔴 High |
| **Maintenance Burden** | 🟢 Low (OSS) | 🟡 Medium | 🔴 High (custom) |

**Verdict:** Start with **Yjs** for MVP, monitor **Diamond Types** for Phase 2 migration.

---

## 9. Implementation Roadmap

### Phase 1: Basic Collaboration (2-4 weeks)
- [x] Research CRDT options
- [ ] Set up Yjs server (y-websocket)
- [ ] Rust WebSocket client + Yjs integration
- [ ] Awareness protocol (cursor positions)
- [ ] Basic conflict resolution

**Success Criteria:** 2 users can edit same note, see each other's cursors.

---

### Phase 2: Merkle Integration (2-3 weeks)
- [ ] Merkle diff algorithm (compare trees)
- [ ] Block-level sync protocol
- [ ] Incremental updates (only changed blocks)
- [ ] Snapshot to SurrealDB (periodic)

**Success Criteria:** Large note syncs only changed blocks, not full document.

---

### Phase 3: Metadata CRDT (1-2 weeks)
- [ ] LWW-CRDT for frontmatter
- [ ] Hybrid logical clock implementation
- [ ] Tag/wikilink conflict resolution
- [ ] UI for viewing conflicts

**Success Criteria:** Concurrent frontmatter edits merge deterministically.

---

### Phase 4: Production Hardening (2-3 weeks)
- [ ] Tombstone garbage collection
- [ ] Network resilience (reconnect, retry)
- [ ] Access control (who can edit)
- [ ] Audit log (who edited what)
- [ ] Performance optimization (delta compression)

**Success Criteria:** System handles 100 concurrent users, 1M character documents.

---

## 10. Recommended Next Steps

1. **Prototype with Yjs (1 week):**
   - Run example server from y-websocket docs
   - Build minimal Rust client (tungstenite + yrs)
   - Verify cursor awareness works

2. **Integrate with Crucible (1 week):**
   - Map Yjs text → ParsedNote.content
   - Trigger Merkle recompute on Yjs snapshot
   - Store snapshots in SurrealDB

3. **Benchmark (3 days):**
   - Latency under load (10 concurrent clients)
   - Memory growth (tombstones over 1 hour)
   - Bandwidth (delta size vs full sync)

4. **Decide on Diamond Types (1 week):**
   - If Yjs performance inadequate, spike Diamond Types
   - Compare implementation effort vs performance gain
   - Make Go/No-Go decision

---

## 11. References

**Academic Papers:**
- "CRDTs: Consistency without concurrency control" (Shapiro et al., 2011)
- "A comprehensive study of Convergent and Commutative Replicated Data Types" (Shapiro et al., 2011)

**Blog Posts:**
- Joseph Gentle: ["CRDTs go brrr"](https://josephg.com/blog/crdts-go-brrr/) - Performance optimizations
- Tag1 Consulting: ["Yjs Deep Dive Part 3"](https://www.tag1consulting.com/blog/yjs-deep-dive-part-3) - Awareness protocol

**Documentation:**
- [Yjs Docs](https://docs.yjs.dev/)
- [Diamond Types Repo](https://github.com/josephg/diamond-types)
- [Tiptap Collaboration](https://tiptap.dev/docs/collaboration/core-concepts/awareness)

**Codebases:**
- y-websocket: https://github.com/yjs/y-websocket
- y-crdt (Rust): https://github.com/y-crdt/y-crdt
- Hocuspocus (Tiptap server): https://github.com/ueberdosis/hocuspocus

---

## Appendix A: Glossary

- **CRDT:** Conflict-free Replicated Data Type - data structure that auto-merges concurrent edits
- **OT:** Operational Transformation - alternative to CRDTs, requires server authority
- **Tombstone:** Metadata marking deleted item (needed for CRDT merge)
- **LWW:** Last-Write-Wins - simple CRDT where newest edit wins
- **Awareness:** Non-persistent state (cursors, presence) separate from document
- **Delta Encoding:** Send only changed operations, not full state
- **GC:** Garbage Collection - removing tombstones no longer needed for merge

---

## Appendix B: Crucible-Specific Constraints

**Existing Architecture:**
- Markdown files = source of truth
- ParsedNote structure (wikilinks, tags, frontmatter, content)
- Merkle trees for integrity (block-level hashing)
- SurrealDB for persistence
- Block-level embeddings for semantic search

**Must Preserve:**
- File-based storage (CRDT is synchronization layer, not storage)
- Merkle tree integrity checks
- Block-level change detection (leverage in CRDT sync)

**Can Enhance:**
- Real-time sync (currently file-watch based)
- Multi-user editing (currently single-user)
- Conflict resolution (currently last-write-wins at file level)

---

**End of Research Document**
