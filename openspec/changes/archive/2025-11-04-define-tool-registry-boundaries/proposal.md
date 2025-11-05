## Why

The current tool system has significant architectural violations and unnecessary complexity. Research of successful agentic frameworks (LangChain, OpenAI Swarm, Anthropic, CrewAI) shows that production systems use extremely simple patterns: simple function registries with direct execution and no caching, lifecycle management, or configuration services. Our current system has accumulated architectural bloat that doesn't exist in any successful production tool system.

## What Changes

- **Simplified Tool Registry**: Reduce tool registry to simple HashMap-based function storage following proven production patterns
- **Direct Function Execution**: Remove all intermediate layers (caching, lifecycle, config) and call functions directly like successful systems
- **Remove Unnecessary Services**: Eliminate caching, lifecycle management, and configuration services that no production systems use
- **Global State Elimination**: Replace global patterns with simple, direct function registration and execution
- **Follow Production Patterns**: Implement the same simple patterns used by all successful agentic frameworks

## Impact

- **Affected specs**:
  - `tool-registry` (new capability)
- **Affected code**:
  - `crates/crucible-tools/src/types.rs` - remove global state, simplify to HashMap registry
  - `crates/crucible-cli/src/common/tool_manager.rs` - replace with simple function registry
  - `crates/crucible-tools/src/` - remove unnecessary caching, lifecycle, and config services
- **Security impact**:
  - Maintains permission validation without complex enforcement layers
  - Removes unrestricted global access to tool registry
  - Follows simple security patterns from successful production systems
- **Architectural impact**:
  - Eliminates global state anti-patterns
  - Removes unnecessary complexity that doesn't exist in production systems
  - Matches proven patterns from LangChain, OpenAI Swarm, Anthropic, etc.
- **Performance impact**:
  - Improves performance by removing unnecessary intermediate layers
  - Direct function execution eliminates overhead
  - Follows successful production patterns that scale efficiently