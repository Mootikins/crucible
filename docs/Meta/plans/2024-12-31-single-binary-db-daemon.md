# Single-Binary DB Daemon - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable multi-session kiln access via a forked DB daemon, while keeping single-binary simplicity and backward-compatible embedded mode.

**Architecture:** Single `cru` binary runs as either CLI client or DB server (hidden subcommand). Config toggles between in-process embedded DB vs daemon-backed DB with auto-fork on first connect.

**Tech Stack:** Rust, tokio, daemonize crate, Unix sockets, JSON-RPC, SurrealDB embedded

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                     cru (single binary)                  │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  storage.mode = "embedded"     storage.mode = "daemon"   │
│  ┌──────────────────────┐      ┌──────────────────────┐  │
│  │ Direct SurrealDB     │      │ Check socket exists? │  │
│  │ (single process)     │      │   ├─ No: fork self   │  │
│  │                      │      │   │   as db-server   │  │
│  │                      │      │   └─ Yes: connect    │  │
│  └──────────────────────┘      └──────────────────────┘  │
│                                          │               │
│                                          ▼               │
│                                ┌──────────────────────┐  │
│                                │ cru db-server        │  │
│                                │ (hidden subcommand)  │  │
│                                │ ┌──────────────────┐ │  │
│                                │ │ SurrealDB        │ │  │
│                                │ │ Unix socket      │ │  │
│                                │ │ Idle watchdog    │ │  │
│                                │ └──────────────────┘ │  │
│                                └──────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Current State

```
cru chat → create_surrealdb_storage() → Direct SurrealDB (file-locked)
         → No multi-session support
         → Daemon exists but not integrated
```

## Target State

```
cru chat → get_storage(config) → if mode == "embedded"
                                    → Direct SurrealDB
                                 else (mode == "daemon")
                                    → ensure_daemon_running()
                                    → DaemonStorageClient
```

---

## Task 1: Add storage mode config

**Files:**
- Create: `crates/crucible-config/src/components/storage.rs`
- Modify: `crates/crucible-config/src/components/mod.rs`
- Modify: `crates/crucible-config/src/config.rs`

**Step 1: Write failing tests**

```rust
// In storage.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_mode_default_is_embedded() {
        let config = StorageConfig::default();
        assert_eq!(config.mode, StorageMode::Embedded);
    }

    #[test]
    fn test_storage_mode_deserialize_daemon() {
        let toml = r#"
            mode = "daemon"
            idle_timeout_secs = 300
        "#;
        let config: StorageConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.mode, StorageMode::Daemon);
        assert_eq!(config.idle_timeout_secs, 300);
    }

    #[test]
    fn test_storage_mode_deserialize_embedded() {
        let toml = r#"mode = "embedded""#;
        let config: StorageConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.mode, StorageMode::Embedded);
    }
}
```

**Step 2: Implement StorageConfig**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// Direct in-process SurrealDB (single session, file-locked)
    #[default]
    Embedded,
    /// Daemon-backed SurrealDB (multi-session via Unix socket)
    Daemon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Storage mode: "embedded" (default) or "daemon"
    #[serde(default)]
    pub mode: StorageMode,

    /// Idle timeout in seconds before daemon auto-shuts down (daemon mode only)
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
}

fn default_idle_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            mode: StorageMode::Embedded,
            idle_timeout_secs: default_idle_timeout(),
        }
    }
}
```

**Step 3: Wire into Config struct**

Add `pub storage: Option<StorageConfig>` to `CliAppConfig`.

**Step 4: Run tests, commit**

```bash
cargo test -p crucible-config storage
git add -A && git commit -m "feat(config): add storage mode config (embedded vs daemon)"
```

---

## Task 2: Add hidden db-server subcommand

**Files:**
- Modify: `crates/crucible-cli/src/main.rs` (add DbServer variant)
- Create: `crates/crucible-cli/src/commands/db_server.rs`
- Modify: `crates/crucible-cli/src/commands/mod.rs`

**Step 1: Add hidden subcommand to CLI**

```rust
// In commands enum
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...

    /// Internal: Run as database server (auto-started by daemon mode)
    #[command(hide = true)]
    DbServer(DbServerArgs),
}

#[derive(Args)]
pub struct DbServerArgs {
    /// Socket path (defaults to XDG runtime dir)
    #[arg(long)]
    socket: Option<PathBuf>,

    /// Idle timeout in seconds
    #[arg(long, default_value = "300")]
    idle_timeout: u64,
}
```

**Step 2: Write basic db_server command**

```rust
// db_server.rs
pub async fn run(args: DbServerArgs) -> Result<()> {
    // For now, just a stub that proves the command exists
    tracing::info!("db-server starting (socket: {:?})", args.socket);

    // TODO: Implement in Task 4
    Ok(())
}
```

**Step 3: Run and verify**

```bash
cargo build -p crucible-cli
./target/debug/cru db-server --help  # Should work
./target/debug/cru --help  # db-server should NOT appear (hidden)
```

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(cli): add hidden db-server subcommand"
```

---

## Task 3: Implement daemon lifecycle (fork + connect)

**Files:**
- Create: `crates/crucible-daemon-client/src/lifecycle.rs`
- Modify: `crates/crucible-daemon-client/src/lib.rs`
- Modify: `crates/crucible-daemon-client/Cargo.toml` (add daemonize)

**Step 1: Add daemonize dependency**

```toml
[dependencies]
daemonize = "0.5"
```

**Step 2: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_socket_path_uses_xdg_runtime() {
        let path = default_socket_path();
        // Should be in XDG_RUNTIME_DIR or /tmp
        assert!(path.to_string_lossy().contains("crucible")
            || path.to_string_lossy().contains("cru"));
    }

    #[tokio::test]
    async fn test_is_daemon_running_false_when_no_socket() {
        let tmp = TempDir::new().unwrap();
        let socket = tmp.path().join("nonexistent.sock");
        assert!(!is_daemon_running(&socket));
    }
}
```

**Step 3: Implement lifecycle functions**

```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::Result;

/// Get default socket path
pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("crucible-db.sock")
    } else {
        PathBuf::from("/tmp").join(format!("crucible-db-{}.sock",
            std::env::var("USER").unwrap_or_default()))
    }
}

/// Check if daemon is running (socket exists and responds)
pub fn is_daemon_running(socket: &Path) -> bool {
    socket.exists() && {
        // Try a quick connect to verify it's alive
        std::os::unix::net::UnixStream::connect(socket).is_ok()
    }
}

/// Fork self as db-server daemon
pub fn fork_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    let exe = std::env::current_exe()?;

    Command::new(&exe)
        .arg("db-server")
        .arg("--socket")
        .arg(socket)
        .arg("--idle-timeout")
        .arg(idle_timeout.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    Ok(())
}

/// Ensure daemon is running, fork if needed
pub async fn ensure_daemon(socket: &Path, idle_timeout: u64) -> Result<()> {
    if is_daemon_running(socket) {
        return Ok(());
    }

    fork_daemon(socket, idle_timeout)?;

    // Wait for socket with exponential backoff
    let mut delay = std::time::Duration::from_millis(50);
    for attempt in 0..10 {
        tokio::time::sleep(delay).await;
        if is_daemon_running(socket) {
            return Ok(());
        }
        delay = std::cmp::min(delay * 2, std::time::Duration::from_secs(1));
        if attempt > 5 {
            tracing::warn!("Daemon not ready after {} attempts", attempt + 1);
        }
    }

    anyhow::bail!("Failed to start db-server daemon")
}
```

**Step 4: Run tests, commit**

```bash
cargo test -p crucible-daemon-client lifecycle
git add -A && git commit -m "feat(daemon-client): add lifecycle management (fork, ensure)"
```

---

## Task 4: Implement db-server socket server

**Files:**
- Modify: `crates/crucible-cli/src/commands/db_server.rs`
- Add deps to `crucible-cli/Cargo.toml`: `tokio-graceful-shutdown`

**Step 1: Implement server with idle watchdog**

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::watch;
use tokio_graceful_shutdown::{SubsystemBuilder, SubsystemHandle, Toplevel};

pub async fn run(args: DbServerArgs) -> Result<()> {
    let socket_path = args.socket.unwrap_or_else(default_socket_path);
    let idle_timeout = Duration::from_secs(args.idle_timeout);

    // Remove stale socket
    let _ = std::fs::remove_file(&socket_path);

    // Bind socket
    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!("db-server listening on {:?}", socket_path);

    // Connection counter for idle detection
    let connection_count = Arc::new(AtomicUsize::new(0));
    let (activity_tx, activity_rx) = watch::channel(std::time::Instant::now());

    // Open SurrealDB (shared across connections)
    let db = create_surrealdb_for_daemon().await?;

    Toplevel::new(move |s| async move {
        // Spawn connection acceptor
        s.start(SubsystemBuilder::new("acceptor", |h| {
            accept_connections(h, listener, db.clone(), connection_count.clone(), activity_tx)
        }));

        // Spawn idle watchdog
        s.start(SubsystemBuilder::new("idle-watchdog", |h| {
            idle_watchdog(h, idle_timeout, connection_count, activity_rx)
        }));
    })
    .catch_signals()
    .handle_shutdown_requests(Duration::from_secs(10))
    .await?;

    // Cleanup
    let _ = std::fs::remove_file(&socket_path);
    tracing::info!("db-server shutdown complete");

    Ok(())
}

async fn idle_watchdog(
    subsys: SubsystemHandle,
    timeout: Duration,
    connections: Arc<AtomicUsize>,
    mut activity: watch::Receiver<std::time::Instant>,
) -> Result<()> {
    loop {
        tokio::select! {
            _ = subsys.on_shutdown_requested() => break,
            _ = tokio::time::sleep(timeout) => {
                let count = connections.load(Ordering::Relaxed);
                let last_activity = *activity.borrow();

                if count == 0 && last_activity.elapsed() >= timeout {
                    tracing::info!("Idle timeout reached, initiating shutdown");
                    subsys.request_global_shutdown();
                    break;
                }
            }
        }
    }
    Ok(())
}
```

**Step 2: Write integration test**

```rust
#[tokio::test]
async fn test_db_server_starts_and_responds_to_ping() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("test.sock");

    // Start server in background
    let socket_clone = socket.clone();
    let server_handle = tokio::spawn(async move {
        run(DbServerArgs {
            socket: Some(socket_clone),
            idle_timeout: 5,
        }).await
    });

    // Wait for socket
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Connect and ping
    let client = DaemonClient::connect_to(&socket).await.unwrap();
    let pong = client.ping().await.unwrap();
    assert_eq!(pong, "pong");

    // Disconnect, wait for idle timeout
    drop(client);
    tokio::time::sleep(Duration::from_secs(6)).await;

    // Server should have shut down
    assert!(server_handle.await.is_ok());
}
```

**Step 3: Commit**

```bash
cargo test -p crucible-cli db_server
git add -A && git commit -m "feat(cli): implement db-server with idle watchdog"
```

---

## Task 5: Unified storage factory

**Files:**
- Modify: `crates/crucible-cli/src/factories/storage.rs`

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn test_get_storage_uses_embedded_by_default() {
    let config = CliAppConfig::default();
    // Should use embedded (direct) storage
    let storage = get_storage(&config).await.unwrap();
    // Verify it's the embedded type (can query immediately)
}

#[tokio::test]
async fn test_get_storage_uses_daemon_when_configured() {
    let mut config = CliAppConfig::default();
    config.storage = Some(StorageConfig {
        mode: StorageMode::Daemon,
        idle_timeout_secs: 60,
    });

    // Should fork daemon and connect
    let storage = get_storage(&config).await.unwrap();
    // Verify queries work
}
```

**Step 2: Implement unified factory**

```rust
use crucible_config::StorageMode;

/// Get storage based on config mode
pub async fn get_storage(config: &CliAppConfig) -> Result<StorageHandle> {
    let storage_config = config.storage.clone().unwrap_or_default();

    match storage_config.mode {
        StorageMode::Embedded => {
            let client = create_surrealdb_storage(config).await?;
            Ok(StorageHandle::Embedded(client))
        }
        StorageMode::Daemon => {
            let socket = default_socket_path();
            ensure_daemon(&socket, storage_config.idle_timeout_secs).await?;

            let client = DaemonClient::connect_to(&socket).await?;
            let kiln_path = config.kiln_path()?;
            let storage = DaemonStorageClient::new(Arc::new(client), kiln_path);
            Ok(StorageHandle::Daemon(Arc::new(storage)))
        }
    }
}

/// Handle for either embedded or daemon storage
pub enum StorageHandle {
    Embedded(SurrealClientHandle),
    Daemon(Arc<DaemonStorageClient>),
}

impl StorageHandle {
    /// Get as StorageClient trait object (for queries)
    pub fn as_query_client(&self) -> Arc<dyn StorageClient> {
        match self {
            StorageHandle::Embedded(h) => {
                // Wrap in adapter that implements StorageClient
                Arc::new(EmbeddedStorageClient::new(h.clone()))
            }
            StorageHandle::Daemon(c) => c.clone(),
        }
    }

    /// Get embedded handle (panics if daemon mode)
    /// Use for operations that need full SurrealClientHandle
    pub fn as_embedded(&self) -> &SurrealClientHandle {
        match self {
            StorageHandle::Embedded(h) => h,
            StorageHandle::Daemon(_) => panic!("Operation requires embedded mode"),
        }
    }
}
```

**Step 3: Commit**

```bash
cargo test -p crucible-cli storage
git add -A && git commit -m "feat(factories): unified storage factory with mode selection"
```

---

## Task 6: Update CLI commands to use unified factory

**Files:**
- Modify: `crates/crucible-cli/src/commands/chat.rs`
- Modify: `crates/crucible-cli/src/commands/process.rs`
- Modify: `crates/crucible-cli/src/commands/stats.rs`
- Others as needed

**Step 1: Find all storage creation points**

```bash
grep -rn "create_surrealdb_storage" crates/crucible-cli/src/commands/
```

**Step 2: Update each command**

For read-heavy commands (stats, search): use `get_storage()` → works with either mode

For write-heavy commands (process):
- If daemon mode, warn that processing should use embedded
- Or add write operations to daemon RPC (future task)

**Step 3: Test manually**

```bash
# Embedded mode (default)
./target/release/cru stats

# Daemon mode
echo '[storage]
mode = "daemon"' >> ~/.config/crucible/config.toml

./target/release/cru stats  # Should fork daemon first time
./target/release/cru stats  # Should reuse daemon
```

**Step 4: Commit**

```bash
git add -A && git commit -m "refactor(commands): use unified storage factory"
```

---

## Task 7: Integration test - multi-session sharing

**Files:**
- Create: `crates/crucible-cli/tests/daemon_multi_session.rs`

**Step 1: Write integration test**

```rust
#[tokio::test]
#[ignore = "requires daemon mode"]
async fn test_two_sessions_share_kiln_via_daemon() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("test.sock");

    // Start daemon manually for test
    let socket_clone = socket.clone();
    let _server = tokio::spawn(async move {
        db_server::run(DbServerArgs {
            socket: Some(socket_clone),
            idle_timeout: 60,
        }).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Session 1: Connect and write
    let client1 = DaemonClient::connect_to(&socket).await.unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();

    let storage1 = DaemonStorageClient::new(Arc::new(client1), kiln.clone());
    storage1.query_raw("CREATE test:1 SET name = 'from session 1'").await.unwrap();

    // Session 2: Connect and read (should see session 1's data)
    let client2 = DaemonClient::connect_to(&socket).await.unwrap();
    let storage2 = DaemonStorageClient::new(Arc::new(client2), kiln);

    let result = storage2.query_raw("SELECT * FROM test").await.unwrap();
    let records: Vec<serde_json::Value> = serde_json::from_value(result).unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["name"], "from session 1");
}
```

**Step 2: Run test, commit**

```bash
cargo test -p crucible-cli daemon_multi_session -- --ignored
git add -A && git commit -m "test: add multi-session daemon integration test"
```

---

## Task 8: Documentation

**Files:**
- Update: `docs/Help/Configuration.md` or create `docs/Help/Storage Modes.md`

**Document:**
- What each mode does
- When to use each:
  - `embedded`: Single user, simple setup, fastest
  - `daemon`: Multi-session, web UI, shared kiln access
- How to configure: `storage.mode = "daemon"`
- Idle timeout behavior
- Troubleshooting (stale sockets, etc.)

---

## Open Questions

1. **Write operations in daemon mode:** Currently `DaemonStorageClient` only supports queries. Should `process` command refuse to run in daemon mode, or should we add write RPC methods?

2. **Kiln management:** With daemon, multiple kilns can be open. Should daemon auto-open kiln on first query, or require explicit `kiln.open` RPC?

3. **Error recovery:** If daemon crashes, should clients auto-restart it, or fail with helpful error?

4. **Logging:** Where should daemon logs go? Separate file? Journald?

---

## Success Criteria

- [ ] Config: `storage.mode = "embedded" | "daemon"`
- [ ] Hidden `db-server` subcommand works standalone
- [ ] Daemon auto-forks on first connect when mode = "daemon"
- [ ] Daemon auto-shuts down after idle timeout
- [ ] Multiple sessions can query same kiln via daemon
- [ ] Embedded mode works exactly as before (no regression)
- [ ] Docs updated

---

## Dependencies

**Crates to add:**
- `daemonize = "0.5"` - for forking
- `tokio-graceful-shutdown = "0.15"` - for clean shutdown with subsystems

**Existing infrastructure to reuse:**
- `DaemonClient` / `DaemonStorageClient` - already implemented
- `crucible-daemon` server code - can be simplified/merged
