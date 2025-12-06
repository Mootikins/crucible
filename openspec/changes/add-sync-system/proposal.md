# Sync System: Merkle-CRDT Architecture

## Why

Crucible needs to sync knowledge across boundaries:
- **Multi-device** - Same user, laptop/desktop/phone, occasional sync
- **Collaboration** - Multiple users editing same vault, real-time
- **Federation** - A2A agents across networks, eventual consistency, untrusted peers

Currently there's no sync - each instance is isolated. This change adds a unified sync architecture that handles all three scenarios with the same core primitives.

## Key Insight

All three scenarios converge on **Merkle-CRDT**:
- Use existing content-addressed blocks and Merkle trees
- Add CRDT layer per block for conflict-free merging
- Different transports/trust models, same sync protocol

```
┌─────────────────────────────────────────────────────────┐
│                    Transport Layer                       │
│  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ Local FS │  │  WebSocket   │  │  Libp2p/Gossip    │  │
│  │(devices) │  │(collaboration)│  │  (federation)    │  │
│  └────┬─────┘  └──────┬───────┘  └─────────┬─────────┘  │
└───────┼───────────────┼────────────────────┼────────────┘
        │               │                    │
        ▼               ▼                    ▼
┌─────────────────────────────────────────────────────────┐
│                   Sync Protocol                          │
│           Merkle root compare → diff → fetch             │
│           (same algorithm, different transports)         │
└─────────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│                    CRDT Layer                            │
│  Per-Block CRDT (Loro for text, LWW for metadata)       │
│  Optional: BFT signatures for untrusted peers           │
└─────────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│              Existing Infrastructure                     │
│    Content-addressed blocks → Merkle tree → SurrealDB   │
└─────────────────────────────────────────────────────────┘
```

## What Changes

### Sync Localities

Three sync modes, configured per vault:

**Local** (multi-device):
- Sync via shared folder, mDNS discovery, or manual export/import
- No coordinator needed
- Fully trusted (same user)

**Coordinated** (collaboration):
- Coordinator server for discovery, presence, relay
- WebSocket for real-time updates
- Authenticated users with capability tokens

**Federated** (A2A):
- DHT or bootstrap nodes for discovery
- Gossip protocol for sync
- BFT-CRDT for untrusted peers
- Capability-based partial sharing

### CRDT Types

| Content Type | CRDT | Notes |
|-------------|------|-------|
| Text blocks | Loro (Fugue) | Minimal interleaving, Rust-native |
| Frontmatter | LWW-Register | Timestamp-based, simple |
| Tags | OR-Set | Add-wins, no conflicts |
| Links/refs | OR-Set | Additive |
| Block order | RGA | Ordered list CRDT |

### Merkle-CRDT Sync Protocol

```
1. Exchange Merkle root hashes
2. If roots match → already synced, done
3. If roots differ → walk tree to find divergent blocks
4. Exchange CRDT operations for divergent blocks only
5. Merge operations (automatic, conflict-free)
6. Rebuild Merkle tree from merged state
7. Repeat until roots match
```

### Coordinator Server

For collaboration mode, a lightweight coordinator provides:

| Endpoint | Purpose |
|----------|---------|
| `/discover` | Register/find peers editing same vault |
| `/presence` | Cursor positions, who's online |
| `/relay` | WebSocket relay for NAT traversal |
| `/auth` | Token issuance and verification |
| `/sync` | Optional: Merkle hub for offline peers |

Coordinator is optional - clients sync P2P after discovery.

### Storage Overhead

| Aspect | Overhead | Mitigation |
|--------|----------|------------|
| CRDT metadata | ~30-40% | Epoch-based compaction |
| Operation log | Unbounded | GC after sync confirmed |
| Tombstones | Accumulate | Block-level aggregation |

For a 1000-note vault: ~1-3 MB total CRDT overhead.

### Trust Models

**Local**: No verification needed (same user)

**Coordinated**:
- Coordinator issues capability tokens
- Tokens specify: read/write, which paths, expiry
- Clients verify each other's tokens

**Federated**:
- All operations signed with agent keys
- BFT-CRDT tolerates Byzantine peers
- Merkle proofs for partial verification
- Capability tokens for selective sharing

## Impact

### Affected Specs

- **sync** (NEW) - Core sync architecture
- **storage** (extends) - CRDT layer on blocks
- **agents** (reference) - A2A sync uses agent identities
- **apis** (reference) - Coordinator endpoints

### Affected Code

**New Components:**
- `crates/crucible-sync/` - NEW crate
  - `src/crdt/` - Loro integration, LWW, OR-Set wrappers
  - `src/merkle_crdt.rs` - Merkle-CRDT sync protocol
  - `src/transport/` - Local, WebSocket, Libp2p transports
  - `src/coordinator/` - Discovery server (optional binary)
  - `src/capabilities.rs` - Token-based access control

**Modified Components:**
- `crates/crucible-core/src/storage/` - Add CRDT wrapper for blocks
- `crates/crucible-surrealdb/` - Store CRDT metadata alongside blocks
- `crates/crucible-cli/` - Add `cru sync` commands

### New Dependencies

- `loro = "1.0"` - Rust CRDT library
- `libp2p` - P2P networking (federation)
- `tokio-tungstenite` - WebSocket (collaboration)

## Design Decisions

1. **Loro over Automerge** - Native Rust, Fugue algorithm has less interleaving, good benchmarks. Fallback to Automerge if issues.

2. **Block-level CRDTs** - Not character-level for whole vault. Each block is a CRDT document. Reduces tombstone growth, matches content-addressing.

3. **Merkle diff first** - Always compare Merkle roots before syncing. Only transfer divergent blocks. Huge bandwidth savings.

4. **Coordinator optional** - Local sync works without any server. Coordinator only needed for real-time collaboration.

5. **BFT as opt-in** - Signature overhead only for federated/untrusted mode. Local/coordinated skip it.

6. **Capabilities not ACLs** - Token-based access scales better for federation. Tokens are self-contained, verifiable offline.

## Future Work

- **Streaming sync** - Sync large vaults incrementally
- **Selective sync** - Only sync certain folders/tags
- **Encryption at rest** - E2E encryption for federated mode
- **Coordinator clustering** - HA coordinator deployment
- **Conflict visualization** - Time-travel UI for reviewing merges

## Open Questions

1. **Yjs for real-time MVP?** - Yjs is more battle-tested for real-time collab. Use Yjs initially, migrate to Loro?

2. **Coordinator persistence** - Should coordinator store vault state, or just facilitate P2P?

3. **Mobile sync** - Different constraints (battery, bandwidth). Defer to Phase 2?

4. **Version history** - How long to keep operation logs? Configurable per vault?
