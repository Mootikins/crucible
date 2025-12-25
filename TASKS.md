# Environment Variable Refactoring

**Goal:** Remove `CRUCIBLE_*` config override env vars, keep only API keys and system vars.

**Principle:** Config file → CLI flags → Defaults. Env vars only for secrets.

**Status:** Phase 2 complete, verification in progress.

---

## Phase 1: Deprecation Warnings - SKIPPED

Skipped per user request. Direct removal with sufficient test coverage.

## Phase 2: Remove Config Override Env Vars - COMPLETE

### 2.1 Core Config (crucible-config)

- [x] **2.1.1 Delete apply_env_overrides() from config.rs**
  - Removed entire method (89 lines)
  - Removed call site in load methods

- [x] **2.1.2 Delete apply_env_overrides() from loader.rs**
  - Removed entire function (101 lines)
  - Removed `load_with_env_overrides()` and sync variant

- [x] **2.1.3 Clean up loader.rs method signatures**
  - Removed env override methods entirely

### 2.2 Test Infrastructure (crucible-config)

- [x] **2.2.1 Refactor test_utils.rs**
  - Removed `TestEnv` struct and implementation
  - Removed `test_env_overrides()` test
  - Fixed ConfigLoader import

- [x] **2.2.2 Delete env_override_tests.rs**
  - Deleted entire file (234 lines)

- [x] **2.2.3 Update remaining tests**
  - Removed 302 lines from config_command_tests.rs
  - Removed 87 lines from crucible-cli/src/config.rs tests

### 2.3 Storage Factory

- [x] **2.3.1 Deprecate create_from_env()**
  - Added `#[deprecated(note = "Use create_from_config() instead")]`

### 2.4 CLI Help Text

- [x] **2.4.1 Remove env var references from help**
  - Updated cli.rs help text
  - Updated mcp.rs help text

## Phase 3: Verification - IN PROGRESS

- [x] **3.1 Run full test suite**
  - 1,135 tests pass
  - `cargo test --workspace --exclude crucible-web --lib --tests`

- [ ] **3.2 Create migration documentation**
  - Document in docs/Help/Config/

- [ ] **3.3 Update CHANGELOG**
  - Document breaking change

---

## Summary of Changes

| File | Lines Removed |
|------|---------------|
| `crucible-config/src/config.rs` | 89 |
| `crucible-config/src/loader.rs` | 101 |
| `crucible-config/src/test_utils.rs` | 71 |
| `crucible-cli/src/config.rs` | 87 |
| `crucible-cli/tests/config_command_tests.rs` | 302 |
| `crucible-cli/tests/env_override_tests.rs` | 234 (deleted) |
| **Total** | **884 lines removed** |

---

## Env Var Taxonomy

### KEEP (Secrets)
| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | OpenAI authentication |
| `ANTHROPIC_API_KEY` | Anthropic authentication |
| `OLLAMA_HOST` | Ollama server (their standard) |

### KEEP (System)
| Variable | Purpose |
|----------|---------|
| `CRUCIBLE_CONFIG` | Path to config file |
| `CRUCIBLE_CONFIG_DIR` | Override config search directory |
| `CRUCIBLE_TEST_MODE` | Enable test DB isolation |
| `CRUCIBLE_DAEMON_SOCKET` | Daemon socket path |
| `CRUCIBLE_DAEMON_PID` | Daemon PID file path |

### REMOVED (Config Overrides)
| Variable | Replacement |
|----------|-------------|
| `CRUCIBLE_KILN_PATH` | `kiln_path` in config.toml or `--kiln-path` |
| `CRUCIBLE_EMBEDDING_URL` | `[embedding] api_url` |
| `CRUCIBLE_EMBEDDING_MODEL` | `[embedding] model` |
| `CRUCIBLE_EMBEDDING_PROVIDER` | `[embedding] provider` |
| `CRUCIBLE_EMBEDDING_MAX_CONCURRENT` | `[embedding] max_concurrent` |
| `CRUCIBLE_DATABASE_URL` | `[database] url` |
| `CRUCIBLE_SERVER_HOST` | `[server] host` |
| `CRUCIBLE_SERVER_PORT` | `[server] port` |
| `CRUCIBLE_LOG_LEVEL` | `--log-level` flag |
| `CRUCIBLE_PROFILE` | `profile` in config.toml |

---

## Decisions Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-12-25 | Skip deprecation, direct removal | Sufficient test coverage |
| 2025-12-25 | Remove CRUCIBLE_* config overrides | CLI flags preferred, config file is source of truth |
| 2025-12-25 | Keep API key env vars | Secrets must not be in config files |
| 2025-12-25 | Add CRUCIBLE_CONFIG | Single pointer to config file for containers |
