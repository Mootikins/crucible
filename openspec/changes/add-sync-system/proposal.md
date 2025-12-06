# Sync System: Merkle Diff + CRDT Sessions

## Why

Crucible needs to sync knowledge across boundaries:
- **Multi-device** - Same user, laptop/desktop/phone, occasional sync
- **Collaboration** - Multiple users editing same vault, real-time
- **Federation** - A2A agents across networks, eventual consistency, untrusted peers

Currently there's no sync - each instance is isolated. This change adds a layered sync architecture that keeps markdown pristine while enabling collaboration.

## Key Insight

**CRDT is for local real-time editing, not server sync.**

```
Server Sync:     Merkle diff + LWW (simple, git-like)
Live Sessions:   CRDT (real-time, desktop/browser only)
```

This separation means:
- No CRDT metadata in files or frontmatter
- Server stores diffs, not operation logs
- CRDT complexity isolated to live collaboration
- Local compute with remote data

```
┌─────────────────────────────────────────────────────────────────┐
│                     Crucible Server                              │
│        (self-hosted, sync + history + auth + sessions)          │
├─────────────────────────────────────────────────────────────────┤
│  /api/sync      Merkle diff, pull/push blocks                   │
│  /api/history   Version history (text diffs + binary LWW)       │
│  /api/auth      Token issuance/verification                     │
│  /api/sessions  WS/SSE for live collaboration                   │
└─────────────────────────────────────────────────────────────────┘
                            ▲
                            │ HTTP + WS/SSE
        ┌───────────────────┼───────────────────┐
        ▼                   ▼                   ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│  CLI Client   │   │ Local Server  │   │ Tauri Desktop │
│  (thin)       │   │ (localhost)   │   │ (embeds all)  │
└───────────────┘   └───────────────┘   └───────────────┘
        │                   │                   │
        ▼                   ▼                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Local Vault                               │
│  notes/*.md           (plaintext, NO artifacts)                 │
│  assets/*             (content-addressed binaries)              │
│  .crucible/                                                     │
│    ├─ sync.db         (hashes, vector clocks, sync state)       │
│    ├─ crdt.db         (local CRDT state for live sessions)      │
│    └─ config.toml     (server URL, auth token)                  │
└─────────────────────────────────────────────────────────────────┘
```

## What Changes

### Two-Layer Architecture

**Layer 1: Server Sync (Merkle + LWW)**
- Batch sync on push/pull
- Plaintext diffs for text, LWW for binaries
- Server keeps full history
- Simple, reliable, git-like
- Works for: multi-device, async collaboration, federation

**Layer 2: Live Sessions (CRDT)**
- Real-time cursors, selections, edits
- Character-level merge during session
- Only when users request collaboration
- Desktop/browser only (not CLI)
- On session end: winner writes markdown, syncs to server

### Sync Modes

| Mode | Server | CRDT Sessions | Use Case |
|------|--------|---------------|----------|
| **Local** | None | None | Single device |
| **LAN** | None (mDNS) | Optional | Home network |
| **Remote** | Required | Optional | Team collaboration |
| **Federated** | Multiple | None | A2A agents |

### Content Strategies

| Content | Server Sync | Live Session |
|---------|-------------|--------------|
| Text blocks | Plaintext diff + vector clock | Loro/Yjs CRDT |
| Frontmatter | LWW (timestamp) | LWW |
| Tags | OR-Set | OR-Set |
| Binaries | Content-addressed + LWW | N/A (not editable) |

### Merkle Sync Protocol (Layer 1)

```
1. Exchange Merkle root hashes
2. If roots match → already synced, done
3. If roots differ → walk tree to find divergent blocks
4. Exchange plaintext diffs for changed blocks
5. Apply LWW merge (vector clock decides winner)
6. Rebuild Merkle tree from merged state
7. Repeat until roots match
```

### Live Session Protocol (Layer 2)

```
1. User A requests session on note via server
2. Server creates session, notifies other online users
3. User B joins session
4. Both load note, initialize local CRDT from markdown
5. Edits broadcast via WS/SSE, CRDT merge locally
6. On session end (or timeout):
   - Last editor's state becomes canonical
   - Written back to markdown
   - Synced to server via Layer 1
```

### Crucible Server

Self-hosted server (inspired by [Oxen](https://github.com/Oxen-AI/Oxen)):

| Endpoint | Purpose |
|----------|---------|
| `/api/sync` | Merkle diff, pull/push blocks |
| `/api/history` | Version history for notes and binaries |
| `/api/auth` | Token issuance (per-user) |
| `/api/sessions` | WS/SSE for live collaboration |

**Auth model:** Token-based (like Oxen). User runs:
```bash
cru auth add-user --email user@example.com
# Generates token, user stores in ~/.crucible/auth.toml
```

### Storage

**Local vault (`.crucible/`):**
- `sync.db` - Hashes, vector clocks, last sync state
- `crdt.db` - CRDT state only during live sessions (temporary)
- `config.toml` - Server URL, auth token

**Server:**
- Full history of all synced changes
- User tracking (who changed what)
- Binary deduplication (content-addressed)

**No overhead in markdown files.** All sync state external.

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
