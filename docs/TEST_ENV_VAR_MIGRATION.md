# Test Environment Variable Migration Guide

## Overview

This guide documents the migration from environment variables to configuration structs in tests to eliminate race conditions during parallel test execution.

## Problem

Tests were using `std::env::set_var()` to configure components, which caused race conditions when tests ran in parallel. Multiple tests modifying the same environment variables simultaneously led to flaky tests and unpredictable behavior.

## Solution

Replace environment variable usage with configuration structs that are:
- **Isolated**: Each test gets its own configuration instance
- **Explicit**: Configuration is passed directly to components
- **Thread-safe**: No global mutable state

## New Test Infrastructure

### Available Test Utilities (`crucible-config/src/test_utils.rs`)

#### `TestConfigBuilder`
Build test configurations fluently:

```rust
use crucible_config::{TestConfigBuilder, TestConfig};

let config = TestConfigBuilder::new()
    .profile("test")
    .kiln_path("/tmp/test-kiln")
    .mock_ollama_embedding()
    .memory_database()
    .debug_logging()
    .build();
```

#### `TestConfig` - Pre-configured Configs

```rust
// Minimal configuration
let config = TestConfig::minimal();

// With Ollama provider
let config = TestConfig::with_ollama();

// With temporary kiln
let (config, _temp_dir) = TestConfig::with_temp_kiln();

// With custom kiln path
let config = TestConfig::with_kiln_path("/path/to/kiln");
```

#### `TestKiln` - Temporary Kiln Management

```rust
use crucible_config::TestKiln;

// Create temp kiln with default files
let (temp_dir, kiln_path) = TestKiln::create_temp_kiln();

// Create temp kiln with custom files
let files = vec![
    ("notes.md", "# My Notes\n\nContent here"),
    ("subfolder/doc.md", "# Document\n\nMore content"),
];
let (temp_dir, kiln_path) = TestKiln::create_temp_kiln_with_files(files);
```

## Migration Patterns

### Pattern 1: CLI Command Tests

**Before:**
```rust
async fn run_cli_command(args: Vec<&str>, env_vars: Vec<(&str, &str)>) -> Result<String> {
    let mut cmd = Command::new(binary_path);
    for (key, value) in env_vars {
        cmd.env(key, value);
    }
    // ...
}

#[tokio::test]
async fn test_search() -> Result<()> {
    let vault_dir = create_test_vault().await?;
    let result = run_cli_command(
        vec!["search", "query"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())],
    ).await?;
    // assertions...
}
```

**After:**
```rust
use crucible_config::Config;

async fn run_cli_command(args: Vec<&str>, config: &Config) -> Result<String> {
    let mut cmd = Command::new(binary_path);

    // Extract config values and pass via CLI args
    if let Some(kiln_path) = config.kiln_path_opt() {
        cmd.arg("--kiln-path").arg(kiln_path);
    }

    for arg in args {
        cmd.arg(arg);
    }
    // ...
}

#[tokio::test]
async fn test_search() -> Result<()> {
    let (config, _kiln_dir) = create_test_kiln_with_config().await?;
    let result = run_cli_command(
        vec!["search", "query"],
        &config,
    ).await?;
    // assertions...
}

async fn create_test_kiln_with_config() -> Result<(Config, TempDir)> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create kiln files...
    std::fs::create_dir_all(temp_dir.path().join(".obsidian"))?;

    // Create config
    let config = TestConfig::with_kiln_path(kiln_path);

    Ok((config, temp_dir))
}
```

### Pattern 2: Embedding Provider Tests

**Before:**
```rust
#[tokio::test]
async fn test_embedding_provider() -> Result<()> {
    std::env::set_var("EMBEDDING_PROVIDER", "ollama");
    std::env::set_var("EMBEDDING_MODEL", "nomic-embed-text");
    std::env::set_var("EMBEDDING_ENDPOINT", "http://localhost:11434");

    let provider = create_provider_from_env().await?;
    // test provider...
}
```

**After:**
```rust
#[tokio::test]
async fn test_embedding_provider() -> Result<()> {
    use crucible_config::{TestConfigBuilder, EmbeddingProviderConfig};

    let config = TestConfigBuilder::new()
        .ollama_embedding("http://localhost:11434", "nomic-embed-text")
        .build();

    let provider_config = config.embedding_provider()?;
    let provider = create_provider(provider_config).await?;
    // test provider...
}
```

### Pattern 3: Daemon/Component Tests

**Before:**
```rust
fn setup_test_env() {
    std::env::set_var("EMBEDDING_MODEL", "nomic-embed-text");
    std::env::set_var("EMBEDDING_ENDPOINT", "http://localhost:11434");
}

#[tokio::test]
async fn test_daemon() -> Result<()> {
    setup_test_env();
    // test daemon...
}
```

**After:**
```rust
struct TestEmbeddingConfig {
    model: String,
    endpoint: String,
}

impl Default for TestEmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "nomic-embed-text".to_string(),
            endpoint: "http://localhost:11434".to_string(),
        }
    }
}

#[tokio::test]
async fn test_daemon() -> Result<()> {
    let config = TestEmbeddingConfig::default();
    // Pass config to daemon components...
}
```

### Pattern 4: Real Kiln Validation Tests

**Before:**
```rust
fn get_vault_path() -> Option<PathBuf> {
    match std::env::var("CRUCIBLE_TEST_VAULT") {
        Ok(val) if val == "1" => dirs::home_dir().map(|h| h.join("Documents/crucible-testing")),
        Ok(val) => Some(PathBuf::from(val)),
        _ => None,
    }
}

#[tokio::test]
#[ignore]
async fn test_real_vault() -> Result<()> {
    let Some(vault_path) = get_vault_path() else {
        println!("Test skipped - set CRUCIBLE_TEST_VAULT");
        return Ok(());
    };
    // test with vault_path...
}
```

**After:**
```rust
struct KilnTestConfig {
    kiln_path: PathBuf,
}

impl KilnTestConfig {
    fn from_env() -> Option<Self> {
        match std::env::var("CRUCIBLE_TEST_VAULT") {
            Ok(val) if val == "1" => {
                dirs::home_dir().map(|home| Self {
                    kiln_path: home.join("Documents/crucible-testing"),
                })
            }
            Ok(val) if !val.is_empty() => {
                Some(Self {
                    kiln_path: PathBuf::from(val),
                })
            }
            _ => None,
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_real_kiln() -> Result<()> {
    let Some(config) = KilnTestConfig::from_env() else {
        println!("Test skipped - set CRUCIBLE_TEST_VAULT");
        return Ok(());
    };
    // test with config.kiln_path...
}
```

## Environment Variables to Keep

Some environment variables are acceptable and should be kept:

1. **`CARGO_MANIFEST_DIR`** - Build-time constant, not a race condition source
2. **`OPENAI_API_KEY`** - External opt-in for real API tests (read-only check)
3. **`CRUCIBLE_TEST_VAULT`** - Opt-in for ignored real kiln validation tests

Pattern for opt-in tests with external dependencies:

```rust
#[tokio::test]
#[ignore] // Must be explicitly run
async fn test_with_real_api() -> Result<()> {
    // Check for API key but don't set it
    if std::env::var("OPENAI_API_KEY").is_err() {
        println!("Skipping - set OPENAI_API_KEY to run");
        return Ok(());
    }

    let config = TestConfigBuilder::new()
        .embedding_provider(EmbeddingProviderConfig::openai(
            std::env::var("OPENAI_API_KEY").unwrap(),
            Some("text-embedding-3-small".to_string()),
        ))
        .build();

    // test with real API...
}
```

## Migration Checklist

When migrating a test file:

- [ ] Add `use crucible_config::{Config, TestConfig, TestConfigBuilder};`
- [ ] Replace `std::env::set_var()` calls with config struct creation
- [ ] Update helper functions to accept `&Config` or config structs
- [ ] Replace `env::var()` reads with config.get() or struct fields
- [ ] Change "vault" terminology to "kiln"
- [ ] Keep temp directories alive for test duration
- [ ] Update test names if they reference "vault"

## Files Updated

### Completed
- ✅ `crates/crucible-config/src/test_utils.rs` - New test infrastructure
- ✅ `crates/crucible-config/src/config.rs` - Added `kiln_path()` methods
- ✅ `crates/crucible-daemon/tests/vault_validation.rs` - Config struct pattern
- ✅ `crates/crucible-daemon/tests/unified_event_flow_test.rs` - Config struct
- ✅ `crates/crucible-daemon/tests/utils/embedding_helpers.rs` - Params instead of env vars

### Needs Migration
- ⏳ `crates/crucible-cli/tests/cli_integration_tests.rs` - Partially updated (helpers done)
- ⏳ `crates/crucible-surrealdb/tests/embedding_provider_integration_tests.rs`
- ⏳ `crates/crucible-llm/tests/candle_factory_integration_tests.rs`
- ⏳ `crates/crucible-config/tests/embedding_config_tests.rs`
- ⏳ Other test files (see `grep -r "std::env::set_var" --include="*.rs" tests/`)

## Testing

After migration, verify:

```bash
# Run tests in parallel (default)
cargo test

# Run specific test file
cargo test -p crucible-cli --test cli_integration_tests

# Run ignored tests (like real kiln validation)
cargo test -- --ignored

# Run with verbose output
cargo test -- --nocapture
```

## Benefits

1. **No Race Conditions**: Each test has isolated configuration
2. **Better Parallelism**: Tests can run truly in parallel
3. **Explicit Dependencies**: Configuration requirements are clear
4. **Easier Debugging**: No hidden global state
5. **Type Safety**: Compile-time checking of configuration

## Next Steps

1. Continue migrating remaining test files using patterns above
2. Remove `std::env::set_var` usage from active (non-archived) tests
3. Update CI/CD to benefit from parallel test execution
4. Consider adding test-specific configuration validation
