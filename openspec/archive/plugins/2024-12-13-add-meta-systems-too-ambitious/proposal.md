# Add Meta-Systems: Extensible Plugin Architecture

## Why

Crucible aims to be a **platform for collaborative intelligence**, not just a product with fixed features. While the tool system provides 10 foundational tools, real-world use cases demand domain-specific capabilities that can't be predicted at design time:

- **Game masters** need dice rolling, combat tracking, NPC generation
- **Researchers** need citation parsing, paper ranking, concept extraction
- **Game developers** need balance analysis, simulation engines, playtesting tools
- **Writers** need consistency checking, character tracking, world-building systems

Building all possible tools into the binary is impossible and defeats the purpose of a knowledge-first platform. Instead, we need **extensible infrastructure** where users and agents can write plugins to add capabilities specific to their domains.

### The Critical Insight: Agents Can Write Code

Modern AI agents (Claude, GPT-4, etc.) can generate working code. This means:
1. Users can ask for new capabilities in natural language
2. Agents generate plugin code (Rune/Lua) to implement those capabilities
3. Plugins are tested in sandbox and saved to the kiln
4. Other agents immediately gain access to the new capabilities
5. **The system extends itself over time without shipping updates**

This creates a **self-bootstrapping platform** where agents write tools that other agents use, enabling infinite domain-specific customization while maintaining security through sandboxing.

### Why Rune and Lua

- **Rune**: Rust-like syntax, memory-safe, designed for sandboxing, async support, native Rust interop
- **Lua**: Widely known, lightweight, battle-tested in games/editors (WoW, Neovim), simple C FFI
- Both languages have mature embedding runtimes with security sandboxes

The plugin system already has foundation in `crucible-plugins/` and `crucible-rune-macros/` crates.

## What Changes

**NEW CAPABILITY: Meta-Systems and Plugin Architecture**

### Plugin System Core
- Multi-language support: Rune (primary), Lua (secondary), WASM (future)
- Plugin discovery from system, kiln, and user directories
- Security sandbox with resource limits (memory, CPU, filesystem)
- Plugin API for kiln operations, tool registration, state management
- Standard library for common patterns (state machines, graph algorithms)

### Plugin Categories
1. **Custom Tools**: Domain-specific operations (dice rolling, citation parsing, etc.)
2. **Workflow Engines**: Orchestration systems (state machines, DAG runners, event systems)
3. **Graph Exploration**: Domain-specific kiln traversal strategies
4. **Meta-Systems**: Infrastructure for other plugins (testing, logging, generators)

### Agent-Plugin Interaction
- Agents can discover available plugins via tool discovery
- Agents can generate plugin code in Rune/Lua
- Agents can test plugins in sandbox before saving
- Agents can compose multiple plugins together
- Agents can request new plugins when needed capabilities are missing

### Security Model
- Capability-based security (plugins declare required permissions)
- User approval for high-risk operations (filesystem, network, plugin generation)
- Resource limits enforced by runtime (memory, CPU, file size)
- Plugin isolation (cannot escape sandbox)

### Standard Library
Crucible provides standard implementations for common patterns:
- State machines (FSM builder and executor)
- Graph algorithms (BFS, DFS, PageRank, clustering)
- Workflow orchestration (DAG execution, parallel tasks)
- Data structures (queues, graphs, trees, priority queues)
- Utilities (JSON/YAML parsing, regex, date/time)

### CLI Integration
- `cru plugins list` - Show available plugins
- `cru plugins show {name}` - Display plugin details and documentation
- `cru plugins test {file}` - Test plugin in sandbox
- `cru plugins install {file}` - Install plugin to system or kiln
- `cru plugins validate` - Check all plugins for security/syntax issues

## Impact

### Affected Specs
- **meta-systems** (NEW) - Complete plugin architecture and standard library
- **tool-system** (reference) - Plugins extend tool capabilities dynamically
- **agent-system** (reference) - Agents use and generate plugins
- **acp-integration** (reference) - Plugin tools available via ACP protocol

### Affected Code

**Existing Foundation (Already Started)**:
- `crates/crucible-plugins/` - Plugin infrastructure (expand)
- `crates/crucible-rune-macros/` - Rune macro support (expand)

**New Components**:
- `crates/crucible-plugins/src/runtime/` - NEW - Plugin execution runtime
  - `runtime/rune.rs` - Rune plugin engine
  - `runtime/lua.rs` - Lua plugin engine
  - `runtime/sandbox.rs` - Security sandbox implementation
  - `runtime/limits.rs` - Resource limit enforcement
- `crates/crucible-plugins/src/discovery.rs` - NEW - Plugin discovery and loading
- `crates/crucible-plugins/src/api.rs` - NEW - Plugin API for kiln access
- `crates/crucible-plugins/src/standard_lib/` - NEW - Standard library implementations
  - `standard_lib/state_machines.rs` - FSM engine
  - `standard_lib/graph.rs` - Graph algorithms
  - `standard_lib/workflows.rs` - DAG orchestration
  - `standard_lib/utils.rs` - Utilities (JSON, regex, etc.)

**Integration Points**:
- `crates/crucible-tools/` - Plugin tools register alongside built-in tools
- `crates/crucible-cli/src/commands/plugins.rs` - NEW - Plugin management commands
- `crates/crucible-agents/` (future) - Agents discover and use plugins
- `crates/crucible-core/src/traits/plugins.rs` - NEW - Plugin trait definitions

**Dependencies Added**:
- `rune = "0.13"` - Rune runtime (already in workspace)
- `mlua = "0.9"` - Lua embedding library
- `serde_json = "1.0"` - JSON serialization for plugin API
- `governor = "0.6"` - Rate limiting for plugin execution
- `caps = "0.5"` - Linux capabilities for sandboxing (optional)

### Implementation Strategy

**Phase 1: Plugin Runtime (Weeks 1-2)**
- Implement Rune plugin execution engine
- Add basic security sandbox with resource limits
- Create plugin discovery system
- Define plugin API for kiln access

**Phase 2: Lua Support & Standard Library (Weeks 3-4)**
- Add Lua plugin support
- Implement standard library (state machines, graph algorithms)
- Create plugin testing framework
- Add resource limit enforcement

**Phase 3: Agent Integration (Week 5)**
- Enable agents to discover plugins via tool system
- Add plugin generation capability for agents
- Implement plugin composition patterns
- Create plugin validation and testing tools

**Phase 4: CLI & Examples (Week 6)**
- Add plugin management CLI commands
- Create example plugins (dice roller, citation parser, etc.)
- Write documentation for plugin development
- Build plugin template generator

### User-Facing Impact

**Immediate Benefits**:
- Users can extend Crucible with domain-specific tools without waiting for releases
- Agents can generate custom plugins for user-specific workflows
- Plugins are shared as markdown notes (version controllable, searchable)
- No recompilation required - plugins loaded dynamically

**Long-Term Vision**:
- Community plugin ecosystem emerges organically
- Users share domain-specific plugin collections (game master toolkit, research suite, etc.)
- Agents build increasingly sophisticated plugins using other plugins
- Platform becomes more capable over time without central development

**Example Use Cases**:
```
Game Master:
$ cru chat "I need to track combat initiative"
🤖 I'll create a combat tracker plugin for you.
   *Generates dice-roller.rune and combat-tracker.rune*
   Done! Use 'roll_dice' and 'track_combat' tools.

Researcher:
$ cru chat "Parse BibTeX citations from this paper"
🤖 I'll write a citation parser plugin.
   *Generates citation-parser.lua*
   Found 47 citations, saved to citations/ folder.

Game Developer:
$ cru chat "Simulate 10,000 playtests with these rules"
🤖 I'll create a simulation engine.
   *Generates simulation-engine.rune*
   Running simulations... Balance analysis saved to analysis.md
```

### Security Considerations

**Sandboxing is Critical**:
- Plugins run in isolated sandboxes with enforced limits
- User approval required for sensitive operations
- Capability-based security model (explicit permission grants)
- No access to parent process memory or system resources

**Plugin Review**:
- Generated plugins should be reviewed by users before permanent installation
- Syntax and safety validation before execution
- Clear indication when agents generate code
- Audit log of plugin execution for debugging

### Timeline
- **Week 1-2**: Rune runtime, sandbox, discovery
- **Week 3-4**: Lua support, standard library
- **Week 5**: Agent integration
- **Week 6**: CLI commands, examples, documentation
- **Estimated effort**: 6 weeks for production-ready plugin system

### Dependencies
- Rune and Lua embedding libraries (both mature)
- Security sandbox infrastructure (OS-level capabilities)
- Tool system (existing, plugins extend it)
- Agent system (parallel development, agents will use plugins)
