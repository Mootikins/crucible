# Implementation Tasks

## 1. Core Type Refactoring
- [ ] 1.1 Rename `crucible-core/src/agent/` directory to `crucible-core/src/capabilities/`
- [ ] 1.2 Rename `AgentDefinition` → `CapabilityDefinition` in types.rs
- [ ] 1.3 Rename `AgentRegistry` → `CapabilityRegistry` in mod.rs
- [ ] 1.4 Rename `AgentLoader` → `CapabilityLoader` in loader.rs
- [ ] 1.5 Rename `CapabilityMatcher` (already correct - keep as-is)
- [ ] 1.6 Update module re-exports in `crucible-core/src/lib.rs`
- [ ] 1.7 Update trait file: `traits/agent.rs` → `traits/capabilities.rs`

## 2. CLI Integration Updates
- [ ] 2.1 Move `crucible-cli/src/agents/` to `crucible-cli/src/acp/discovery/`
- [ ] 2.2 Update all imports in CLI crate to use new capability names
- [ ] 2.3 Update `crucible-cli/src/commands/chat.rs` imports
- [ ] 2.4 Update factory methods to use capability terminology
- [ ] 2.5 Search for remaining "agent" references that should be "capability"

## 3. Capability-to-ACP Delegation
- [ ] 3.1 Add `acp_server: Option<String>` field to `CapabilityDefinition`
- [ ] 3.2 Create `capabilities/delegation.rs` module
- [ ] 3.3 Implement delegation logic: capability → ACP server selection
- [ ] 3.4 Write unit tests for delegation matching
- [ ] 3.5 Handle ACP unavailability with clear error messages

## 4. Update File Paths
- [ ] 4.1 Update capability definition paths: `.crucible/agents/` → `.crucible/capabilities/`
- [ ] 4.2 Update system capability path in config
- [ ] 4.3 Update example capability files
- [ ] 4.4 Update CLI commands: `cru agents` → `cru capabilities`

## 5. Documentation
- [ ] 5.1 Write ADR-001: Capabilities vs Agents vs Orchestration
- [ ] 5.2 Update AGENTS.md to use "capabilities" terminology
- [ ] 5.3 Add capability-to-ACP delegation examples
- [ ] 5.4 Document migration path for existing agent definitions
- [ ] 5.5 Update inline documentation in all refactored modules

## 6. Testing
- [ ] 6.1 Update all existing agent tests to use capability names
- [ ] 6.2 Add integration test for capability-to-ACP delegation
- [ ] 6.3 Test capability discovery from both directories
- [ ] 6.4 Test precedence (project overrides system)
- [ ] 6.5 Run full test suite and fix failures

## 7. Cleanup
- [ ] 7.1 Search codebase for remaining "agent" that should be "capability"
- [ ] 7.2 Update error messages to use new terminology
- [ ] 7.3 Update log messages to use new terminology
- [ ] 7.4 Run clippy and fix warnings
- [ ] 7.5 Update CHANGELOG.md with breaking changes
