# Deployment Tiers Specification

## Overview

**Status**: DRAFT - Future architecture planning

Crucible supports multiple deployment configurations to scale from personal offline use to enterprise-grade real-time collaboration. This spec defines the deployment tiers, their backend configurations, and client transport strategies.

## Design Principles

1. **Same Core, Different Backends**: Crucible Core logic (parser, agents, workflows) remains identical across tiers
2. **Trait Abstraction**: Storage backends implement the same `Storage` trait interface
3. **Progressive Enhancement**: Higher tiers add capabilities without breaking lower-tier features
4. **Client-Appropriate Transport**: Desktop apps use optimal protocols; web uses firewall-friendly SSE

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                       Crucible Core                              │
│           (Parser, Agents, Workflows, Graph Queries)             │
└─────────────────────────────────────────────────────────────────┘
                                │
                     Storage Trait Abstraction
                                │
        ┌───────────────┬───────┴───────┬───────────────┐
        ▼               ▼               ▼               ▼
   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────────┐
   │  Local  │    │  Light  │    │  Team   │    │ Enterprise  │
   │SurrealDB│    │SurrealDB│    │SurrealDB│    │ SurrealDB   │
   │+RocksDB │    │+SurrealKV│   │ Server  │    │ + TiKV/     │
   │embedded │    │or Memory│    │ remote  │    │ FoundationDB│
   └─────────┘    └─────────┘    └─────────┘    └─────────────┘
```

## Deployment Tiers

### Tier 1: Local (Personal Vault)

**Use Case**: Single user, offline-first, maximum privacy

| Aspect | Configuration |
|--------|---------------|
| Backend | SurrealDB + RocksDB (embedded) |
| Connection | `rocksdb://./data/kiln.db` |
| Real-time | Local filesystem watching |
| Auth | None (local only) |
| Sync | Manual export/import or future Merkle-CRDT |

**Characteristics**:
- Zero network dependencies
- Full feature set available offline
- Data never leaves device
- Best performance for single-user workloads

### Tier 2: Light (Self-Hosted Simple)

**Use Case**: Single user or small team, lightweight deployment

| Aspect | Configuration |
|--------|---------------|
| Backend | SurrealDB + SurrealKV or Memory |
| Connection | `surrealkv://./data/kiln.db` or `mem://` |
| Real-time | WebSocket (internal) |
| Auth | Optional basic auth |
| Sync | LiteStream for backup/replication |

**Characteristics**:
- Lighter resource footprint than RocksDB
- Suitable for edge deployments, Raspberry Pi, containers
- Can run ephemerally with external backup
- Good for development and testing

### Tier 3: Team (Collaborative Server)

**Use Case**: Team collaboration, shared knowledge base

| Aspect | Configuration |
|--------|---------------|
| Backend | SurrealDB Server (remote) |
| Connection | `ws://server:8000` or `wss://server:8000` |
| Real-time | LIVE SELECT over WebSocket |
| Auth | SurrealDB authentication (Root, Namespace, Database, Scope) |
| Sync | Built-in via SurrealDB server |

**Characteristics**:
- Multi-user concurrent access
- Centralized data with real-time sync
- Role-based access control possible
- WebSocket-based live queries

### Tier 4: Enterprise (Global Scale)

**Use Case**: Large organizations, global distribution, high availability

| Aspect | Configuration |
|--------|---------------|
| Backend | SurrealDB + TiKV cluster or FoundationDB |
| Connection | `tikv://pd1:2379,pd2:2379,pd3:2379` |
| Real-time | LIVE SELECT with replication |
| Auth | Enterprise SSO integration |
| Sync | Automatic via distributed storage |

**Characteristics**:
- Horizontal scaling to 100+ TB
- Multi-region replication
- High availability (no single point of failure)
- Compliance-ready (SOC2, HIPAA possible)

## Alternative Backend: SpacetimeDB

For use cases requiring real-time multiplayer collaboration (Figma-style):

| Aspect | SpacetimeDB |
|--------|-------------|
| Backend | SpacetimeDB (Rust + WASM modules) |
| Connection | Native SpacetimeDB protocol |
| Real-time | Automatic table subscriptions, sub-ms sync |
| Auth | Built-in |
| Best For | GPUI desktop with live collaboration |

**When to Consider SpacetimeDB**:
- Real-time cursors and presence
- Sub-millisecond sync requirements
- ECS-style data patterns (similar to Crucible's EAV)
- Reducers (transactional event handlers) for workflows

**Trade-offs**:
- Less mature than SurrealDB
- No graph query language (would need application-layer graph)
- BSL 1.1 license (same as SurrealDB)

## Client Transport Matrix

| Client Type | Local | Light/Team | Enterprise | SpacetimeDB |
|-------------|-------|------------|------------|-------------|
| **GPUI Desktop** | Direct embed | WebSocket | WebSocket | Native SDK |
| **Web Browser** | N/A | SSE (via Axum) | SSE (via Axum) | WebSocket |
| **CLI** | Direct embed | WebSocket | WebSocket | N/A |
| **Mobile (future)** | N/A | SSE or REST | SSE or REST | Native SDK |

## Web Transport: SSE Bridge

Web clients use Server-Sent Events for firewall-friendly real-time updates:

```
┌─────────────┐     WebSocket      ┌──────────────────┐     SSE      ┌─────────────┐
│  SurrealDB  │◄──────────────────►│  Crucible Server │─────────────►│ Web Browser │
│  (any tier) │   LIVE SELECT      │  (Axum/Actix)    │              │             │
└─────────────┘                    └──────────────────┘              └─────────────┘
```

**Why SSE for Web**:
- Not blocked by corporate firewalls/WAFs
- No HTTP 101 upgrade required
- Simple `EventSource` API in browsers
- Crucible server controls transformation/filtering
- Works with any SurrealDB backend tier

**Implementation** (Axum):
```rust
use axum::response::sse::{Event, KeepAlive, Sse};

async fn notes_sse(State(db): State<SurrealClient>) -> Sse<impl Stream<...>> {
    let live = db.live_select("notes").await?;
    let stream = live.map(|change| Event::default().json_data(&change));
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

## Configuration

### Backend Selection

Current approach uses path-based detection. Future approach uses explicit backend enum:

```rust
pub enum SurrealBackend {
    /// In-memory (ephemeral, testing)
    Memory,
    /// RocksDB file-based (Tier 1 default)
    RocksDb { path: String },
    /// SurrealKV - lighter than RocksDB (Tier 2)
    SurrealKV { path: String },
    /// Remote SurrealDB server (Tier 3)
    Remote { url: String, credentials: Option<Credentials> },
    /// TiKV distributed cluster (Tier 4)
    TiKV { endpoints: Vec<String> },
}

pub struct SurrealDbConfig {
    pub namespace: String,
    pub database: String,
    pub backend: SurrealBackend,
    pub max_connections: Option<u32>,
    pub timeout_seconds: Option<u32>,
}
```

### Feature Flags

```toml
[features]
default = ["backend-rocksdb"]
backend-rocksdb = ["surrealdb/kv-rocksdb"]
backend-surrealkv = ["surrealdb/kv-surrealkv"]
backend-tikv = ["surrealdb/kv-tikv"]
backend-remote = ["surrealdb/protocol-ws"]
all-backends = ["backend-rocksdb", "backend-surrealkv", "backend-tikv", "backend-remote"]
```

## Migration Path

### Phase 1: Current State
- SurrealDB + RocksDB embedded only
- Path-based backend detection (`:memory:` vs file path)

### Phase 2: Multi-Backend Support
- Add `SurrealBackend` enum to configuration
- Update `SurrealClient::new()` to match on backend type
- Add feature flags for optional backends
- Maintain backward compatibility via `from_legacy_path()`

### Phase 3: Web Server Layer
- Add Axum/Actix HTTP server crate
- Implement SSE bridge for LIVE SELECT
- REST endpoints for CRUD operations

### Phase 4: SpacetimeDB Option (Future)
- Evaluate SpacetimeDB for GPUI real-time collaboration
- Potentially dual-backend: SurrealDB for storage, SpacetimeDB for sync

## References

- [SurrealDB Architecture](https://surrealdb.com/docs/surrealdb/introduction/architecture)
- [SurrealDB Storage Considerations](https://surrealdb.com/learn/fundamentals/performance/deployment-storage)
- [SurrealKV Deep Dive](https://ori-cohen.medium.com/surrealkv-diving-deep-with-the-new-storage-engine-in-surrealdb-2-0-5c8d276aaaf6)
- [SpacetimeDB](https://spacetimedb.com/)
- [TrailBase](https://github.com/trailbaseio/trailbase) - Alternative Rust backend (OSL 3.0)
- [Axum SSE](https://docs.rs/axum/latest/axum/response/sse/)
