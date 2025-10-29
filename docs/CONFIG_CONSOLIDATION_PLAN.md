> **Note:** The `crucible-daemon` crate has been removed; references in this document remain for historical context.

# Configuration Consolidation Plan

**⚠️ DEPRECATED**: This plan is complete. Configuration is now in config files, not environment variables. - Phase 2

**Status:** Planning Document
**Created:** 2025-10-26
**Updated:** 2025-10-26 (Added env var removal)
**Purpose:** Eliminate config fragmentation across Crucible codebase
**Prerequisites:** Phase 1 (test cleanup) must be complete

---

## Executive Summary

Crucible currently has **150 config structs** across 8 crates with **18 duplicate names**. Additionally, **environment variable configuration** causes test flakiness and configuration complexity.

**Problems:**
- Type confusion (which `EmbeddingConfig` to use?)
- Maintenance burden (changes require updates in 3-4 places)
- Architectural ambiguity (no single source of truth)
- Test brittleness (configs drift, tests break)
- **Environment variable pollution** (tests interfere, hard to debug)

**Goals:**
1. Consolidate to ~50 config structs with zero duplicates
2. **Eliminate ALL environment variable configuration**
3. Establish `crucible-config` as the canonical source
4. Use **file-based config only** (YAML/TOML)

---

## Current State Analysis

### Config Distribution by Crate

```
crucible-daemon (removed):  30 config structs (MOST OVER-ENGINEERED)
crucible-cli:     24 config structs
crucible-services: 23 config structs (BEING REMOVED)
crucible-config:  21 config structs (CANONICAL - well designed)
crucible-watch:   24 config structs
crucible-core:    20 config structs
crucible-llm:      4 config structs
crucible-surrealdb: 4 config structs
```

### Critical Duplications

#### EmbeddingConfig (3 versions)
1. **crucible-config::provider::EmbeddingProviderConfig** ⭐ CANONICAL
   - Location: `crates/crucible-config/src/provider.rs:38`
   - Features: Provider type, model config, API config, dimensions
   - Used by: CLI for user-facing config

2. **crucible-llm::embeddings::config::EmbeddingConfig**
   - Location: `crates/crucible-llm/src/embeddings/config.rs`
   - Features: Provider enum, endpoint, model name, batch size
   - Used by: Internal LLM provider implementations
   - **Assessment:** Implementation detail, should use canonical config

3. **crucible-surrealdb::EmbeddingConfig**
   - Location: `crates/crucible-surrealdb/src/embedding_config.rs`
   - Features: Model name, dimensions, provider type
   - Used by: Database embedding storage
   - **Assessment:** Thin wrapper, should use canonical config

#### DatabaseConfig (4 versions)
1. **crucible-config::config::DatabaseConfig** ⭐ CANONICAL
   - Location: `crates/crucible-config/src/config.rs:204`
   - Features: URL, namespace, database, timeout, pool size
   - Used by: CLI configuration

2. **crucible-core::config::DatabaseConfig**
   - Location: `crates/crucible-core/src/config.rs`
   - Features: Connection string, pool config, retry config
   - Used by: Core database abstractions
   - **Assessment:** Duplicates canonical, can be removed

3. **crucible-daemon (removed)::config::DatabaseConfig**
   - Location: `crates/crucible-daemon (removed)/src/config.rs:212`
   - Features: Connection, sync strategies, transactions, indexing, backup
   - **OVER-ENGINEERED:** 200+ lines for features we don't use
   - Used by: Daemon tests (which are being archived)
   - **Assessment:** Remove entirely

4. **crucible-services::config::DatabaseConfig**
   - Location: `crates/crucible-services/src/config/enhanced_config.rs`
   - Used by: Service layer (being removed)
   - **Assessment:** Remove with services refactor

#### PerformanceConfig (4 versions)
1. **crucible-core::config::PerformanceConfig**
   - Features: Thread pool, queue sizes, timeouts

2. **crucible-daemon (removed)::config::PerformanceConfig**
   - Features: Workers, cache, resource limits
   - **OVER-ENGINEERED:** Worker affinity, CPU limits, file descriptor limits

3. **crucible-services::config::PerformanceConfig**
   - Features: Connection pooling, rate limiting

4. **crucible-watch::config::PerformanceConfig**
   - Features: Event processing, batching, debounce

**Assessment:** All have legitimate different concerns, but share common patterns. Need unified `PerformanceConfig` with specialized sections.

#### LoggingConfig (3 versions)
1. **crucible-config::config::LoggingConfig** ⭐ CANONICAL
   - Location: `crates/crucible-config/src/config.rs:297`
   - Features: Level, format, output destination

2. **crucible-core::config::LoggingConfig**
   - Features: Filters, targets, structured logging

3. **crucible-services::config::LoggingConfig**
   - Features: Service-specific logging

**Assessment:** Merge core features into canonical, remove service version

#### CacheConfig (3 versions)
1. **crucible-core::config::CacheConfig**
   - Features: TTL, max size, eviction policy

2. **crucible-daemon (removed)::config::CacheConfig**
   - Features: Cache type (LRU/TTL/Redis), size, TTL, options

3. **crucible-services::config::CacheConfig**
   - Features: Distributed caching

**Assessment:** Create canonical in crucible-config with all features

---

## Phase 2 Consolidation Strategy

### Guiding Principles

1. **Single Source of Truth:** `crucible-config` owns all shared configs
2. **Composition Over Duplication:** Use nested configs, not copies
3. **File-Based Config Only:** No environment variable configuration
4. **Backwards Compatibility:** Provide migration helpers where needed
5. **Type Safety:** Use newtypes to distinguish contexts
6. **Testability:** Configs should have good defaults and builders

### Target Architecture

```
crucible-config/
├── lib.rs                 # Re-exports
├── config.rs              # Main Config struct
├── provider.rs            # EmbeddingProviderConfig (exists)
├── database.rs            # DatabaseConfig (consolidated)
├── performance.rs         # PerformanceConfig (new, consolidated)
├── logging.rs             # LoggingConfig (consolidated)
├── watch.rs               # WatchConfig (consolidated from daemon/watch)
├── caching.rs             # CacheConfig (consolidated)
├── profile.rs             # ProfileConfig (exists)
├── loader.rs              # ConfigLoader (exists)
├── migration.rs           # Migration tools (exists)
└── test_utils.rs          # Test helpers (exists)
```

---

## Detailed Consolidation Steps

### Step 0: Remove Environment Variable Configuration ⭐ NEW

**Current Problem:**
- 4 failing tests due to env var pollution
- Tests interfere with each other in parallel execution
- Hard to debug: `OBSIDIAN_KILN_PATH`, `CRUCIBLE_CHAT_MODEL`, `OLLAMA_ENDPOINT`, etc.
- Configuration is split between files and env vars (confusing)

**Current Env Var Usage:**
```bash
# Find all env var reads
rg "std::env::var|env::var|std::env::set_var" --type rust crates/
```

**Common Env Vars to Remove:**
- `OBSIDIAN_KILN_PATH` → Config file only
- `OBSIDIAN_KILN_PATH` → Deprecated, use KILN
- `CRUCIBLE_CHAT_MODEL` → Config file only
- `CRUCIBLE_TEMPERATURE` → Config file only
- `OLLAMA_ENDPOINT` → Config file only
- `EMBEDDING_ENDPOINT` → Config file only
- `EMBEDDING_MODEL` → Config file only
- `CRUCIBLE_TEST_MODE` → Use cfg(test) instead
- `RUST_LOG` → Keep (standard tracing convention)

**Migration Strategy:**

1. **Audit all env var usage**
   ```bash
   # Find all environment variable reads
   rg "env::var\(\"([^\"]+)\"\)" --type rust -o | sort | uniq
   ```

2. **Update CliConfig::load() to ONLY load from files**
   ```rust
   // OLD (REMOVE):
   pub fn load(...) -> Result<Self> {
       // Load from file
       let mut config = load_from_file()?;

       // Override with env vars ❌ REMOVE THIS
       if let Ok(path) = env::var("OBSIDIAN_KILN_PATH") {
           config.kiln.path = PathBuf::from(path);
       }
       if let Ok(model) = env::var("CRUCIBLE_CHAT_MODEL") {
           config.llm.chat_model = model;
       }
       // ... more overrides ...
   }

   // NEW (FILE ONLY):
   pub fn load(config_path: Option<PathBuf>) -> Result<Self> {
       let config_path = config_path
           .or_else(|| Self::default_config_path())
           .ok_or(ConfigError::NoConfigFound)?;

       let config = Self::from_file(&config_path)?;
       config.validate()?;
       Ok(config)
   }
   ```

3. **Update tests to use builders/test helpers**
   ```rust
   // OLD (env var pollution):
   #[test]
   fn test_config() {
       env::set_var("OBSIDIAN_KILN_PATH", "/tmp/test"); // ❌ Pollutes parallel tests
       let config = CliConfig::load(None, None, None).unwrap();
       assert_eq!(config.kiln.path, PathBuf::from("/tmp/test"));
       env::remove_var("OBSIDIAN_KILN_PATH"); // ❌ May not run if test panics
   }

   // NEW (explicit config):
   #[test]
   fn test_config() {
       let config = CliConfig::builder()
           .kiln_path("/tmp/test")
           .build()
           .unwrap();
       assert_eq!(config.kiln.path, PathBuf::from("/tmp/test"));
   } // ✅ No cleanup needed, no pollution
   ```

4. **Add config builder pattern**
   ```rust
   // crates/crucible-config/src/config.rs
   impl Config {
       pub fn builder() -> ConfigBuilder {
           ConfigBuilder::default()
       }
   }

   #[derive(Default)]
   pub struct ConfigBuilder {
       kiln_path: Option<PathBuf>,
       embedding_provider: Option<EmbeddingProviderConfig>,
       database: Option<DatabaseConfig>,
       // ... other fields
   }

   impl ConfigBuilder {
       pub fn kiln_path(mut self, path: impl Into<PathBuf>) -> Self {
           self.kiln_path = Some(path.into());
           self
       }

       pub fn build(self) -> Result<Config> {
           Ok(Config {
               kiln: KilnConfig {
                   path: self.kiln_path.ok_or(ConfigError::MissingKilnPath)?,
               },
               // ... populate from self with defaults
           })
       }
   }
   ```

5. **Update CLI argument parsing**
   ```rust
   // OLD: Mix of file + env vars
   // NEW: File path OR inline overrides via CLI args

   #[derive(Parser)]
   struct Cli {
       /// Path to config file
       #[arg(long)]
       config: Option<PathBuf>,

       /// Override kiln path
       #[arg(long)]
       kiln_path: Option<PathBuf>,

       /// Override chat model
       #[arg(long)]
       chat_model: Option<String>,
   }

   fn main() {
       let cli = Cli::parse();
       let mut config = CliConfig::load(cli.config)?;

       // Apply CLI overrides (explicit, not env vars)
       if let Some(path) = cli.kiln_path {
           config.kiln.path = path;
       }
       if let Some(model) = cli.chat_model {
           config.llm.chat_model = model;
       }
   }
   ```

**Benefits:**
- ✅ No more test pollution
- ✅ Explicit configuration (easier to debug)
- ✅ All config in one place (file)
- ✅ CLI args can still override (explicit)
- ✅ No env var cleanup needed in tests

**Breaking Changes:**
- Users must migrate env vars to config files
- Provide migration tool: `cru config migrate-env-vars`

**Migration Tool:**
```rust
// crates/crucible-cli/src/commands/config.rs

pub async fn migrate_env_vars() -> Result<()> {
    println!("Checking for environment variables to migrate...");

    let mut config = Config::default();
    let mut found_vars = Vec::new();

    if let Ok(path) = env::var("OBSIDIAN_KILN_PATH") {
        config.kiln.path = PathBuf::from(path);
        found_vars.push(("OBSIDIAN_KILN_PATH", path));
    }

    if let Ok(model) = env::var("CRUCIBLE_CHAT_MODEL") {
        config.llm.chat_model = model.clone();
        found_vars.push(("CRUCIBLE_CHAT_MODEL", model));
    }

    // ... check all env vars

    if found_vars.is_empty() {
        println!("No environment variables found to migrate.");
        return Ok(());
    }

    println!("\nFound {} environment variables:", found_vars.len());
    for (key, value) in &found_vars {
        println!("  {} = {}", key, value);
    }

    let config_path = Config::default_config_path()
        .unwrap_or_else(|| PathBuf::from("~/.config/crucible/config.yaml"));

    println!("\nSaving to: {}", config_path.display());
    config.save(&config_path)?;

    println!("\n✅ Migration complete!");
    println!("\nYou can now remove these environment variables:");
    for (key, _) in &found_vars {
        println!("  unset {}", key);
    }

    Ok(())
}
```

**Estimated Effort:** 4-6 hours
**Impact:** Fixes 4 test failures, improves testability

---

### Step 1: EmbeddingConfig Consolidation

**Current Usage Analysis:**
```bash
# Find all EmbeddingConfig usage
crucible-cli:        Uses crucible-config::EmbeddingProviderConfig
crucible-llm:        Defines own EmbeddingConfig
crucible-surrealdb:  Defines own EmbeddingConfig
crucible-daemon (removed):     Tests use crucible-llm::EmbeddingConfig
```

**Migration Plan:**

1. **Enhance Canonical Config** (`crucible-config::provider::EmbeddingProviderConfig`)
   - Add any missing fields from llm/surrealdb versions
   - Add conversion methods: `to_llm_config()`, `to_db_config()`

2. **Update crucible-llm**
   ```rust
   // OLD: pub struct EmbeddingConfig { ... }
   // NEW: pub type EmbeddingConfig = crucible_config::EmbeddingProviderConfig;

   // Or better - use directly:
   use crucible_config::EmbeddingProviderConfig;

   impl OllamaProvider {
       pub fn new(config: &EmbeddingProviderConfig) -> Self { ... }
   }
   ```

3. **Update crucible-surrealdb**
   ```rust
   // OLD: mod embedding_config;
   // NEW: use crucible_config::EmbeddingProviderConfig;

   // If specific fields needed, use newtype:
   pub struct EmbeddingStorageConfig {
       pub provider: EmbeddingProviderConfig,
       pub storage_specific_field: String,
   }
   ```

4. **Update all imports** (systematic search-replace)
   ```bash
   # Find all usages
   rg "crucible_llm::.*::EmbeddingConfig" --type rust
   rg "crucible_surrealdb::EmbeddingConfig" --type rust

   # Replace with
   crucible_config::EmbeddingProviderConfig
   ```

5. **Remove old definitions**
   - Delete `crucible-llm/src/embeddings/config.rs`
   - Delete `crucible-surrealdb/src/embedding_config.rs`
   - Update Cargo.toml exports

**Estimated Impact:** ~20 files, ~50 lines changed

---

### Step 2: DatabaseConfig Consolidation

**Current Usage Analysis:**
```
crucible-config:     DatabaseConfig (canonical)
crucible-core:       DatabaseConfig (duplicate)
crucible-daemon (removed):     DatabaseConfig (over-engineered)
crucible-services:   DatabaseConfig (being removed)
```

**Migration Plan:**

1. **Audit Canonical Config** (`crucible-config::config::DatabaseConfig`)
   ```rust
   // Current (config.rs:204)
   pub struct DatabaseConfig {
       pub url: String,
       pub namespace: String,
       pub database: String,
       pub timeout_seconds: u64,
       pub pool_size: u32,
   }
   ```

2. **Identify Missing Features**
   - From `crucible-core`: Retry config, connection pooling details
   - From `crucible-daemon (removed)`: Transaction config, indexing config
   - **Decision:** Daemon features are over-engineered, skip them

3. **Enhance Canonical Config**
   ```rust
   // Enhanced version
   pub struct DatabaseConfig {
       // Existing fields
       pub url: String,
       pub namespace: String,
       pub database: String,
       pub timeout_seconds: u64,

       // Add from crucible-core
       pub pool: ConnectionPoolConfig,
       pub retry: RetryConfig,
   }

   pub struct ConnectionPoolConfig {
       pub min_size: u32,
       pub max_size: u32,
       pub idle_timeout_seconds: u64,
   }

   pub struct RetryConfig {
       pub max_attempts: u32,
       pub initial_delay_ms: u64,
       pub max_delay_ms: u64,
   }
   ```

4. **Update crucible-core**
   ```rust
   // OLD: mod config; pub use config::DatabaseConfig;
   // NEW: pub use crucible_config::DatabaseConfig;
   ```

5. **Remove daemon version** (after archiving tests)

6. **Systematic replacement**
   ```bash
   # Find usages
   rg "crucible_core::.*DatabaseConfig" --type rust
   rg "crucible_daemon::.*DatabaseConfig" --type rust

   # Replace with crucible_config::DatabaseConfig
   ```

**Estimated Impact:** ~30 files, ~100 lines changed

---

### Step 3: PerformanceConfig Consolidation

**Challenge:** Each crate has legitimate different performance concerns.

**Solution:** Unified config with specialized sections.

**New Design:**
```rust
// crucible-config/src/performance.rs

/// Unified performance configuration
pub struct PerformanceConfig {
    /// Thread pool configuration
    pub threads: ThreadPoolConfig,

    /// Cache configuration
    pub cache: CacheConfig,

    /// Event processing performance
    pub events: EventPerformanceConfig,

    /// Resource limits
    pub limits: ResourceLimits,
}

pub struct ThreadPoolConfig {
    /// Number of worker threads (None = CPU count)
    pub num_workers: Option<usize>,

    /// Max task queue size
    pub max_queue_size: usize,

    /// Thread stack size
    pub stack_size_kb: Option<usize>,
}

pub struct EventPerformanceConfig {
    /// Max events per batch
    pub max_batch_size: usize,

    /// Batch timeout (ms)
    pub batch_timeout_ms: u64,

    /// Processing parallelism
    pub parallel_handlers: usize,
}

pub struct ResourceLimits {
    /// Max memory usage (bytes)
    pub max_memory_bytes: Option<u64>,

    /// Max open file descriptors
    pub max_file_descriptors: Option<u32>,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            threads: ThreadPoolConfig {
                num_workers: None, // Use num_cpus
                max_queue_size: 10000,
                stack_size_kb: None,
            },
            cache: CacheConfig::default(),
            events: EventPerformanceConfig {
                max_batch_size: 100,
                batch_timeout_ms: 100,
                parallel_handlers: 4,
            },
            limits: ResourceLimits {
                max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
                max_file_descriptors: Some(10000),
            },
        }
    }
}
```

**Migration:**
- `crucible-core` → Use `PerformanceConfig::threads`
- `crucible-watch` → Use `PerformanceConfig::events`
- `crucible-daemon (removed)` → Remove (tests being archived)
- `crucible-services` → Remove (being removed)

**Estimated Impact:** ~15 files, ~80 lines changed

---

### Step 4: DaemonConfig Simplification

**Current Situation:**
- `crucible-daemon (removed)/src/config.rs`: 1200 lines, 30 config structs
- Features: Filesystem watching, database, performance, health, services
- **Reality:** Daemon = `crucible-watch::WatchManager` + background event triggering

**Problem:** Over-engineered for actual use case.

**Solution Options:**

#### Option A: Thin Adapter (RECOMMENDED)
```rust
// crates/crucible-daemon (removed)/src/lib.rs
use crucible_config::Config;
use crucible_watch::{WatchManager, WatchConfig};

pub struct Daemon {
    config: Config,
    watcher: WatchManager,
}

impl Daemon {
    pub async fn new(config: Config) -> Result<Self> {
        // Convert config to watch config
        let watch_config = WatchConfig::from_crucible_config(&config)?;
        let watcher = WatchManager::new(watch_config)?;

        Ok(Self { config, watcher })
    }

    pub async fn start(&mut self) -> Result<()> {
        self.watcher.start().await
    }
}
```

**Changes Required:**
- Delete `crates/crucible-daemon (removed)/src/config.rs` (1200 lines)
- Update `crates/crucible-daemon (removed)/src/lib.rs` to use `crucible-config::Config`
- Update `crates/crucible-daemon (removed)/src/main.rs` to use unified config
- Archive all daemon tests that use `DaemonConfig`

**Estimated Impact:** Delete 1200 lines, add ~50 lines adapter code

#### Option B: Minimal DaemonConfig
```rust
// If we really need daemon-specific config
pub struct DaemonConfig {
    /// Base crucible config
    pub base: crucible_config::Config,

    /// Watch-specific overrides
    pub watch: Option<WatchConfig>,

    /// Health check configuration
    pub health: HealthConfig,
}
```

**Assessment:** Option A is cleaner. Daemon doesn't need separate config.

---

### Step 5: LoggingConfig Consolidation

**Current Versions:**
1. `crucible-config::LoggingConfig` (canonical)
2. `crucible-core::LoggingConfig` (adds filters, targets)
3. `crucible-services::LoggingConfig` (service-specific)

**Migration Plan:**

1. **Enhance Canonical**
   ```rust
   // crucible-config/src/logging.rs (new file)

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LoggingConfig {
       /// Log level (trace, debug, info, warn, error)
       pub level: String,

       /// Log format (json, pretty, compact)
       pub format: LogFormat,

       /// Output destination
       pub output: LogOutput,

       /// Per-module filters
       pub filters: Vec<ModuleFilter>,

       /// Whether to include timestamps
       pub timestamps: bool,

       /// Whether to include thread IDs
       pub thread_ids: bool,
   }

   pub struct ModuleFilter {
       pub module: String,
       pub level: String,
   }

   pub enum LogFormat {
       Json,
       Pretty,
       Compact,
   }

   pub enum LogOutput {
       Stdout,
       Stderr,
       File(PathBuf),
       Rolling { dir: PathBuf, max_files: usize },
   }
   ```

2. **Remove Duplicates**
   - Delete `crucible-core/src/config.rs` LoggingConfig section
   - Remove from crucible-services (being removed anyway)

3. **Update Imports**
   ```bash
   rg "crucible_core::.*LoggingConfig" --type rust
   # Replace with crucible_config::LoggingConfig
   ```

**Estimated Impact:** ~12 files, ~40 lines changed

---

### Step 6: CacheConfig Consolidation

**Current Versions:**
1. `crucible-core::CacheConfig`
2. `crucible-daemon (removed)::CacheConfig`
3. `crucible-services::CacheConfig`

**Migration Plan:**

1. **Create Canonical**
   ```rust
   // crucible-config/src/caching.rs (new file)

   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct CacheConfig {
       /// Cache type
       pub cache_type: CacheType,

       /// Maximum cache size (bytes or entries)
       pub max_size: Option<usize>,

       /// Time-to-live (seconds)
       pub ttl_seconds: Option<u64>,

       /// Eviction policy
       pub eviction: EvictionPolicy,

       /// Cache-specific options
       #[serde(flatten)]
       pub options: HashMap<String, Value>,
   }

   pub enum CacheType {
       /// In-memory LRU cache
       Lru,
       /// In-memory TTL cache
       Ttl,
       /// Redis distributed cache
       Redis { url: String },
       /// Disabled
       None,
   }

   pub enum EvictionPolicy {
       Lru,  // Least Recently Used
       Lfu,  // Least Frequently Used
       Fifo, // First In First Out
       Ttl,  // Time-based
   }
   ```

2. **Remove Duplicates**
   - Update crucible-core to use canonical
   - Remove daemon version (tests archived)
   - Remove services version

**Estimated Impact:** ~8 files, ~30 lines changed

---

## Migration Execution Plan

### Pre-Migration Checklist
- [ ] Phase 1 complete (all tests compile)
- [ ] Git working directory clean
- [ ] Create feature branch: `config-consolidation`
- [ ] Document current config usage with script:
  ```bash
  ./scripts/audit-config-usage.sh > docs/config-usage-before.txt
  ```

### Migration Order (Least Risky → Most Risky)

1. **LoggingConfig** (simple, well-understood)
   - Low risk, high value
   - Few dependencies

2. **CacheConfig** (isolated, not critical path)
   - Low risk, medium value
   - Limited usage

3. **EmbeddingConfig** (widely used, but clear canonical)
   - Medium risk, high value
   - Many usages but straightforward replacement

4. **DatabaseConfig** (core functionality, many usages)
   - Medium risk, high value
   - Test thoroughly

5. **PerformanceConfig** (complex, multi-faceted)
   - High risk, medium value
   - May reveal architectural issues

6. **DaemonConfig** (complete redesign)
   - High risk, high value
   - Requires architectural thinking

### Per-Migration Process

For each config consolidation:

1. **Audit Phase**
   ```bash
   # Find all definitions
   rg "pub struct ${CONFIG_NAME}" --type rust

   # Find all usages
   rg "use.*${CONFIG_NAME}" --type rust
   rg "${CONFIG_NAME}::" --type rust
   ```

2. **Design Phase**
   - Compare all versions side-by-side
   - Identify unique features
   - Design unified schema
   - Write migration plan

3. **Implementation Phase**
   - Create/enhance canonical config
   - Add conversion methods if needed
   - Write tests for canonical config

4. **Migration Phase**
   - Update imports in dependency order (leaves → roots)
   - Run `cargo check` after each file
   - Fix compilation errors immediately

5. **Cleanup Phase**
   - Remove old config definitions
   - Remove unused imports
   - Run `cargo fmt`
   - Run `cargo clippy`

6. **Verification Phase**
   - Run `cargo test --workspace`
   - Check documentation builds: `cargo doc --no-deps`
   - Manual testing of affected features

7. **Commit Phase**
   - Clear commit message with before/after
   - Reference issue/task number
   - Note breaking changes

### Safety Checks

After each migration step:
```bash
# Check compilation
cargo check --workspace

# Check tests
cargo test --workspace --no-run

# Check for unused code
cargo clippy -- -W dead_code

# Check dependencies
cargo tree | grep crucible

# Verify no config duplicates remain
rg "pub struct ${CONFIG_NAME}" --type rust | wc -l
# Should be 1
```

---

## Testing Strategy

### Unit Tests
Each canonical config needs tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingProviderConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = EmbeddingProviderConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EmbeddingProviderConfig =
            serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_config_validation() {
        let mut config = EmbeddingProviderConfig::default();
        config.dimensions = 0; // Invalid
        assert!(config.validate().is_err());
    }
}
```

### Integration Tests
Test config loading from files:
```rust
#[test]
fn test_load_from_yaml() {
    let yaml = r#"
        provider: ollama
        model: nomic-embed-text
        dimensions: 768
    "#;

    let config: EmbeddingProviderConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.model, "nomic-embed-text");
}
```

### Migration Tests
Before/after compatibility:
```rust
#[test]
fn test_migration_from_old_config() {
    let old_json = r#"{ /* old format */ }"#;
    let old: OldEmbeddingConfig = serde_json::from_str(old_json).unwrap();
    let new = EmbeddingProviderConfig::from_old(old);
    assert_eq!(new.validate(), Ok(()));
}
```

---

## Backwards Compatibility

### Deprecation Strategy

For public APIs being removed:
```rust
#[deprecated(
    since = "0.2.0",
    note = "Use crucible_config::EmbeddingProviderConfig instead"
)]
pub type EmbeddingConfig = crucible_config::EmbeddingProviderConfig;
```

### Migration Helpers

```rust
impl From<OldDatabaseConfig> for DatabaseConfig {
    fn from(old: OldDatabaseConfig) -> Self {
        Self {
            url: old.connection_string,
            namespace: old.namespace,
            database: old.database,
            timeout_seconds: old.timeout_seconds,
            pool: ConnectionPoolConfig {
                min_size: old.pool.min_connections,
                max_size: old.pool.max_connections,
                idle_timeout_seconds: old.pool.idle_timeout_seconds,
            },
            retry: RetryConfig::default(),
        }
    }
}
```

### Config File Migration Tool

```rust
// crates/crucible-cli/src/commands/config.rs

pub async fn migrate_config(old_path: PathBuf) -> Result<()> {
    let old_config: OldConfig = load_config(&old_path)?;
    let new_config: Config = old_config.into();

    let new_path = old_path.with_extension("yaml.migrated");
    save_config(&new_path, &new_config)?;

    println!("Migrated config saved to: {}", new_path.display());
    println!("Review and replace old config when ready.");

    Ok(())
}
```

---

## Documentation Updates

### Files to Update

1. **README.md**
   - Update config examples
   - Document new canonical locations

2. **docs/ARCHITECTURE.md**
   - Update config layer diagram
   - Document consolidation rationale

3. **docs/CONFIGURATION.md** (new)
   - Comprehensive config reference
   - All canonical configs documented
   - Migration guide

4. **Cargo.toml**
   - Update crate descriptions
   - Note config consolidation in changelogs

5. **In-code Documentation**
   ```rust
   //! # Configuration System
   //!
   //! Crucible uses a unified configuration system centered on `crucible-config`.
   //!
   //! ## Canonical Configs
   //!
   //! - [`Config`]: Main configuration struct
   //! - [`EmbeddingProviderConfig`]: Embedding provider configuration
   //! - [`DatabaseConfig`]: Database connection configuration
   //! - [`PerformanceConfig`]: Performance tuning configuration
   //!
   //! ## Loading Configuration
   //!
   //! ```rust
   //! use crucible_config::{Config, ConfigLoader};
   //!
   //! let config = ConfigLoader::new()
   //!     .file("config.yaml")
   //!     .env()
   //!     .load()?;
   //! ```
   ```

---

## Rollback Plan

If consolidation causes major issues:

### Immediate Rollback
```bash
# Revert to last good commit
git reset --hard HEAD~1

# Or revert specific files
git checkout HEAD~1 -- crates/crucible-config/src/provider.rs
```

### Partial Rollback
- Keep successful consolidations
- Revert problematic ones
- Document issues for later retry

### Lessons Learned
- Document what went wrong
- Update migration process
- Consider different approach

---

## Success Metrics

### Quantitative Metrics
- [ ] Config struct count: 150 → ~50 (67% reduction)
- [ ] Duplicate config names: 18 → 0 (100% elimination)
- [ ] Lines of config code: ~5000 → ~2000 (60% reduction)
- [ ] Config-related compiler errors: 0 (must not break)
- [ ] Config-related test failures: 0 (must not break)

### Qualitative Metrics
- [ ] Single canonical location for each config concept
- [ ] Clear config ownership (crucible-config)
- [ ] Improved developer experience (less confusion)
- [ ] Better documentation (comprehensive reference)
- [ ] Easier testing (centralized test utils)

---

## Timeline Estimate

**Total Effort:** ~3-4 days (1 developer)

| Step | Effort | Dependencies |
|------|--------|--------------|
| LoggingConfig consolidation | 2 hours | None |
| CacheConfig consolidation | 2 hours | None |
| EmbeddingConfig consolidation | 4 hours | None |
| DatabaseConfig consolidation | 4 hours | EmbeddingConfig |
| PerformanceConfig consolidation | 6 hours | DatabaseConfig |
| DaemonConfig simplification | 8 hours | All above |
| Testing & verification | 4 hours | All above |
| Documentation updates | 4 hours | All above |

---

## Open Questions

1. **Breaking Changes Policy**
   - Is it acceptable to break APIs in this pre-1.0 phase?
   - Should we maintain backwards compat with type aliases?

2. **Feature Parity**
   - Which features from duplicate configs are actually used?
   - Can we safely drop unused features?

3. **Migration Timeline**
   - Should we do all at once or incremental releases?
   - What's the rollout strategy for users?

4. **Config Format**
   - YAML, TOML, JSON - which should be canonical?
   - Support all three with conversion?

5. **Validation**
   - How strict should config validation be?
   - Runtime vs compile-time validation?

---

## References

- Current config analysis: `/tmp/config_analysis.txt`
- Duplicate detection script: `./scripts/find-duplicate-configs.sh`
- Config usage audit: `./docs/config-usage-before.txt` (run pre-migration)
- Related issues: GitHub #TODO
- Architecture docs: `./docs/ARCHITECTURE.md`

---

## Appendix A: Config Inventory

### crucible-config (CANONICAL)
- Config (main)
- DatabaseConfig
- ServerConfig
- LoggingConfig
- ProfileConfig
- EmbeddingProviderConfig
- ChatProviderConfig
- ApiConfig
- ModelConfig
- GenerationConfig
- ConfigLoader
- ConfigLoaderBuilder
- ConfigMigrator
- LegacyConfig variants
- TestConfigBuilder

### crucible-daemon (removed) (TO BE SIMPLIFIED)
- DaemonConfig (1200 lines - REMOVE)
- FilesystemConfig (move to crucible-watch)
- DatabaseConfig (duplicate - REMOVE)
- PerformanceConfig (consolidate)
- HealthConfig (simplify or remove)
- ServicesConfig (remove - services gone)
- 24 other sub-configs (mostly remove)

### crucible-core (REDUCE)
- CrucibleConfig (migrate to canonical)
- DatabaseConfig (duplicate - REMOVE)
- LoggingConfig (duplicate - REMOVE)
- NetworkConfig (evaluate keep/remove)
- PerformanceConfig (consolidate)
- OrchestrationConfig (evaluate)
- ServiceConfig (evaluate)
- CircuitBreakerConfig (keep - specialized)
- SinkConfig (keep - specialized)

### crucible-watch (KEEP SPECIALIZED)
- WatchConfig (specialized - keep)
- WatchManagerConfig (specialized - keep)
- DebounceConfig (specialized - keep)
- FilterConfig (specialized - keep)
- PerformanceConfig (consolidate shared parts)
- MonitoringConfig (evaluate)

### crucible-cli (SIMPLIFY)
- CliConfig (keep - CLI-specific)
- BackendConfigs (keep - CLI-specific)
- ReplConfig (keep - REPL-specific)
- EmbeddingConfig (duplicate - REMOVE, use canonical)
- Remove duplicate OllamaConfig, OpenAIConfig

### crucible-llm (SIMPLIFY)
- EmbeddingConfig (use canonical)
- OllamaConfig (internal - keep or consolidate)
- OpenAIConfig (internal - keep or consolidate)
- CandleConfig (specialized - keep)

### crucible-surrealdb (SIMPLIFY)
- EmbeddingConfig (use canonical)
- SurrealDbConfig (specialized - keep)
- KilnPipelineConfig (evaluate)
- KilnScannerConfig (evaluate)

---

## Appendix B: Code Patterns

### Before Consolidation
```rust
// Three different places defining the same concept

// crates/crucible-cli/src/config.rs
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
}

// crates/crucible-llm/src/embeddings/config.rs
pub struct EmbeddingConfig {
    pub provider: ProviderType,
    pub endpoint: String,
    pub model: String,
}

// crates/crucible-surrealdb/src/embedding_config.rs
pub struct EmbeddingConfig {
    pub model_name: String,
    pub dimensions: usize,
}
```

### After Consolidation
```rust
// Single canonical location

// crates/crucible-config/src/provider.rs
pub struct EmbeddingProviderConfig {
    pub provider: ProviderType,
    pub endpoint: Option<String>,
    pub model: String,
    pub dimensions: usize,
    pub api_config: ApiConfig,
}

// Other crates just use it
use crucible_config::EmbeddingProviderConfig;
```

---

**End of Phase 2 Plan**

This document will be updated as Phase 2 progresses with actual implementation details, lessons learned, and final metrics.
