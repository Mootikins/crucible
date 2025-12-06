# Implementation Tasks

## Phase 1: Local Foundation (No Network)

- [ ] 1.1 Create `crucible-sync` crate structure
- [ ] 1.2 Implement block hash tracking in `.crucible/sync.db`
- [ ] 1.3 Implement vector clock per block
- [ ] 1.4 Detect local changes since last sync (hash compare)
- [ ] 1.5 Add `cru sync status` command (local state only)
- [ ] 1.6 Unit tests for hash tracking and change detection

## Phase 2: Local HTTP Server + LAN Sync

- [ ] 2.1 Create `crucible-api` crate (local HTTP server)
- [ ] 2.2 Implement `/api/vault` endpoint (list notes, metadata)
- [ ] 2.3 Implement `/api/note/:id` endpoint (read/write)
- [ ] 2.4 Serve static UI at localhost (placeholder)
- [ ] 2.5 Implement mDNS discovery for LAN peers
- [ ] 2.6 Implement Merkle diff exchange over HTTP
- [ ] 2.7 Add `cru sync lan` command
- [ ] 2.8 Integration tests with two local instances

## Phase 3: Crucible Server + Remote Sync

- [ ] 3.1 Create `crucible-server` crate
- [ ] 3.2 Implement `/api/sync` - Merkle diff endpoint
- [ ] 3.3 Implement `/api/history` - Version history storage
- [ ] 3.4 Implement `/api/auth` - Token issuance
- [ ] 3.5 Add `cru auth add-user` command
- [ ] 3.6 Add `cru sync remote` command
- [ ] 3.7 Implement binary content-addressed storage
- [ ] 3.8 Implement LWW merge for binaries
- [ ] 3.9 Add server deployment docs
- [ ] 3.10 Integration tests with real server

## Phase 4: Live Sessions (CRDT)

- [ ] 4.1 Create `crucible-crdt` crate
- [ ] 4.2 Add Yjs or Loro dependency
- [ ] 4.3 Implement `/api/sessions` - WS/SSE endpoint
- [ ] 4.4 Implement session join/leave protocol
- [ ] 4.5 Implement CRDT initialization from markdown
- [ ] 4.6 Implement real-time edit broadcasting
- [ ] 4.7 Implement cursor/selection awareness
- [ ] 4.8 Implement session end → markdown write
- [ ] 4.9 Add session timeout handling
- [ ] 4.10 Integration tests with multiple users

## Phase 5: Desktop Integration

- [ ] 5.1 Create `crucible-desktop` Tauri app
- [ ] 5.2 Embed local HTTP server
- [ ] 5.3 Build web UI for note editing
- [ ] 5.4 Integrate CRDT for live editing
- [ ] 5.5 Show sync status in UI
- [ ] 5.6 Show collaboration cursors
- [ ] 5.7 Package for macOS/Linux/Windows

## Phase 6: Federation (A2A)

- [ ] 6.1 Implement multi-server sync
- [ ] 6.2 Add capability tokens for partial sharing
- [ ] 6.3 Implement BFT signatures for untrusted peers
- [ ] 6.4 Add libp2p for decentralized discovery
- [ ] 6.5 Integration tests with federated setup

## Future TODOs (not this change)

- E2E encryption for all sync
- Mobile sync (battery/bandwidth optimizations)
- Conflict visualization UI (time-travel)
- Selective sync (folder/tag filters)
- Coordinator clustering (HA)
- Browser-only client (WASM)

## Crate Structure

```
crates/
├── crucible-core/       # Parsing, storage, merkle (Phase 1)
├── crucible-sync/       # Sync primitives, diff/patch (Phase 1-2)
├── crucible-api/        # Local HTTP server (Phase 2)
├── crucible-server/     # Remote server (Phase 3)
├── crucible-crdt/       # CRDT for live sessions (Phase 4)
├── crucible-cli/        # CLI client (all phases)
└── crucible-config/     # Config management (all phases)

packages/
└── crucible-desktop/    # Tauri app (Phase 5)
```

## Progressive Rollout Summary

| Phase | What Works | UI |
|-------|------------|-----|
| 1 | Local vault, change detection | CLI |
| 2 | LAN sync, local web access | CLI + browser |
| 3 | Remote sync, auth, history | CLI + browser |
| 4 | Real-time collaboration | CLI + browser |
| 5 | Desktop app | Tauri |
| 6 | Federation | All |
