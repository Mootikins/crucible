# Daemon Consolidation Plan

> **Status:** Implemented
> **Supersedes:** [[2024-12-31-single-binary-db-daemon]] (partially implemented, diverged)

## Problem Statement

The daemon architecture diverged during implementation, resulting in:

1. **Two server implementations:**
   - `cru-server` binary (`crucible-daemon` crate) - full features: sessions, agents, kilns
   - `cru db-server` subcommand (`crucible-cli`) - partial: kilns, notes, search only

2. **Two socket paths:**
   - `$XDG_RUNTIME_DIR/crucible/daemon.sock` (cru-server)
   - `$XDG_RUNTIME_DIR/crucible-db.sock` (db-server)

3. **Conflicting client code:**
   - `DaemonClient::start_daemon()` spawns `cru-server` binary
   - `lifecycle::fork_daemon()` forks self as `db-server` subcommand

## Decision

**Consolidate to single-binary pattern:**
- Remove `cru-server` binary
- Move all functionality into `cru db-server` subcommand
- Single socket path: `$XDG_RUNTIME_DIR/crucible.sock`
- Auto-fork on demand, idle timeout shutdown

## Architecture (Target State)

```
┌────────────────────────────────────────────────────────────┐
│                    cru (single binary)                      │
├────────────────────────────────────────────────────────────┤
│                                                             │
│  CLI Commands          Hidden Subcommand                    │
│  ┌─────────────┐       ┌──────────────────────────────┐    │
│  │ cru chat    │       │ cru db-server                │    │
│  │ cru search  │       │ (auto-forked on demand)      │    │
│  │ cru process │       │                              │    │
│  └──────┬──────┘       │ ┌──────────────────────────┐ │    │
│         │              │ │ Unix Socket Server       │ │    │
│         │              │ │ • JSON-RPC 2.0           │ │    │
│         ▼              │ │ • Idle watchdog          │ │    │
│  ┌──────────────┐      │ └──────────────────────────┘ │    │
│  │DaemonClient  │◄────►│ ┌──────────────────────────┐ │    │
│  │ (connects or │      │ │ Managers:                │ │    │
│  │  forks self) │      │ │ • KilnManager            │ │    │
│  └──────────────┘      │ │ • SessionManager         │ │    │
│                        │ │ • AgentManager           │ │    │
│                        │ │ • SubscriptionManager    │ │    │
│                        │ └──────────────────────────┘ │    │
│                        └──────────────────────────────┘    │
└────────────────────────────────────────────────────────────┘
```

## Implementation Tasks

### Phase 1: Unify Socket Path

**Files to modify:**
- `crates/crucible-daemon/src/lifecycle.rs` - canonical `socket_path()`
- `crates/crucible-daemon-client/src/lifecycle.rs` - use canonical path
- `crates/crucible-cli/src/commands/db_server.rs` - use canonical path

**Change:**
```rust
// Single source of truth in crucible-daemon/src/lifecycle.rs
pub fn socket_path() -> PathBuf {
    std::env::var("CRUCIBLE_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::runtime_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("crucible.sock")
        })
}
```

### Phase 2: Merge RPC Handlers

Move session/agent RPC handlers from `crucible-daemon/src/server.rs` into `crucible-cli/src/commands/db_server.rs`.

**RPC methods to add to db-server:**
- `session.create`, `session.list`, `session.get`
- `session.pause`, `session.resume`, `session.end`
- `session.subscribe`, `session.unsubscribe`
- `session.configure_agent`, `session.send_message`, `session.cancel`

**Dependencies to add to crucible-cli:**
- `crucible-daemon` (for SessionManager, AgentManager, SubscriptionManager)

### Phase 3: Update Client

**File:** `crates/crucible-daemon-client/src/client.rs`

Change `start_daemon()` to fork self:
```rust
async fn start_daemon() -> Result<()> {
    use crate::lifecycle::{fork_daemon, socket_path};
    
    let socket = socket_path();
    let idle_timeout = 300; // 5 minutes default
    
    fork_daemon(&socket, idle_timeout)
}
```

### Phase 4: Remove cru-server Binary

1. Delete `crates/crucible-daemon/src/main.rs`
2. Update `crates/crucible-daemon/Cargo.toml` - remove `[[bin]]` section
3. Keep crate as library (managers, protocol types)
4. Update `crates/crucible-daemon/tests/` - use db-server instead

### Phase 5: Update Documentation

- Update `AGENTS.md` with daemon architecture section
- Archive `2024-12-31-single-binary-db-daemon.md` (superseded)
- Update any docs referencing `cru-server`

## Migration Path

**For existing users:**
1. Old `cru-server` processes will be orphaned (manual kill)
2. New `cru db-server` will auto-start on next command
3. Socket path changes, so old clients won't connect

**Cleanup command:**
```bash
pkill -f 'cru-server'
rm -f $XDG_RUNTIME_DIR/crucible/daemon.sock
rm -f $XDG_RUNTIME_DIR/crucible-db.sock
```

## Success Criteria

- [ ] Single socket path used everywhere
- [ ] `cru db-server` has full session/agent support
- [ ] `cru-server` binary removed
- [ ] `DaemonClient::connect_or_start()` forks self as `db-server`
- [ ] All daemon tests pass using db-server
- [ ] AGENTS.md documents the architecture
- [ ] No dead code warnings

## Open Questions

1. **Idle timeout config:** Should it be configurable via config file, or just CLI arg?
   - **Decision:** CLI arg for db-server, config file for clients calling ensure_daemon()

2. **Session persistence location:** Currently sessions stored in kiln. Keep or move to central location?
   - **Decision:** Keep in kiln (`.crucible/sessions/`) for now

## Related Documents

- [[../Analysis/2026-01-10 Dioxus Unified Binary Architecture]] - future GUI extension
- [[../../Help/Configuration]] - storage mode configuration
