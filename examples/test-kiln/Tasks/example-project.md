---
title: Example Feature Implementation
description: Demonstrates TASKS.md format for the task harness CLI
context_files:
  - src/lib.rs
  - tests/integration.rs
verify: just test
tdd: true
---

## Phase 1: Foundation

### 1.1 Setup

- [x] Create project structure [id:: 1.1.1]
  - Initialize cargo project
  - Add workspace dependencies
  - [tests:: test_project_compiles]

- [x] Add core types [id:: 1.1.2] [deps:: 1.1.1]
  - Define Config struct
  - Add Error type
  - [tests:: test_config_default, test_error_display]

### 1.2 Configuration

- [/] Implement config loading [id:: 1.2.1] [deps:: 1.1.2]
  - Load from TOML file
  - Environment variable overrides
  - [tests:: test_load_from_file, test_env_override]

- [ ] Add config validation [id:: 1.2.2] [deps:: 1.2.1]
  - Validate required fields
  - Range checks for numeric values
  - [tests:: test_validation_missing_field, test_validation_range]

## Phase 2: Core Features

### 2.1 Main Feature

- [ ] Implement feature A [id:: 2.1.1] [deps:: 1.2.2]
  - Core algorithm implementation
  - Error handling
  - [tests:: test_feature_a_basic, test_feature_a_edge_cases]

- [ ] Add feature B [id:: 2.1.2] [deps:: 2.1.1]
  - Build on feature A
  - Add caching layer
  - [tests:: test_feature_b_with_cache]

### 2.2 Integration

- [-] Connect to external API [id:: 2.2.1] [deps:: 2.1.1]
  - Blocked: Waiting for API credentials
  - [tests:: test_api_connection]

- [?] Design data model [id:: 2.2.2]
  - Question: Should we use normalized or denormalized schema?
  - Need input before proceeding

## Phase 3: Polish

### 3.1 Documentation

- [ ] Add API documentation [id:: 3.1.1] [deps:: 2.1.2]
  - Document public functions
  - Add examples

- [ ] Write user guide [id:: 3.1.2] [deps:: 3.1.1]
  - Getting started section
  - Configuration reference

### 3.2 Final Verification

- [w] Await code review [id:: 3.2.1] [deps:: 3.1.2]
  - Waiting on team review

- [!] Fix critical bug [id:: 3.2.2] [priority:: high]
  - Urgent: Memory leak in hot path
  - [tests:: test_no_memory_leak]

- [ ] Release preparation [id:: 3.2.3] [deps:: 3.2.1, 3.2.2]
  - Update CHANGELOG
  - Bump version
  - Create release tag
