# Meta-Systems Specification

## Overview

This specification defines Crucible's extensible plugin architecture, enabling users and agents to write domain-specific code (Rune, Lua) that extends platform capabilities. Plugins run in secure sandboxes and integrate seamlessly with the tool system, agent system, and knowledge graph.

## ADDED Requirements

### Requirement: Multi-Language Plugin Runtime
The system SHALL support plugins written in multiple languages, with Rune and Lua as primary targets, executing in sandboxed environments with resource limits.

#### Scenario: Execute Rune plugin
- **WHEN** user or agent invokes a Rune plugin
- **THEN** system SHALL compile Rune code to bytecode
- **AND** SHALL execute in isolated Rune VM
- **AND** SHALL enforce memory limits (default 100MB, configurable)
- **AND** SHALL enforce CPU timeout (default 30s, configurable)
- **AND** SHALL return structured result or error

#### Scenario: Execute Lua plugin
- **WHEN** user or agent invokes a Lua plugin
- **THEN** system SHALL load Lua code into VM
- **AND** SHALL execute with restricted standard library (no io, os, debug)
- **AND** SHALL enforce memory limits
- **AND** SHALL enforce execution timeout
- **AND** SHALL return structured result or error

#### Scenario: Plugin execution failure
- **WHEN** plugin exceeds resource limits or encounters error
- **THEN** system SHALL terminate plugin gracefully
- **AND** SHALL return detailed error with line number and context
- **AND** SHALL not crash parent process
- **AND** SHALL log failure for debugging

#### Scenario: Plugin compilation caching
- **WHEN** plugin is loaded for first time
- **THEN** system SHALL compile to bytecode and cache
- **AND** subsequent invocations SHALL use cached bytecode
- **AND** cache SHALL be invalidated when source changes
- **AND** cache SHALL respect size limits

### Requirement: Plugin Discovery and Loading
The system SHALL automatically discover plugins from configured directories, parse metadata, validate security, and register as available tools.

#### Scenario: Discover plugins at startup
- **WHEN** CLI initializes
- **THEN** system SHALL scan `~/.config/crucible/plugins/` for system plugins
- **AND** SHALL scan `.crucible/plugins/` for kiln-specific plugins
- **AND** SHALL parse plugin frontmatter for metadata
- **AND** SHALL validate plugin syntax and security
- **AND** SHALL register valid plugins in tool registry

#### Scenario: Plugin override by scope
- **WHEN** kiln plugin has same name as system plugin
- **THEN** kiln plugin SHALL take precedence
- **AND** system SHALL log override for transparency
- **AND** `cru plugins list` SHALL show kiln plugin source

#### Scenario: Plugin hot-reload
- **WHEN** plugin file is modified during session
- **THEN** system SHALL detect change via file watcher
- **AND** SHALL invalidate cached bytecode
- **AND** SHALL reload plugin on next invocation
- **AND** active executions SHALL continue with old version

#### Scenario: Plugin validation failure
- **WHEN** plugin has syntax errors or security violations
- **THEN** system SHALL log validation failure with details
- **AND** SHALL NOT register plugin as available tool
- **AND** `cru plugins validate` SHALL report specific issues
- **AND** system SHALL continue loading other plugins

### Requirement: Plugin Metadata Format
Plugins SHALL be defined as code files with YAML frontmatter containing metadata, capabilities, and documentation.

#### Scenario: Parse plugin frontmatter
- **WHEN** system loads plugin file
- **THEN** system SHALL parse YAML frontmatter block
- **AND** SHALL extract name, description, version, author
- **AND** SHALL extract capabilities array (KilnRead, KilnWrite, etc.)
- **AND** SHALL extract parameters schema for tool registration
- **AND** SHALL extract documentation strings

#### Scenario: Plugin with minimal metadata
- **WHEN** plugin frontmatter includes only name and description
- **THEN** system SHALL use default values for other fields
- **AND** default capabilities SHALL be [KilnRead] only
- **AND** default version SHALL be "0.1.0"
- **AND** plugin SHALL be registered successfully

#### Scenario: Plugin with dependencies
- **WHEN** plugin frontmatter specifies dependencies
- **THEN** system SHALL verify dependencies are available
- **AND** SHALL load dependencies before loading plugin
- **AND** SHALL fail validation if dependencies missing
- **AND** SHALL report dependency errors clearly

#### Scenario: Example Rune plugin frontmatter
```yaml
---
name: dice-roller
description: Roll dice using standard notation (2d6+3)
version: 1.0.0
author: user@example.com
language: rune
capabilities: [KilnRead]
parameters:
  expression:
    type: string
    description: Dice notation (e.g., "2d6+3", "1d20")
    required: true
returns:
  type: object
  properties:
    expression: string
    result: number
    rolls: array
---
```

### Requirement: Capability-Based Security
The system SHALL enforce capability-based security where plugins declare required permissions, and high-risk operations require user approval.

#### Scenario: Plugin requests KilnRead capability
- **WHEN** plugin with [KilnRead] capability calls `crucible.read_note()`
- **THEN** system SHALL allow operation without user prompt
- **AND** SHALL enforce read-only access
- **AND** SHALL log access in audit log

#### Scenario: Plugin requests KilnWrite capability
- **WHEN** plugin with [KilnWrite] capability calls `crucible.write_note()`
- **THEN** system SHALL prompt user for approval on first use
- **AND** user MAY approve once, always, or deny
- **AND** approval SHALL be remembered for session or persisted
- **AND** denied operation SHALL return permission error

#### Scenario: Plugin capability escalation
- **WHEN** plugin without Network capability calls network API
- **THEN** system SHALL reject call immediately
- **AND** SHALL return permission denied error
- **AND** SHALL log attempted escalation
- **AND** SHALL notify user of security violation

#### Scenario: Available capabilities
- **WHEN** plugin defines capabilities in frontmatter
- **THEN** system SHALL support capability types:
  - `KilnRead` - Read notes and metadata
  - `KilnWrite` - Create, update, delete notes
  - `NetworkHttp` - Make HTTP requests
  - `NetworkHttps` - Make HTTPS requests
  - `FsRead` - Read files outside kiln
  - `FsWrite` - Write files outside kiln
  - `PluginInvoke` - Call other plugins
  - `StateManage` - Access persistent state
  - `Unsafe` - Unrestricted access (requires explicit user approval)

#### Scenario: User approval prompt
- **WHEN** plugin requires approval for operation
- **THEN** system SHALL display clear prompt with:
  - Plugin name and description
  - Operation being requested
  - Potential impact and risks
  - Options: Approve Once, Always, Deny
- **AND** SHALL timeout after 30s (default, configurable)
- **AND** timeout SHALL deny operation by default

### Requirement: Plugin API for Kiln Operations
Plugins SHALL have access to standardized API for reading/writing notes, searching, graph traversal, and state management.

#### Scenario: Read note from plugin
- **WHEN** plugin calls `crucible.read_note(path)`
- **THEN** system SHALL read note at specified path
- **AND** SHALL return content and frontmatter as structured data
- **AND** SHALL return error if note not found or permission denied
- **AND** SHALL respect capability requirements

#### Scenario: Write note from plugin
- **WHEN** plugin calls `crucible.write_note(path, content, frontmatter)`
- **THEN** system SHALL check KilnWrite capability
- **AND** SHALL prompt user for approval if required
- **AND** upon approval SHALL create or update note
- **AND** SHALL return success with metadata (word count, etc.)

#### Scenario: Semantic search from plugin
- **WHEN** plugin calls `crucible.search(query, options)`
- **THEN** system SHALL perform semantic search using embeddings
- **AND** SHALL return ranked results with relevance scores
- **AND** MAY filter by frontmatter properties
- **AND** SHALL respect result limits (default 20, max 100)

#### Scenario: Graph traversal from plugin
- **WHEN** plugin calls `crucible.graph.traverse(start, strategy, options)`
- **THEN** system SHALL traverse graph using specified strategy
- **AND** strategy MAY be "bfs", "dfs", "semantic", "backlinks"
- **AND** SHALL return discovered nodes with metadata
- **AND** SHALL respect depth limits (default 3, max 10)

#### Scenario: Persistent state management
- **WHEN** plugin calls `crucible.state.get(key)` or `crucible.state.set(key, value)`
- **THEN** system SHALL scope state to plugin namespace
- **AND** state SHALL persist across executions
- **AND** state SHALL be serialized as JSON
- **AND** state SHALL be stored in `.crucible/plugin-state/{plugin-name}.json`

#### Scenario: List available notes
- **WHEN** plugin calls `crucible.list_notes(folder, options)`
- **THEN** system SHALL return notes in specified folder
- **AND** MAY include metadata (word count, modification time)
- **AND** MAY filter by frontmatter properties
- **AND** MAY traverse recursively if requested

### Requirement: Standard Library for Common Patterns
The system SHALL provide reusable implementations of common patterns (state machines, graph algorithms, workflows) accessible to all plugins.

#### Scenario: Use state machine library
- **WHEN** plugin imports state machine library
- **THEN** plugin SHALL be able to define states and transitions
- **AND** library SHALL validate state transitions
- **AND** SHALL provide entry/exit callbacks
- **AND** SHALL persist state automatically
- **AND** SHALL support state visualization

#### Scenario: Use graph algorithms library
- **WHEN** plugin imports graph library
- **THEN** plugin SHALL access BFS, DFS, PageRank algorithms
- **AND** SHALL be able to run community detection
- **AND** SHALL compute shortest paths
- **AND** algorithms SHALL work with kiln graph

#### Scenario: Use workflow orchestration library
- **WHEN** plugin imports workflow library
- **THEN** plugin SHALL define tasks as DAG
- **AND** library SHALL execute tasks in topological order
- **AND** SHALL support parallel task execution
- **AND** SHALL handle errors with retry logic
- **AND** SHALL report progress during execution

#### Scenario: Use utilities library
- **WHEN** plugin imports utils library
- **THEN** plugin SHALL access JSON/YAML parsing
- **AND** SHALL have regex support
- **AND** SHALL have date/time utilities
- **AND** SHALL have string manipulation helpers

### Requirement: Agent-Generated Plugins
Agents SHALL be able to discover existing plugins, generate new plugin code, test in sandbox, and install for immediate use.

#### Scenario: Agent discovers plugins
- **WHEN** agent calls `list_plugins` tool
- **THEN** system SHALL return all registered plugins
- **AND** response SHALL include name, description, parameters
- **AND** response SHALL include capabilities required
- **AND** response SHALL include usage examples

#### Scenario: Agent reads plugin documentation
- **WHEN** agent calls `describe_plugin(name)` tool
- **THEN** system SHALL return full plugin metadata
- **AND** SHALL include parameter schemas
- **AND** SHALL include return type schema
- **AND** SHALL include example usage code
- **AND** SHALL include source code for inspection

#### Scenario: Agent generates plugin code
- **WHEN** agent invokes `generate_plugin` tool with specification
- **THEN** agent SHALL generate Rune or Lua code
- **AND** code SHALL include proper frontmatter
- **AND** code SHALL implement requested functionality
- **AND** system SHALL validate syntax before installation

#### Scenario: Agent tests generated plugin
- **WHEN** agent generates plugin code
- **THEN** system SHALL execute plugin in test sandbox
- **AND** SHALL run with provided test inputs
- **AND** SHALL validate outputs match expectations
- **AND** SHALL report any errors or failures
- **AND** SHALL allow iteration before installation

#### Scenario: Agent installs plugin
- **WHEN** agent confirms plugin is working correctly
- **THEN** system SHALL save plugin to `.crucible/plugins/`
- **AND** SHALL register plugin in tool registry
- **AND** plugin SHALL be immediately available for use
- **AND** system SHALL confirm installation to agent

#### Scenario: Agent composes plugins
- **WHEN** agent generates plugin that uses other plugins
- **THEN** plugin SHALL declare dependencies in frontmatter
- **AND** SHALL import dependencies via standard mechanism
- **AND** system SHALL validate dependencies exist
- **AND** plugin SHALL have access to dependency APIs

### Requirement: Resource Limits and Sandboxing
The system SHALL enforce strict resource limits on plugin execution to prevent abuse, with configurable limits per plugin.

#### Scenario: Memory limit enforcement
- **WHEN** plugin allocates memory during execution
- **THEN** system SHALL track total memory usage
- **AND** SHALL terminate plugin if exceeds limit (default 100MB)
- **AND** SHALL return out-of-memory error
- **AND** SHALL clean up allocated resources

#### Scenario: CPU timeout enforcement
- **WHEN** plugin executes for extended time
- **THEN** system SHALL track execution duration
- **AND** SHALL terminate plugin if exceeds timeout (default 30s)
- **AND** SHALL return timeout error
- **AND** SHALL not block other operations

#### Scenario: File size limit enforcement
- **WHEN** plugin attempts to create or write file
- **THEN** system SHALL check file size
- **AND** SHALL reject operation if exceeds limit (default 10MB)
- **AND** SHALL return file-too-large error

#### Scenario: Rate limiting
- **WHEN** plugin is invoked frequently
- **THEN** system SHALL track invocation rate
- **AND** SHALL throttle if exceeds limit (default 100/min)
- **AND** SHALL return rate-limit error
- **AND** SHALL allow burst allowance

#### Scenario: Configurable limits per plugin
- **WHEN** plugin frontmatter specifies custom limits
- **THEN** system SHALL use plugin-specific limits
- **AND** SHALL not allow limits exceeding system maximums
- **AND** SHALL require user approval for high limits
- **AND** SHALL log limit changes

### Requirement: Plugin Tool Registration
Plugins SHALL be registered as tools in the tool system, making them discoverable and invocable by agents via standard tool interface.

#### Scenario: Plugin registered as tool
- **WHEN** valid plugin is discovered
- **THEN** system SHALL register plugin as tool in registry
- **AND** tool name SHALL be plugin name
- **AND** tool description SHALL be from plugin frontmatter
- **AND** tool parameters SHALL be from plugin schema
- **AND** tool SHALL be invocable via standard tool API

#### Scenario: Agent invokes plugin tool
- **WHEN** agent calls plugin via tool system
- **THEN** system SHALL validate parameters against schema
- **AND** SHALL check capabilities before execution
- **AND** SHALL prompt user if approval required
- **AND** SHALL execute plugin and return result
- **AND** SHALL handle errors gracefully

#### Scenario: Plugin tool in MCP discovery
- **WHEN** MCP client queries available tools
- **THEN** plugin tools SHALL appear alongside built-in tools
- **AND** SHALL have proper JSON schemas
- **AND** SHALL be indistinguishable from built-in tools to agents

#### Scenario: Plugin tool versioning
- **WHEN** multiple versions of plugin exist
- **THEN** system SHALL use highest version by default
- **AND** MAY allow explicit version selection
- **AND** SHALL maintain backward compatibility where possible

### Requirement: CLI Plugin Management
The system SHALL provide CLI commands for listing, testing, installing, and managing plugins.

#### Scenario: List available plugins
- **WHEN** user runs `cru plugins list`
- **THEN** system SHALL display table with plugin information
- **AND** SHALL show name, description, version, source
- **AND** SHALL indicate system vs kiln plugins
- **AND** SHALL show capabilities required
- **AND** SHALL support `--json` output format

#### Scenario: Show plugin details
- **WHEN** user runs `cru plugins show {name}`
- **THEN** system SHALL display full plugin metadata
- **AND** SHALL show parameter schemas
- **AND** SHALL show usage examples
- **AND** SHALL show source file path
- **AND** SHALL show dependencies if any

#### Scenario: Test plugin in sandbox
- **WHEN** user runs `cru plugins test {file}`
- **THEN** system SHALL load plugin in test environment
- **AND** SHALL execute with test inputs if provided
- **AND** SHALL report success or failure with details
- **AND** SHALL not install plugin (dry-run only)

#### Scenario: Install plugin
- **WHEN** user runs `cru plugins install {file} --scope {system|kiln}`
- **THEN** system SHALL validate plugin
- **AND** SHALL copy to appropriate directory
- **AND** SHALL register in tool system
- **AND** SHALL confirm installation
- **AND** SHALL show installation path

#### Scenario: Validate all plugins
- **WHEN** user runs `cru plugins validate`
- **THEN** system SHALL check all discovered plugins
- **AND** SHALL report syntax errors
- **AND** SHALL report security violations
- **AND** SHALL report missing dependencies
- **AND** SHALL exit 0 if all valid, non-zero if any invalid

#### Scenario: Remove plugin
- **WHEN** user runs `cru plugins remove {name}`
- **THEN** system SHALL unregister plugin from tool system
- **AND** SHALL delete plugin file
- **AND** SHALL clean up cached bytecode
- **AND** SHALL confirm removal

#### Scenario: Create new plugin from template
- **WHEN** user runs `cru plugins new {name} --lang {rune|lua}`
- **THEN** system SHALL generate plugin template
- **AND** SHALL include proper frontmatter
- **AND** SHALL include example implementation
- **AND** SHALL include example tests
- **AND** SHALL save to current directory

### Requirement: Standard Library - State Machines
The system SHALL provide a state machine library enabling plugins to define and execute finite state machines.

#### Scenario: Define state machine
- **WHEN** plugin defines state machine with states and transitions
- **THEN** library SHALL validate state definitions
- **AND** SHALL validate transition definitions
- **AND** SHALL ensure initial state is valid
- **AND** SHALL detect unreachable states

#### Scenario: Execute state transition
- **WHEN** plugin calls `sm.transition(event)`
- **THEN** library SHALL validate transition is allowed from current state
- **AND** SHALL execute exit callback for current state
- **AND** SHALL change to new state
- **AND** SHALL execute entry callback for new state
- **AND** SHALL persist new state

#### Scenario: State persistence
- **WHEN** state machine state changes
- **THEN** library SHALL persist state automatically
- **AND** SHALL restore state on plugin reload
- **AND** SHALL scope state to plugin instance
- **AND** SHALL support multiple state machine instances

#### Scenario: State visualization
- **WHEN** plugin requests state machine visualization
- **THEN** library SHALL generate Graphviz dot format
- **AND** SHALL show states as nodes
- **AND** SHALL show transitions as edges
- **AND** SHALL highlight current state

### Requirement: Standard Library - Graph Algorithms
The system SHALL provide graph algorithm library for kiln traversal and analysis.

#### Scenario: BFS traversal
- **WHEN** plugin calls `graph.bfs(start, options)`
- **THEN** library SHALL perform breadth-first search from start node
- **AND** SHALL return nodes in BFS order
- **AND** SHALL respect depth limit
- **AND** SHALL support filtering by frontmatter properties

#### Scenario: DFS traversal
- **WHEN** plugin calls `graph.dfs(start, options)`
- **THEN** library SHALL perform depth-first search from start node
- **AND** SHALL return nodes in DFS order
- **AND** SHALL detect and handle cycles

#### Scenario: PageRank calculation
- **WHEN** plugin calls `graph.pagerank(options)`
- **THEN** library SHALL compute PageRank for kiln graph
- **AND** SHALL return nodes sorted by importance
- **AND** SHALL support damping factor configuration
- **AND** SHALL iterate until convergence

#### Scenario: Community detection
- **WHEN** plugin calls `graph.communities()`
- **THEN** library SHALL detect note clusters
- **AND** SHALL use modularity-based algorithm
- **AND** SHALL return communities with member nodes
- **AND** SHALL support minimum community size

#### Scenario: Shortest path
- **WHEN** plugin calls `graph.shortest_path(start, end)`
- **THEN** library SHALL find shortest path via wikilinks
- **AND** SHALL return path as ordered list of notes
- **AND** SHALL return null if no path exists

### Requirement: Standard Library - Workflow Orchestration
The system SHALL provide workflow library for defining and executing multi-task workflows as directed acyclic graphs.

#### Scenario: Define workflow DAG
- **WHEN** plugin defines workflow with tasks and dependencies
- **THEN** library SHALL validate DAG structure (no cycles)
- **AND** SHALL validate all dependencies exist
- **AND** SHALL compute topological ordering

#### Scenario: Execute workflow sequentially
- **WHEN** plugin calls `workflow.execute()` in sequential mode
- **THEN** library SHALL execute tasks in topological order
- **AND** SHALL wait for each task to complete
- **AND** SHALL pass outputs to dependent tasks
- **AND** SHALL report progress during execution

#### Scenario: Execute workflow in parallel
- **WHEN** plugin calls `workflow.execute(parallel=true)`
- **THEN** library SHALL execute independent tasks concurrently
- **AND** SHALL respect task dependencies
- **AND** SHALL limit concurrency to system maximum
- **AND** SHALL aggregate results

#### Scenario: Workflow error handling
- **WHEN** task in workflow fails
- **THEN** library SHALL implement retry logic if configured
- **AND** SHALL skip dependent tasks if retry exhausted
- **AND** SHALL mark workflow as partially failed
- **AND** SHALL return detailed error report

### Requirement: Inter-Plugin Communication
Plugins with PluginInvoke capability SHALL be able to call other plugins, enabling plugin composition and reuse.

#### Scenario: Plugin calls another plugin
- **WHEN** plugin with PluginInvoke capability calls `crucible.invoke_plugin(name, params)`
- **THEN** system SHALL verify PluginInvoke capability
- **AND** SHALL verify target plugin exists
- **AND** SHALL execute target plugin with parameters
- **AND** SHALL return target plugin result
- **AND** SHALL propagate errors appropriately

#### Scenario: Plugin dependency declaration
- **WHEN** plugin declares dependencies in frontmatter
- **THEN** system SHALL load dependencies before plugin
- **AND** SHALL make dependencies available via import
- **AND** SHALL fail validation if dependencies missing
- **AND** SHALL detect circular dependencies

#### Scenario: Plugin version compatibility
- **WHEN** plugin depends on specific version of another plugin
- **THEN** system SHALL check version compatibility
- **AND** SHALL fail validation if version mismatch
- **AND** SHALL report version conflict clearly

### Requirement: Plugin Audit Logging
The system SHALL maintain audit log of plugin executions, capability usage, and security events.

#### Scenario: Log plugin execution
- **WHEN** plugin executes
- **THEN** system SHALL log execution start with timestamp
- **AND** SHALL log plugin name, version, invoker (user/agent)
- **AND** SHALL log execution end with duration
- **AND** SHALL log success or error status

#### Scenario: Log capability usage
- **WHEN** plugin uses capability (read, write, network, etc.)
- **THEN** system SHALL log capability invocation
- **AND** SHALL log specific operation (read_note, path, etc.)
- **AND** SHALL log approval status (granted/denied)

#### Scenario: Log security events
- **WHEN** plugin attempts capability escalation or security violation
- **THEN** system SHALL log security event with high priority
- **AND** SHALL include full context (plugin, operation, reason)
- **AND** SHALL notify user of security event

#### Scenario: Audit log retention
- **WHEN** audit log grows large
- **THEN** system SHALL rotate logs automatically
- **AND** SHALL retain recent logs (default 30 days)
- **AND** SHALL compress archived logs
- **AND** SHALL respect user retention configuration

### Requirement: Example Plugins Provided
The system SHALL ship with example plugins demonstrating common patterns and use cases.

#### Scenario: Dice roller plugin (game master toolkit)
- **WHEN** system installs
- **THEN** SHALL include `dice-roller.rune` example
- **AND** plugin SHALL support standard dice notation (2d6+3)
- **AND** plugin SHALL support advantage/disadvantage rolls
- **AND** plugin SHALL be fully documented with examples

#### Scenario: Combat tracker plugin (game master toolkit)
- **WHEN** system installs
- **THEN** SHALL include `combat-tracker.rune` example
- **AND** plugin SHALL track initiative order
- **AND** plugin SHALL track HP and status effects
- **AND** plugin SHALL use dice-roller plugin as dependency

#### Scenario: Citation parser plugin (research toolkit)
- **WHEN** system installs
- **THEN** SHALL include `citation-parser.lua` example
- **AND** plugin SHALL parse BibTeX citations
- **AND** plugin SHALL extract authors, title, year, venue
- **AND** plugin SHALL create citation notes with proper frontmatter

#### Scenario: Concept extractor plugin (research toolkit)
- **WHEN** system installs
- **THEN** SHALL include `concept-extractor.lua` example
- **AND** plugin SHALL extract key concepts from notes
- **AND** plugin SHALL use NLP techniques
- **AND** plugin SHALL create concept map with wikilinks

#### Scenario: State machine example
- **WHEN** system installs
- **THEN** SHALL include state machine example using standard library
- **AND** example SHALL demonstrate multi-phase workflow
- **AND** example SHALL show state persistence
- **AND** example SHALL include visualization

## MODIFIED Requirements

### Requirement: Tool System Extended by Plugins
The existing tool system SHALL be extended to support dynamically registered plugin tools.

#### Scenario: Plugin tools appear in tool discovery
- **WHEN** ACP agent queries available tools
- **THEN** response SHALL include both built-in and plugin tools
- **AND** plugin tools SHALL have same schema format
- **AND** SHALL be indistinguishable to agents

#### Scenario: Tool execution routes to plugins
- **WHEN** tool invocation matches plugin name
- **THEN** system SHALL route to plugin execution runtime
- **AND** SHALL enforce plugin security sandbox
- **AND** SHALL handle errors as tool execution errors

### Requirement: Agent System Uses Plugins
Agents SHALL discover and use plugins as tools, and MAY generate new plugins when needed capabilities are missing.

#### Scenario: Agent discovers plugin capabilities
- **WHEN** agent explores available tools
- **THEN** agent SHALL see plugin tools with full descriptions
- **AND** agent SHALL understand plugin parameters and returns
- **AND** agent SHALL be able to compose multiple plugins

#### Scenario: Agent generates plugin on-demand
- **WHEN** agent needs capability not available in tools
- **THEN** agent MAY generate plugin code
- **AND** agent SHALL test plugin before installation
- **AND** plugin SHALL be available for subsequent operations

## REMOVED Requirements

_No existing requirements removed - this is a new capability that extends existing systems_

---

## Implementation Notes

### Language Choice Rationale
- **Rune**: Primary choice due to Rust syntax familiarity, strong safety guarantees, and excellent Rust interop
- **Lua**: Secondary choice for wider adoption, simpler syntax, and gaming community familiarity
- **Future WASM**: Enables plugins in any language (Rust, C++, Go, etc.) compiled to WASM

### Security Architecture
- Sandboxing via language runtime (Rune VM, Lua VM)
- No unsafe code allowed without explicit user approval
- Capability-based permissions modeled after Web APIs
- Resource limits prevent denial-of-service
- Audit logging for security monitoring

### Performance Considerations
- Bytecode caching reduces compilation overhead
- Plugin execution overhead should be < 1ms for simple operations
- Concurrent execution for independent plugins
- Standard library optimized for common cases

### Integration Points
- Tool system: Plugin tools register alongside built-in tools
- Agent system: Agents discover and generate plugins
- CLI: Plugin management commands
- Configuration: Plugin limits and permissions in config file
