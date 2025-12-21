# Test Infrastructure Feature Flags

> Layered infrastructure requirements on top of the existing test tier system for OS-agnostic, developer-configurable test execution.

**Date:** 2025-12-20
**Status:** Approved
**Complements:** [Test Consolidation Design](./2025-12-20-test-consolidation-design.md)

## Overview

Add workspace-level feature flags that gate tests requiring specific infrastructure (Ollama, embedding endpoints, developer vaults). This layers on top of the existing Unit/Integration/Contract/Slow tier system.

## Feature Flags

Defined at workspace level in root `Cargo.toml`:

| Feature | Requirements | Env Vars |
|---------|--------------|----------|
| `test-local-kiln` | Developer's personal vault | `CRUCIBLE_KILN_PATH` |
| `test-ollama` | Running Ollama server | `OLLAMA_HOST` (optional) |
| `test-embeddings` | Embedding API endpoint | `EMBEDDING_ENDPOINT`, `EMBEDDING_MODEL` |
| `test-onnx-download` | Network + ~100MB disk | None |

**Not gated** (always available):
- `tempfile::TempDir` — OS-agnostic temp directories
- `examples/test-kiln` — Bundled fixture in repo

## Implementation

### 1. Workspace Features

```toml
# Root Cargo.toml
[workspace.features]
test-local-kiln = []
test-ollama = []
test-embeddings = []
test-onnx-download = []
```

Each crate re-exports relevant features in its own `[features]` section.

### 2. OS-Agnostic Temp Paths

New helper in `crucible-core::test_support`:

```rust
/// Returns a cross-platform path that doesn't exist (for error-handling tests)
pub fn nonexistent_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_nonexistent_{}", name))
}
```

Replace ~39 hardcoded `/tmp` references:
- Error-handling tests → `nonexistent_path("file.md")`
- Tests needing real files → `tempfile::TempDir` (existing pattern)

### 3. Test Migration

Convert ~35 infrastructure-gated `#[ignore]` tests:

```rust
// Before
#[ignore = "Requires Ollama server"]
#[tokio::test]
async fn test_ollama_embeddings() { ... }

// After
#[cfg(feature = "test-ollama")]
#[tokio::test]
async fn test_ollama_embeddings() { ... }
```

**Leave as `#[ignore]`:**
- Unimplemented features ("Requires phonetic matching implementation")
- Flaky/timing tests ("Performance-sensitive test")
- Watch mode tests (require real-time FS monitoring)

### 4. Developer Configuration

Local `.cargo/config.toml`:

```toml
[env]
CRUCIBLE_KILN_PATH = "/home/user/my-vault"
EMBEDDING_ENDPOINT = "https://llama.example.com"
EMBEDDING_MODEL = "nomic-embed-text-v1.5-q8_0"
```

Run infrastructure tests:

```bash
cargo test --features test-ollama
cargo test --features test-embeddings,test-local-kiln
```

### 5. CI Integration

```yaml
jobs:
  test:
    steps:
      - run: cargo nextest run --profile ci
      # No infrastructure features - runs everywhere

  test-with-infrastructure:
    if: github.event_name == 'schedule'  # Nightly
    steps:
      - run: cargo nextest run --features test-onnx-download
```

## Files Changed

| Location | Change |
|----------|--------|
| `Cargo.toml` (root) | Add `[workspace.features]` |
| `crates/*/Cargo.toml` | Re-export relevant features |
| `crucible-core/src/test_support/mod.rs` | Add `nonexistent_path()` |
| ~39 files with `/tmp` | Replace with `temp_dir()` or `TempDir` |
| ~35 infrastructure tests | Replace `#[ignore]` with `#[cfg(feature)]` |
| `AGENTS.md` | Document feature flags in Testing section |

## Success Criteria

1. `cargo test` passes on Windows, macOS, Linux without external services
2. `cargo test --features test-ollama` runs Ollama-dependent tests when available
3. No hardcoded `/tmp` paths remain in test code
4. AGENTS.md documents how to enable infrastructure tests locally
