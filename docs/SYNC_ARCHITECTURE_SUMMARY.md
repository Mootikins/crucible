# Crucible Sync Architecture Summary

> Critical evaluation and synthesis of sync design decisions (2025-12-05)

## Core Insight

**Markdown is source of truth. Sync is a protocol, not a storage format.**

No in-file artifacts. Hash on parse. Server keeps history. Local compute, remote data.

## Architecture Overview

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
│    ├─ crdt.db         (local CRDT state for live editing)       │
│    └─ config.toml     (server URL, auth token)                  │
└─────────────────────────────────────────────────────────────────┘
```

## Key Design Decisions

### 1. No In-File Artifacts

| Rejected | Reason |
|----------|--------|
| CRDT metadata in frontmatter | Pollutes user content, 30-40% bloat |
| Sidecar `.crdt` files per note | Clutter, drift risk |
| Full CRDT in markdown | Not "markdown first" anymore |

**Chosen:** External sync state in `.crucible/` directory. Markdown stays pristine.

### 2. Sync Strategy by Content Type

| Content | Strategy | Granularity | Overhead |
|---------|----------|-------------|----------|
| Text (markdown) | Plaintext diff + vector clock | Block-level | Minimal |
| Binaries (images, PDFs) | Content-addressed + LWW | Whole file | None |
| Structure (folders) | OR-Set | Directory | Minimal |

**Rationale:** Block-level is sufficient. Character-level CRDT only matters for real-time collaboration, which happens locally with remote data.

### 3. CRDT Locality

**Critical insight:** CRDT is for **local real-time editing**, not server sync.

```
Server Sync (Merkle + LWW):
  - Batch sync on push/pull
  - Plaintext diffs
  - Server keeps full history
  - Simple, git-like

Local Collaboration (CRDT):
  - Real-time cursors, selections
  - Character-level merge
  - Only when users in same session
  - Desktop/browser only
```

**Flow:**
1. User A and B request collaboration session via server
2. Server establishes WS/SSE channel
3. Users sync CRDT state **directly** (P2P or via server relay)
4. On session end, winner writes to markdown, syncs to server
5. Server stores as regular diff, not CRDT ops

### 4. Progressive Rollout

| Phase | Component | Sync | Collaboration | UI |
|-------|-----------|------|---------------|-----|
| **1** | Core + CLI | Local only | None | CLI |
| **2** | + Local HTTP | + LAN (mDNS) | None | + Browser (localhost) |
| **3** | + Crucible Server | + Remote (token auth) | None | + Tauri desktop |
| **4** | + Session layer | + WS/SSE | Real-time CRDT | + Live cursors |

### 5. Server vs Local Responsibilities

| Responsibility | Server | Local |
|---------------|--------|-------|
| History storage | ✓ Full history | ✗ Current state only |
| Auth | ✓ Token issuance | Token storage |
| Sync protocol | ✓ Merkle exchange | ✓ Merkle exchange |
| CRDT merge | ✗ | ✓ During live sessions |
| Binary dedup | ✓ Content-addressed | ✓ Content-addressed |
| User tracking | ✓ Who changed what | ✗ |

## Comparison with Existing Systems

### vs Git

| Aspect | Git | Crucible |
|--------|-----|----------|
| Commits | Explicit | Automatic on sync |
| Merge conflicts | Manual resolution | LWW (block-level) |
| Binary handling | Poor | Content-addressed + LWW |
| Real-time | No | Yes (CRDT sessions) |
| Self-hosted | Yes | Yes |

### vs Oxen AI

| Aspect | Oxen | Crucible |
|--------|------|----------|
| Focus | Large datasets | Markdown knowledge |
| Commits | Explicit | Automatic |
| Merkle | Custom, optimized | BLAKE3, simpler |
| Real-time | No | Yes (sessions) |
| Inspiration | ✓ Server architecture, token auth |

### vs Any-Sync (Anytype)

| Aspect | Any-Sync | Crucible |
|--------|----------|----------|
| CRDT | Full, everywhere | Local sessions only |
| Encryption | E2E always | Optional (Phase 5+) |
| Nodes | 4 specialized types | 1 server + clients |
| Complexity | High | Lower |
| Inspiration | ✓ Session coordination, CRDT patterns |

## Trade-offs Acknowledged

### Accepted Trade-offs

1. **Block-level, not character-level sync** - Fine for async collaboration. Character-level only during live sessions.

2. **Server required for collaboration** - Local-first for single user, but multi-user needs coordinator. Acceptable.

3. **CRDT complexity isolated to sessions** - Simpler sync, but sessions need CRDT library (Loro/Yjs).

4. **No offline real-time** - If disconnected during session, fall back to last-write-wins on reconnect.

### Rejected Alternatives

1. **Full CRDT everywhere** - 30-40% overhead, complexity doesn't pay off for markdown-first.

2. **Git as sync backend** - Requires git knowledge, explicit commits, poor binary handling.

3. **Frontmatter sync state** - Pollutes user content, defeats "markdown first".

4. **Character-level server sync** - Overkill. Block-level sufficient, character-level only live.

## Open Questions

1. **Yjs vs Loro for sessions?** - Yjs more battle-tested, Loro is Rust-native. Start Yjs, evaluate Loro.

2. **Session persistence?** - If session disconnects mid-edit, how long to hold CRDT state? 5 min timeout?

3. **Conflict notification?** - LWW is silent. Should we notify when block was overwritten by sync?

4. **Mobile sync?** - Different constraints. Defer to Phase 5, focus on desktop/CLI first.

## Implementation Priority

```
Phase 1 (Now):       Core + CLI + local storage
Phase 2 (Next):      Local HTTP server + LAN sync
Phase 3 (Soon):      Crucible Server + remote sync + auth
Phase 4 (Later):     WS/SSE sessions + CRDT for real-time
Phase 5 (Future):    E2E encryption, mobile, federation
```

## Crate Structure

```
crates/
├── crucible-core/       # Parsing, storage, merkle (Phase 1)
├── crucible-sync/       # Sync protocol, diff/patch (Phase 2)
├── crucible-server/     # HTTP API, auth, history, sessions (Phase 3)
├── crucible-crdt/       # CRDT for live sessions (Phase 4)
├── crucible-cli/        # CLI client (Phase 1)
└── crucible-api/        # Local HTTP server for UIs (Phase 2)

packages/
└── crucible-desktop/    # Tauri app (Phase 3)
```

## References

- [Oxen AI](https://github.com/Oxen-AI/Oxen) - Rust, Merkle trees, self-hosted server, token auth
- [Any-Sync](https://github.com/anyproto/any-sync) - CRDT + DAG, session coordination, MIT license
- [Loro](https://loro.dev) - Rust CRDT library, Fugue algorithm
- [Yjs](https://yjs.dev) - Battle-tested CRDT, better for MVP

---

*This document reflects design decisions as of 2025-12-05. Updated as architecture evolves.*
