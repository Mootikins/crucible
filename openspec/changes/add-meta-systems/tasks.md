# Implementation Tasks

## 1. Plugin Runtime Foundation

### 1.1 Rune Runtime Engine
- [ ] 1.1.1 Create `crates/crucible-plugins/src/runtime/rune.rs` module
- [ ] 1.1.2 Implement `RunePlugin` struct with VM initialization
- [ ] 1.1.3 Add plugin code compilation and validation
- [ ] 1.1.4 Implement function discovery and invocation
- [ ] 1.1.5 Add error handling and detailed error messages
- [ ] 1.1.6 Write unit tests for Rune plugin execution

### 1.2 Security Sandbox
- [ ] 1.2.1 Create `crates/crucible-plugins/src/runtime/sandbox.rs` module
- [ ] 1.2.2 Define `SandboxConfig` with resource limits
- [ ] 1.2.3 Implement memory limit enforcement (default 100MB)
- [ ] 1.2.4 Implement CPU time limit enforcement (default 30s)
- [ ] 1.2.5 Implement filesystem access restrictions
- [ ] 1.2.6 Add network access controls (deny by default)
- [ ] 1.2.7 Implement process isolation (if available on platform)
- [ ] 1.2.8 Write security tests for sandbox escapes

### 1.3 Resource Limits
- [ ] 1.3.1 Create `crates/crucible-plugins/src/runtime/limits.rs` module
- [ ] 1.3.2 Implement `ResourceLimiter` trait
- [ ] 1.3.3 Add memory tracking and enforcement
- [ ] 1.3.4 Add execution timeout with graceful termination
- [ ] 1.3.5 Add rate limiting using governor crate
- [ ] 1.3.6 Implement file size limits (default 10MB)
- [ ] 1.3.7 Add configurable limits per plugin
- [ ] 1.3.8 Write tests for limit enforcement

### 1.4 Plugin Discovery
- [ ] 1.4.1 Create `crates/crucible-plugins/src/discovery.rs` module
- [ ] 1.4.2 Implement plugin scanning in `~/.config/crucible/plugins/`
- [ ] 1.4.3 Implement plugin scanning in `.crucible/plugins/`
- [ ] 1.4.4 Add plugin metadata parsing from frontmatter
- [ ] 1.4.5 Implement plugin validation (syntax, safety checks)
- [ ] 1.4.6 Create plugin registry with HashMap storage
- [ ] 1.4.7 Add plugin override logic (kiln > system)
- [ ] 1.4.8 Handle missing directories gracefully
- [ ] 1.4.9 Write tests for discovery and override logic

## 2. Plugin API and Integration

### 2.1 Core Plugin API
- [ ] 2.1.1 Create `crates/crucible-plugins/src/api.rs` module
- [ ] 2.1.2 Define `PluginContext` with access to kiln operations
- [ ] 2.1.3 Implement `crucible.read_note(path)` API function
- [ ] 2.1.4 Implement `crucible.write_note(path, content)` API function
- [ ] 2.1.5 Implement `crucible.search(query)` semantic search API
- [ ] 2.1.6 Implement `crucible.list_notes(folder)` API function
- [ ] 2.1.7 Add error handling with user-friendly messages
- [ ] 2.1.8 Write API integration tests

### 2.2 Graph API
- [ ] 2.2.1 Implement `crucible.graph.traverse(strategy)` API
- [ ] 2.2.2 Add traversal strategies (BFS, DFS, semantic similarity)
- [ ] 2.2.3 Implement `crucible.graph.backlinks(note)` API
- [ ] 2.2.4 Implement `crucible.graph.forwardlinks(note)` API
- [ ] 2.2.5 Add graph query filtering and limits
- [ ] 2.2.6 Write graph API tests

### 2.3 State Management API
- [ ] 2.3.1 Implement `crucible.state.get(key)` persistent storage API
- [ ] 2.3.2 Implement `crucible.state.set(key, value)` API
- [ ] 2.3.3 Add state scoping (global, kiln-specific, plugin-specific)
- [ ] 2.3.4 Implement state serialization (JSON)
- [ ] 2.3.5 Add state expiration and cleanup
- [ ] 2.3.6 Write state management tests

### 2.4 Tool Registration
- [ ] 2.4.1 Implement plugin tool discovery
- [ ] 2.4.2 Add tool registration to tool system registry
- [ ] 2.4.3 Create tool schema generation from plugin metadata
- [ ] 2.4.4 Add parameter validation for plugin tools
- [ ] 2.4.5 Implement tool invocation routing to plugins
- [ ] 2.4.6 Write tool registration tests

## 3. Lua Runtime Support

### 3.1 Lua Runtime Engine
- [ ] 3.1.1 Create `crates/crucible-plugins/src/runtime/lua.rs` module
- [ ] 3.1.2 Add mlua dependency to Cargo.toml
- [ ] 3.1.3 Implement `LuaPlugin` struct with VM initialization
- [ ] 3.1.4 Add plugin code loading and validation
- [ ] 3.1.5 Implement function discovery and invocation
- [ ] 3.1.6 Add Lua-Rust type conversions
- [ ] 3.1.7 Write unit tests for Lua plugin execution

### 3.2 Lua API Bindings
- [ ] 3.2.1 Expose crucible.read_note to Lua
- [ ] 3.2.2 Expose crucible.write_note to Lua
- [ ] 3.2.3 Expose crucible.search to Lua
- [ ] 3.2.4 Expose crucible.graph.* APIs to Lua
- [ ] 3.2.5 Expose crucible.state.* APIs to Lua
- [ ] 3.2.6 Add Lua standard library integration
- [ ] 3.2.7 Write Lua binding tests

### 3.3 Lua Sandbox
- [ ] 3.3.1 Configure Lua VM with restricted standard library
- [ ] 3.3.2 Remove unsafe Lua functions (io, os, debug)
- [ ] 3.3.3 Add memory limits for Lua VM
- [ ] 3.3.4 Add execution timeout for Lua scripts
- [ ] 3.3.5 Write Lua sandbox security tests

## 4. Standard Library

### 4.1 State Machine Library
- [ ] 4.1.1 Create `crates/crucible-plugins/src/standard_lib/state_machines.rs`
- [ ] 4.1.2 Define `StateMachine` struct with states and transitions
- [ ] 4.1.3 Implement state transition validation
- [ ] 4.1.4 Add state entry/exit callbacks
- [ ] 4.1.5 Implement state persistence
- [ ] 4.1.6 Add state machine visualization (dot format)
- [ ] 4.1.7 Expose to Rune and Lua as library
- [ ] 4.1.8 Write FSM tests and examples

### 4.2 Graph Algorithms Library
- [ ] 4.2.1 Create `crates/crucible-plugins/src/standard_lib/graph.rs`
- [ ] 4.2.2 Implement BFS traversal
- [ ] 4.2.3 Implement DFS traversal
- [ ] 4.2.4 Implement PageRank algorithm
- [ ] 4.2.5 Implement community detection (clustering)
- [ ] 4.2.6 Add shortest path algorithms
- [ ] 4.2.7 Expose to Rune and Lua as library
- [ ] 4.2.8 Write graph algorithm tests

### 4.3 Workflow Orchestration Library
- [ ] 4.3.1 Create `crates/crucible-plugins/src/standard_lib/workflows.rs`
- [ ] 4.3.2 Define DAG structure for workflow tasks
- [ ] 4.3.3 Implement topological sort for task ordering
- [ ] 4.3.4 Add task dependency resolution
- [ ] 4.3.5 Implement parallel task execution
- [ ] 4.3.6 Add error handling and retry logic
- [ ] 4.3.7 Expose to Rune and Lua as library
- [ ] 4.3.8 Write workflow orchestration tests

### 4.4 Utilities Library
- [ ] 4.4.1 Create `crates/crucible-plugins/src/standard_lib/utils.rs`
- [ ] 4.4.2 Add JSON parsing and serialization
- [ ] 4.4.3 Add YAML parsing and serialization
- [ ] 4.4.4 Add regex support
- [ ] 4.4.5 Add date/time utilities
- [ ] 4.4.6 Add string manipulation helpers
- [ ] 4.4.7 Add file path utilities
- [ ] 4.4.8 Expose to Rune and Lua as library

## 5. Capability-Based Security

### 5.1 Capability Definition
- [ ] 5.1.1 Define `Capability` enum (KilnRead, KilnWrite, Network, etc.)
- [ ] 5.1.2 Create `CapabilitySet` for plugin permissions
- [ ] 5.1.3 Parse capabilities from plugin frontmatter
- [ ] 5.1.4 Implement capability validation at runtime
- [ ] 5.1.5 Add default capabilities (KilnRead only)
- [ ] 5.1.6 Write capability tests

### 5.2 Permission Checks
- [ ] 5.2.1 Implement permission check before API calls
- [ ] 5.2.2 Add user approval prompts for high-risk operations
- [ ] 5.2.3 Implement session-based approval caching
- [ ] 5.2.4 Add capability escalation detection
- [ ] 5.2.5 Create audit log for capability usage
- [ ] 5.2.6 Write permission check tests

### 5.3 User Approval Flow
- [ ] 5.3.1 Design approval prompt UI for CLI
- [ ] 5.3.2 Implement approve/deny/remember options
- [ ] 5.3.3 Add approval persistence to config
- [ ] 5.3.4 Implement approval timeout (default 30s)
- [ ] 5.3.5 Add approval bypass for trusted plugins
- [ ] 5.3.6 Write approval flow tests

## 6. Agent-Plugin Interaction

### 6.1 Plugin Discovery for Agents
- [ ] 6.1.1 Add `list_plugins` tool for agents
- [ ] 6.1.2 Add `describe_plugin` tool with full API docs
- [ ] 6.1.3 Implement plugin capability query
- [ ] 6.1.4 Add plugin search by functionality
- [ ] 6.1.5 Return plugin schemas in tool discovery
- [ ] 6.1.6 Write agent discovery tests

### 6.2 Plugin Generation by Agents
- [ ] 6.2.1 Add `generate_plugin` tool for agents
- [ ] 6.2.2 Implement plugin code validation
- [ ] 6.2.3 Add syntax checking for Rune and Lua
- [ ] 6.2.4 Implement automatic testing of generated plugins
- [ ] 6.2.5 Add plugin installation after generation
- [ ] 6.2.6 Create plugin templates for common patterns
- [ ] 6.2.7 Write plugin generation tests

### 6.3 Plugin Composition
- [ ] 6.3.1 Implement inter-plugin communication
- [ ] 6.3.2 Add plugin dependency resolution
- [ ] 6.3.3 Create plugin import/export system
- [ ] 6.3.4 Add plugin versioning support
- [ ] 6.3.5 Write plugin composition tests

## 7. CLI Integration

### 7.1 Plugin Management Commands
- [ ] 7.1.1 Create `crates/crucible-cli/src/commands/plugins.rs`
- [ ] 7.1.2 Implement `cru plugins list` command
- [ ] 7.1.3 Implement `cru plugins show {name}` command
- [ ] 7.1.4 Implement `cru plugins test {file}` command
- [ ] 7.1.5 Implement `cru plugins install {file}` command
- [ ] 7.1.6 Implement `cru plugins validate` command
- [ ] 7.1.7 Implement `cru plugins remove {name}` command
- [ ] 7.1.8 Add `--json` output mode for scripting
- [ ] 7.1.9 Write CLI integration tests

### 7.2 Plugin Testing Tool
- [ ] 7.2.1 Implement sandbox testing framework
- [ ] 7.2.2 Add test case definition format
- [ ] 7.2.3 Implement test execution and reporting
- [ ] 7.2.4 Add coverage reporting (if possible)
- [ ] 7.2.5 Create test result visualization
- [ ] 7.2.6 Write testing tool tests

### 7.3 Plugin Template Generator
- [ ] 7.3.1 Implement `cru plugins new {name}` command
- [ ] 7.3.2 Create Rune plugin template
- [ ] 7.3.3 Create Lua plugin template
- [ ] 7.3.4 Add interactive template wizard
- [ ] 7.3.5 Generate example tests with templates
- [ ] 7.3.6 Write template generator tests

## 8. Example Plugins and Documentation

### 8.1 Example Plugins
- [ ] 8.1.1 Create `dice-roller.rune` (game master toolkit)
- [ ] 8.1.2 Create `combat-tracker.rune` (initiative, HP tracking)
- [ ] 8.1.3 Create `citation-parser.lua` (research toolkit)
- [ ] 8.1.4 Create `concept-extractor.lua` (knowledge extraction)
- [ ] 8.1.5 Create `state-machine-example.rune` (workflow demo)
- [ ] 8.1.6 Test all example plugins thoroughly
- [ ] 8.1.7 Document each example plugin with use cases

### 8.2 Standard Library Examples
- [ ] 8.2.1 Create FSM example (multi-phase workflow)
- [ ] 8.2.2 Create graph traversal example (related notes)
- [ ] 8.2.3 Create DAG workflow example (multi-agent coordination)
- [ ] 8.2.4 Create utility library example (data processing)
- [ ] 8.2.5 Document standard library patterns

### 8.3 Plugin Development Guide
- [ ] 8.3.1 Write plugin development quickstart
- [ ] 8.3.2 Document plugin API reference
- [ ] 8.3.3 Create security best practices guide
- [ ] 8.3.4 Write plugin testing guide
- [ ] 8.3.5 Document capability system
- [ ] 8.3.6 Add troubleshooting section
- [ ] 8.3.7 Create video tutorials (optional)

### 8.4 Domain-Specific Guides
- [ ] 8.4.1 Write "Game Master Toolkit" guide
- [ ] 8.4.2 Write "Research Workflow" guide
- [ ] 8.4.3 Write "Game Development Suite" guide
- [ ] 8.4.4 Create plugin gallery with screenshots
- [ ] 8.4.5 Document plugin sharing best practices

## 9. Testing and Validation

### 9.1 Unit Tests
- [ ] 9.1.1 Plugin runtime tests (Rune and Lua)
- [ ] 9.1.2 Sandbox security tests
- [ ] 9.1.3 Resource limit enforcement tests
- [ ] 9.1.4 Plugin API tests
- [ ] 9.1.5 Standard library tests
- [ ] 9.1.6 Capability system tests
- [ ] 9.1.7 Discovery and loading tests

### 9.2 Integration Tests
- [ ] 9.2.1 End-to-end plugin execution tests
- [ ] 9.2.2 Agent-generated plugin tests
- [ ] 9.2.3 Plugin composition tests
- [ ] 9.2.4 Multi-plugin workflow tests
- [ ] 9.2.5 CLI command integration tests
- [ ] 9.2.6 Tool registration tests

### 9.3 Security Tests
- [ ] 9.3.1 Sandbox escape attempt tests
- [ ] 9.3.2 Resource exhaustion tests
- [ ] 9.3.3 Capability escalation tests
- [ ] 9.3.4 Malicious code detection tests
- [ ] 9.3.5 Permission bypass tests
- [ ] 9.3.6 Audit log validation tests

### 9.4 Performance Tests
- [ ] 9.4.1 Plugin loading performance benchmarks
- [ ] 9.4.2 Execution overhead benchmarks
- [ ] 9.4.3 Memory usage profiling
- [ ] 9.4.4 Concurrent plugin execution tests
- [ ] 9.4.5 Large kiln stress tests

## 10. Performance and Optimization

### 10.1 Plugin Caching
- [ ] 10.1.1 Implement compiled plugin caching
- [ ] 10.1.2 Add plugin bytecode generation
- [ ] 10.1.3 Implement cache invalidation logic
- [ ] 10.1.4 Add cache size limits
- [ ] 10.1.5 Write caching performance tests

### 10.2 Execution Optimization
- [ ] 10.2.1 Profile plugin execution hotspots
- [ ] 10.2.2 Optimize API call overhead
- [ ] 10.2.3 Implement batched API calls
- [ ] 10.2.4 Add async plugin execution support
- [ ] 10.2.5 Optimize standard library implementations

### 10.3 Concurrent Execution
- [ ] 10.3.1 Design concurrent plugin execution model
- [ ] 10.3.2 Implement plugin execution pool
- [ ] 10.3.3 Add resource sharing mechanisms
- [ ] 10.3.4 Implement deadlock detection
- [ ] 10.3.5 Write concurrent execution tests

## 11. Future Extensibility

### 11.1 WASM Support (Stretch Goal)
- [ ] 11.1.1 Research WASM runtime options
- [ ] 11.1.2 Implement basic WASM plugin loading
- [ ] 11.1.3 Add WASM API bindings
- [ ] 11.1.4 Create WASM plugin examples
- [ ] 11.1.5 Document WASM plugin development

### 11.2 Plugin Marketplace (Future)
- [ ] 11.2.1 Design plugin repository format
- [ ] 11.2.2 Implement plugin publishing
- [ ] 11.2.3 Add plugin discovery from repository
- [ ] 11.2.4 Create rating and review system
- [ ] 11.2.5 Add plugin dependency management

### 11.3 Visual Plugin Editor (Future)
- [ ] 11.3.1 Design flow-based plugin editor
- [ ] 11.3.2 Implement node-based programming UI
- [ ] 11.3.3 Add visual debugging tools
- [ ] 11.3.4 Create plugin templates library

## 12. Deployment

### 12.1 Pre-Deployment Checklist
- [ ] 12.1.1 All tests passing (unit + integration + security)
- [ ] 12.1.2 Documentation complete and reviewed
- [ ] 12.1.3 Example plugins tested and documented
- [ ] 12.1.4 Security audit completed
- [ ] 12.1.5 Performance benchmarks meet targets
- [ ] 12.1.6 Standard library thoroughly tested

### 12.2 Deployment Steps
- [ ] 12.2.1 Merge plugin system to main branch
- [ ] 12.2.2 Create plugin directory in installer
- [ ] 12.2.3 Update CLI help text with plugin commands
- [ ] 12.2.4 Release notes with plugin system features
- [ ] 12.2.5 User migration guide (if applicable)

### 12.3 Post-Deployment
- [ ] 12.3.1 Monitor for user-reported issues
- [ ] 12.3.2 Collect feedback on plugin development experience
- [ ] 12.3.3 Gather metrics on plugin usage patterns
- [ ] 12.3.4 Plan next iteration based on feedback
