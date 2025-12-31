# Wire Daemon Into Storage - Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make CLI commands use daemon-backed storage by default, enabling multi-session kiln sharing.

**Architecture:** Add config toggle for daemon mode, unify storage traits, update factory to conditionally create daemon or direct storage.

**Tech Stack:** Rust, Unix sockets, JSON-RPC, existing DaemonClient/DaemonStorageClient

---

## Current State

```
CLI Commands → create_surrealdb_storage() → Direct SurrealDB
                    ↓ (unused)
              create_daemon_storage() → DaemonClient → Daemon → SurrealDB
```

## Target State

```
CLI Commands → create_storage(config) → if config.storage.use_daemon
                                           → DaemonStorageClient
                                        else
                                           → Direct SurrealDB
```

---

## Task 1: Add storage config with daemon toggle

**Files:**
- Create: `crates/crucible-config/src/components/storage.rs`
- Modify: `crates/crucible-config/src/components/mod.rs`
- Modify: `crates/crucible-config/src/config.rs` (add to Config and CliAppConfig)

**Step 1: Write the failing test**

```rust
#[test]
fn test_storage_config_defaults() {
    let config = StorageConfig::default();
    assert!(!config.use_daemon); // default off for backward compat
}

#[test]
fn test_storage_config_deserialize() {
    let toml = r#"
        use_daemon = true
    "#;
    let config: StorageConfig = toml::from_str(toml).unwrap();
    assert!(config.use_daemon);
}
```

**Step 2: Implement StorageConfig**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Use daemon for storage operations (enables multi-session kiln sharing)
    #[serde(default)]
    pub use_daemon: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { use_daemon: false }
    }
}
```

**Step 3: Run tests, commit**

---

## Task 2: Verify DaemonStorageClient API compatibility

**Files:**
- Read: `crates/crucible-daemon-client/src/storage.rs`
- Read: `crates/crucible-surrealdb/src/lib.rs` (direct storage API)

**Step 1: Document current APIs**

Check what methods each provides:
- Direct: `SurrealStorage` - what traits does it implement?
- Daemon: `DaemonStorageClient` - what methods does it have?

**Step 2: Identify gaps**

List any methods in direct storage that daemon client lacks.

**Step 3: Create compatibility trait if needed**

```rust
pub trait KilnStorage: Send + Sync {
    async fn query(&self, query: &str) -> Result<Value>;
    async fn store_note(&self, note: &ParsedNote) -> Result<()>;
    // ... other common methods
}
```

---

## Task 3: Update storage factory with config-based selection

**Files:**
- Modify: `crates/crucible-cli/src/factories/storage.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_create_storage_uses_daemon_when_configured() {
    let mut config = CliAppConfig::default();
    config.storage = Some(StorageConfig { use_daemon: true });

    // This should attempt to connect to daemon
    // We can mock or just verify the code path
}

#[tokio::test]
async fn test_create_storage_uses_direct_when_not_configured() {
    let config = CliAppConfig::default();
    // Should use direct storage
}
```

**Step 2: Implement unified factory**

```rust
pub async fn create_storage(
    config: &CliAppConfig,
    kiln_path: &Path,
) -> Result<Arc<dyn KilnStorage>> {
    let use_daemon = config
        .storage
        .as_ref()
        .map(|s| s.use_daemon)
        .unwrap_or(false);

    if use_daemon {
        Ok(Arc::new(create_daemon_storage(kiln_path).await?))
    } else {
        Ok(Arc::new(create_surrealdb_storage(kiln_path).await?))
    }
}
```

---

## Task 4: Update CLI commands to use unified factory

**Files:**
- Modify: `crates/crucible-cli/src/commands/chat.rs`
- Modify: `crates/crucible-cli/src/commands/process.rs`
- Modify: `crates/crucible-cli/src/commands/stats.rs`
- Modify: other commands as needed

**Step 1: Identify all storage creation points**

```bash
grep -r "create_surrealdb_storage\|create_enriched" crates/crucible-cli/src/commands/
```

**Step 2: Update each to use unified factory**

Replace direct calls with `create_storage(config, kiln_path)`.

---

## Task 5: Integration test - multi-session sharing

**Files:**
- Create: `crates/crucible-cli/tests/daemon_integration.rs`

**Step 1: Write integration test**

```rust
#[tokio::test]
#[ignore = "requires daemon"]
async fn test_two_sessions_share_kiln_via_daemon() {
    // Setup: start daemon, create temp kiln

    // Session 1: store a note
    let storage1 = create_daemon_storage(&kiln).await.unwrap();
    storage1.store_note(&test_note()).await.unwrap();

    // Session 2: read the note (should see it)
    let storage2 = create_daemon_storage(&kiln).await.unwrap();
    let notes = storage2.list_notes().await.unwrap();
    assert_eq!(notes.len(), 1);
}
```

---

## Task 6: Documentation

**Files:**
- Update: `docs/Help/Rules Files.md` (if relevant)
- Create: `docs/Help/Daemon.md`

Document:
- What daemon mode does
- When to use it (multi-session, web UI)
- How to enable: `storage.use_daemon = true`
- Manual control: `cru daemon start/stop/status`

---

## Open Questions

1. **Auto-start:** Should `create_daemon_storage` auto-start daemon? Current impl uses `connect_or_start()` which does this.

2. **Fallback:** What if daemon dies? Reconnect? Fail? Fall back to direct?

3. **Default:** Should daemon be default eventually? Or always opt-in?

---

## Success Criteria

- [ ] Config toggle: `storage.use_daemon = true/false`
- [ ] Factory respects config
- [ ] All CLI commands work with daemon storage
- [ ] Integration test: two sessions share kiln
- [ ] Docs updated
- [ ] No regressions when daemon disabled (default)
