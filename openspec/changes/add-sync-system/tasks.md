# Implementation Tasks

## Phase 1: CRDT Foundation

- [ ] 1.1 Add `loro` dependency to workspace
- [ ] 1.2 Create `crucible-sync` crate structure
- [ ] 1.3 Implement `CrdtBlock` wrapper around Loro document
- [ ] 1.4 Implement LWW-Register for frontmatter fields
- [ ] 1.5 Implement OR-Set for tags
- [ ] 1.6 Add CRDT metadata to block storage schema
- [ ] 1.7 Unit tests for all CRDT types

## Phase 2: Merkle-CRDT Protocol

- [ ] 2.1 Define `SyncMessage` enum (RootHash, TreeDiff, Operations, Ack)
- [ ] 2.2 Implement Merkle root comparison
- [ ] 2.3 Implement tree traversal to find divergent blocks
- [ ] 2.4 Implement operation exchange for divergent blocks
- [ ] 2.5 Implement merge and tree rebuild
- [ ] 2.6 Add protocol state machine
- [ ] 2.7 Integration tests with two in-memory peers

## Phase 3: Local Transport

- [ ] 3.1 Implement shared folder transport (watch + sync)
- [ ] 3.2 Implement mDNS discovery for local network
- [ ] 3.3 Implement manual peer addition
- [ ] 3.4 Add sync trigger on file change
- [ ] 3.5 Integration tests for local sync

## Phase 4: CLI Commands

- [ ] 4.1 Add `cru sync` subcommand group
- [ ] 4.2 Implement `cru sync status`
- [ ] 4.3 Implement `cru sync now`
- [ ] 4.4 Implement `cru sync add-peer`
- [ ] 4.5 Implement `cru sync config`
- [ ] 4.6 Add sync mode to vault config

## Phase 5: Compaction

- [ ] 5.1 Implement snapshot creation
- [ ] 5.2 Implement operation log truncation
- [ ] 5.3 Implement tombstone garbage collection
- [ ] 5.4 Add configurable compaction threshold
- [ ] 5.5 Add scheduled compaction

## Phase 6: WebSocket Transport (Collaboration)

- [ ] 6.1 Implement WebSocket client transport
- [ ] 6.2 Implement reconnection with exponential backoff
- [ ] 6.3 Add presence/awareness protocol
- [ ] 6.4 Implement cursor position broadcasting
- [ ] 6.5 Integration tests with mock coordinator

## Phase 7: Coordinator Server

- [ ] 7.1 Create `crucible-coordinator` binary
- [ ] 7.2 Implement `/discover` endpoint
- [ ] 7.3 Implement `/presence` WebSocket endpoint
- [ ] 7.4 Implement `/relay` for NAT traversal
- [ ] 7.5 Implement `/auth` for token issuance
- [ ] 7.6 Add coordinator to example-config
- [ ] 7.7 Integration tests with real coordinator

## Phase 8: Capability Tokens

- [ ] 8.1 Define capability token schema
- [ ] 8.2 Implement token signing
- [ ] 8.3 Implement token verification
- [ ] 8.4 Add path-based restrictions
- [ ] 8.5 Implement token revocation list
- [ ] 8.6 Add `cru sync share` command
- [ ] 8.7 Add `cru sync revoke` command

## Phase 9: BFT-CRDT (Federation)

- [ ] 9.1 Add ed25519 signing to operations
- [ ] 9.2 Implement signature verification
- [ ] 9.3 Add causal dependency tracking
- [ ] 9.4 Implement Byzantine peer detection
- [ ] 9.5 Add audit logging for rejected operations
- [ ] 9.6 Integration tests with malicious peer simulation

## Phase 10: Libp2p Transport (Federation)

- [ ] 10.1 Add libp2p dependency
- [ ] 10.2 Implement DHT discovery
- [ ] 10.3 Implement gossip protocol transport
- [ ] 10.4 Add bootstrap node configuration
- [ ] 10.5 Implement peer reputation tracking
- [ ] 10.6 Integration tests with multiple federated peers

## Future TODOs (not this change)

- Streaming sync for large vaults
- Selective sync (folder/tag filters)
- E2E encryption for federated mode
- Coordinator clustering (HA)
- Mobile-specific optimizations
- Conflict visualization UI (time-travel)
- Yjs bridge for real-time collab MVP
