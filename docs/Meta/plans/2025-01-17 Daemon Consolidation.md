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

**Keep separate daemon binary:**
- Keep `cru-server` binary (full-featured daemon)
- Remove `cru db-server` subcommand (vestigial, incomplete)
- Single socket path: `$XDG_RUNTIME_DIR/crucible.sock`
- `DaemonClient::connect_or_start()` spawns `cru-server`

## Architecture (Target State)

```
┌─────────────────────────────────────────────────────────────┐
│  CLI (cru)                    Daemon (cru-server)           │
│  ┌─────────────┐              ┌──────────────────────────┐  │
│  │ cru chat    │◄────────────►│ Unix Socket Server       │  │
│  │ cru search  │   JSON-RPC   │ ($XDG_RUNTIME_DIR/       │  │
│  │ cru process │              │  crucible.sock)          │  │
│  └─────────────┘              │                          │  │
│                               │ Managers:                │  │
│  storage.mode = "embedded"    │ • KilnManager            │  │
│  → Direct DB access           │ • SessionManager         │  │
│                               │ • AgentManager           │  │
│  storage.mode = "daemon"      │ • SubscriptionManager    │  │
│  → RPC to cru-server          └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Tasks

### Phase 1: Unify Socket Path ✅

**Files modified:**
- `crates/crucible-daemon/src/lifecycle.rs` - canonical `socket_path()`
- `crates/crucible-daemon-client/src/lifecycle.rs` - delegates to canonical path

**Result:**
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

### Phase 2: Remove db-server Subcommand ✅

**Removed:**
- `crates/crucible-cli/src/commands/db_server.rs`
- `DbServer` command variant from CLI
- All references to `db-server` in factories and tests

**Rationale:** The `db-server` was an incomplete subset of `cru-server`. Rather than duplicating all the session/agent functionality, we kept the full-featured daemon.

### Phase 3: Keep cru-server Binary ✅

**Kept as-is:**
- `crates/crucible-daemon/src/main.rs` - daemon entrypoint
- `crates/crucible-daemon/Cargo.toml` - `[[bin]]` section for `cru-server`
- All managers (KilnManager, SessionManager, AgentManager, SubscriptionManager)

**DaemonClient** already spawns `cru-server`:
```rust
async fn start_daemon() -> Result<()> {
    let daemon_exe = if exe.ends_with("cru") {
        exe.parent()?.join("cru-server")
    } else {
        PathBuf::from("cru-server")
    };
    Command::new(daemon_exe).spawn()?;
    Ok(())
}
```

### Phase 4: Update Documentation ✅

- Updated `AGENTS.md` daemon architecture section
- This plan document reflects the actual implementation

## Migration Path

**For existing users:**
1. Old `cru-server` processes at old socket path should be killed
2. New unified socket path is `$XDG_RUNTIME_DIR/crucible.sock`
3. `cru-server` auto-starts via `DaemonClient::connect_or_start()`

**Cleanup command:**
```bash
pkill -f 'cru-server'
rm -f $XDG_RUNTIME_DIR/crucible/daemon.sock  # old path
rm -f $XDG_RUNTIME_DIR/crucible-db.sock      # old path
rm -f $XDG_RUNTIME_DIR/crucible.sock         # new path
```

## Success Criteria

- [x] Single socket path used everywhere (`crucible.sock`)
- [x] `cru db-server` subcommand removed (obsolete)
- [x] `cru-server` binary kept (full-featured)
- [x] `DaemonClient::connect_or_start()` spawns `cru-server`
- [x] All daemon tests pass
- [x] AGENTS.md documents the architecture
- [ ] No dead code warnings (to verify)

## Resolved Questions

1. **Which binary pattern?** Keep `cru-server` as separate binary or merge into `cru db-server`?
   - **Decision:** Keep `cru-server` as separate binary (simpler, full-featured)

2. **Idle timeout config:** Should it be configurable via config file, or just CLI arg?
   - **Decision:** Config file via `storage.idle_timeout_secs`

3. **Session persistence location:** Currently sessions stored in kiln. Keep or move to central location?
   - **Decision:** Keep in kiln (`.crucible/sessions/`) for now

## Related Documents

- [[../Analysis/2026-01-10 Dioxus Unified Binary Architecture]] - future GUI extension
- [[../../Help/Configuration]] - storage mode configuration
