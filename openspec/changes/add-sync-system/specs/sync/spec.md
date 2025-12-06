# Sync System Specification

## Overview

This specification defines Crucible's sync system - a Merkle-CRDT architecture enabling conflict-free synchronization across devices, collaborators, and federated agents.

## ADDED Requirements

### Requirement: Sync Localities

The system SHALL support three sync modes (localities) with unified core protocol.

#### Scenario: Configure local sync
- **GIVEN** user has Crucible on multiple devices
- **WHEN** user configures `sync.mode = "local"` in vault config
- **THEN** system SHALL sync via local transport (shared folder, mDNS)
- **AND** no coordinator server SHALL be required
- **AND** all peers SHALL be treated as fully trusted

#### Scenario: Configure coordinated sync
- **GIVEN** team wants real-time collaboration
- **WHEN** user configures `sync.mode = "coordinated"` with coordinator URL
- **THEN** system SHALL connect to coordinator for discovery
- **AND** system SHALL sync via WebSocket transport
- **AND** peers SHALL authenticate via capability tokens

#### Scenario: Configure federated sync
- **GIVEN** agents on different networks need to share knowledge
- **WHEN** user configures `sync.mode = "federated"`
- **THEN** system SHALL use DHT or bootstrap nodes for discovery
- **AND** system SHALL use gossip protocol for sync
- **AND** all operations SHALL be cryptographically signed

### Requirement: Merkle-CRDT Sync Protocol

The system SHALL sync by comparing Merkle roots and exchanging CRDT operations for divergent blocks only.

#### Scenario: Sync when roots match
- **GIVEN** two peers with same Merkle root hash
- **WHEN** sync is initiated
- **THEN** system SHALL compare roots
- **AND** system SHALL determine no sync needed
- **AND** sync SHALL complete immediately

#### Scenario: Sync divergent blocks
- **GIVEN** two peers with different Merkle roots
- **WHEN** sync is initiated
- **THEN** system SHALL walk Merkle trees to find divergent blocks
- **AND** system SHALL exchange CRDT operations for those blocks only
- **AND** system SHALL NOT transfer unchanged blocks
- **AND** bandwidth SHALL be proportional to changes, not vault size

#### Scenario: Merge concurrent edits
- **GIVEN** two peers edited same block concurrently
- **WHEN** CRDT operations are exchanged
- **THEN** system SHALL merge operations automatically
- **AND** merge SHALL be deterministic (same result on both peers)
- **AND** no user conflict resolution SHALL be required
- **AND** merged content SHALL preserve both edits

### Requirement: CRDT Types per Content

The system SHALL use appropriate CRDT types for different content.

#### Scenario: Text block editing
- **GIVEN** text content in a block
- **WHEN** concurrent edits occur
- **THEN** system SHALL use Loro (Fugue algorithm) for merging
- **AND** interleaving SHALL be minimized
- **AND** character-level operations SHALL be preserved

#### Scenario: Frontmatter editing
- **GIVEN** YAML frontmatter in a note
- **WHEN** concurrent edits to same field
- **THEN** system SHALL use LWW-Register (last-write-wins)
- **AND** timestamp with peer ID SHALL break ties
- **AND** user SHALL be notified of overwrites

#### Scenario: Tag editing
- **GIVEN** tags on a note
- **WHEN** concurrent tag additions
- **THEN** system SHALL use OR-Set (add-wins)
- **AND** adding same tag on both peers SHALL result in one tag
- **AND** remove + add conflict SHALL result in tag present

### Requirement: Coordinator Server

The system SHALL provide optional coordinator for collaboration mode.

#### Scenario: Peer discovery via coordinator
- **GIVEN** coordinator running at configured URL
- **WHEN** client connects with vault ID
- **THEN** coordinator SHALL return list of online peers for that vault
- **AND** client SHALL receive updates when peers join/leave

#### Scenario: Presence awareness
- **GIVEN** multiple users editing same vault
- **WHEN** user moves cursor or selects text
- **THEN** coordinator SHALL broadcast cursor position to peers
- **AND** peers SHALL display other users' cursors
- **AND** presence SHALL update within 100ms

#### Scenario: NAT traversal via relay
- **GIVEN** two peers behind NAT
- **WHEN** direct P2P connection fails
- **THEN** coordinator SHALL relay WebSocket traffic
- **AND** sync SHALL work through relay
- **AND** relay SHALL be transparent to sync protocol

#### Scenario: Coordinator offline
- **GIVEN** coordinator becomes unavailable
- **WHEN** client attempts sync
- **THEN** client SHALL use cached peer list
- **AND** client SHALL attempt direct P2P with known peers
- **AND** sync SHALL continue without coordinator if peers reachable

### Requirement: Capability-Based Access Control

The system SHALL use capability tokens for access control in coordinated/federated modes.

#### Scenario: Issue capability token
- **GIVEN** vault owner wants to share with collaborator
- **WHEN** owner creates capability
- **THEN** system SHALL generate signed token
- **AND** token SHALL specify: permissions (read/write), paths, expiry
- **AND** token SHALL be self-contained (verifiable without coordinator)

#### Scenario: Verify capability on sync
- **GIVEN** peer presents capability token
- **WHEN** sync request is received
- **THEN** system SHALL verify token signature
- **AND** system SHALL check token not expired
- **AND** system SHALL enforce path restrictions
- **AND** invalid token SHALL reject sync

#### Scenario: Revoke capability
- **GIVEN** capability token was issued
- **WHEN** owner revokes capability
- **THEN** coordinator SHALL add token to revocation list
- **AND** peers SHALL check revocation on next coordinator contact
- **AND** revoked token SHALL be rejected

### Requirement: BFT-CRDT for Federation

The system SHALL support Byzantine Fault Tolerant CRDTs for untrusted peers.

#### Scenario: Sign all operations
- **GIVEN** federated sync mode enabled
- **WHEN** local edit creates CRDT operation
- **THEN** system SHALL sign operation with agent key
- **AND** signature SHALL cover operation content and causal dependencies

#### Scenario: Verify operation signatures
- **GIVEN** receiving CRDT operation from peer
- **WHEN** applying operation
- **THEN** system SHALL verify signature
- **AND** invalid signature SHALL reject operation
- **AND** system SHALL log rejected operations for audit

#### Scenario: Tolerate Byzantine peers
- **GIVEN** malicious peer sends invalid operations
- **WHEN** operations fail verification
- **THEN** system SHALL reject invalid operations
- **AND** valid operations from other peers SHALL still apply
- **AND** system state SHALL remain consistent

### Requirement: Storage and Compaction

The system SHALL manage CRDT storage overhead with compaction.

#### Scenario: Store CRDT metadata
- **GIVEN** block with CRDT operations
- **WHEN** block is persisted
- **THEN** system SHALL store CRDT metadata alongside content
- **AND** metadata SHALL include operation log and vector clock
- **AND** storage overhead SHALL be ~30-40% of content size

#### Scenario: Compact operation log
- **GIVEN** operation log exceeds configured threshold
- **WHEN** compaction runs
- **THEN** system SHALL create snapshot of current state
- **AND** system SHALL discard operations before snapshot
- **AND** snapshot SHALL be sufficient for future merges

#### Scenario: Garbage collect tombstones
- **GIVEN** deleted content has tombstones
- **WHEN** all peers have synced past deletion
- **THEN** system SHALL remove tombstones
- **AND** storage SHALL be reclaimed
- **AND** late-joining peers SHALL receive compacted state

### Requirement: CLI Commands

The system SHALL provide CLI commands for sync operations.

#### Scenario: Check sync status
- **GIVEN** vault with sync configured
- **WHEN** user runs `cru sync status`
- **THEN** system SHALL display current mode and peers
- **AND** system SHALL show pending changes count
- **AND** system SHALL show last sync timestamp

#### Scenario: Force sync
- **GIVEN** vault with sync configured
- **WHEN** user runs `cru sync now`
- **THEN** system SHALL initiate sync with all known peers
- **AND** system SHALL display progress
- **AND** system SHALL report conflicts (if any metadata overwrites)

#### Scenario: Add peer manually
- **GIVEN** local sync mode
- **WHEN** user runs `cru sync add-peer <address>`
- **THEN** system SHALL add peer to known peers list
- **AND** system SHALL attempt initial sync
- **AND** system SHALL persist peer for future syncs

## CHANGED Requirements

(None - this is a new system)

## REMOVED Requirements

(None - no existing functionality removed)

## Dependencies

### Internal Dependencies
- `storage` - CRDT metadata stored alongside blocks
- `agents` - Agent keys used for BFT signatures
- `apis` - Coordinator uses HTTP/WebSocket

### External Dependencies
- `loro = "1.0"` - CRDT implementation
- `libp2p` - P2P networking (federation)
- `tokio-tungstenite` - WebSocket (collaboration)
- `ed25519-dalek` - Signatures (BFT)

## Open Questions

1. **Yjs for MVP?** - Use battle-tested Yjs for real-time initially, migrate to Loro?
2. **Coordinator persistence** - Just discovery, or also sync hub for offline?
3. **Mobile constraints** - Defer mobile-specific optimizations to Phase 2?
4. **History retention** - Configurable per vault? Default 30 days?
