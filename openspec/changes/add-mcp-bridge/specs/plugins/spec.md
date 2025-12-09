## ADDED Requirements

### Requirement: Unified Event System

The system SHALL provide a unified event system where all hooks, filters, and interceptors are event handlers subscribed to typed events with pattern matching.

#### Scenario: Emit typed event
- **WHEN** something happens in the system (tool call, note change, etc.)
- **THEN** system SHALL emit typed event with payload
- **AND** event SHALL include timestamp and source
- **AND** registered handlers SHALL be invoked in priority order

#### Scenario: Subscribe to event type with pattern
- **GIVEN** Rune script registers handler with `on("tool:after", "just_test*", handler_fn)`
- **WHEN** event matches type AND pattern
- **THEN** handler SHALL be invoked with event context
- **AND** handler MAY modify event payload (filter)
- **AND** modified payload flows to next handler

#### Scenario: Event types available
- **WHEN** system is running
- **THEN** following event types SHALL be emittable:
  - `mcp:attached` - upstream MCP server connected
  - `tool:discovered` - tool discovered from any source (enables filtering/enrichment)
  - `tool:before` - before tool execution
  - `tool:after` - after tool execution
  - `tool:error` - tool execution failed
  - `note:parsed` - note was parsed (includes AST)
  - `note:created` - new note created
  - `note:modified` - note content changed
- **AND** custom event types MAY be registered

### Requirement: Event Handler (Hook)

The system SHALL support Rune functions as event handlers that can observe or transform events.

#### Scenario: Register Rune hook
- **GIVEN** Rune script exports handler function
- **WHEN** script is loaded
- **THEN** system SHALL register handler for specified event type
- **AND** handler SHALL receive event context and payload
- **AND** return value SHALL replace payload for next handler (if returned)

#### Scenario: Hook with wildcard pattern
- **GIVEN** hook registered with pattern `just_*`
- **WHEN** event has matching identifier (e.g., tool name `just_test`)
- **THEN** hook SHALL be invoked
- **AND** non-matching events SHALL skip this hook

#### Scenario: Hook priority ordering
- **GIVEN** multiple hooks registered for same event type
- **WHEN** event is emitted
- **THEN** hooks SHALL execute in priority order (lower = earlier)
- **AND** each hook receives output of previous hook
- **AND** chain continues until all hooks execute or one cancels

### Requirement: Built-in Filters as Hooks

The system SHALL implement built-in filters (test output, TOON transform) as pre-registered hooks on tool events.

#### Scenario: Test output filter as hook
- **GIVEN** test_filter is enabled in configuration
- **WHEN** `tool:after` event matches pattern `just_test*` or `just_ci*`
- **THEN** built-in test filter hook SHALL transform result
- **AND** extract summary lines from cargo/pytest/jest/go/rspec output
- **AND** pass filtered result to next hook

#### Scenario: TOON transform as hook
- **GIVEN** toon_transform is enabled in configuration
- **WHEN** `tool:after` event fires
- **THEN** built-in TOON hook SHALL convert JSON to TOON notation
- **AND** preserve data with reduced token usage

#### Scenario: Configure built-in hook patterns
- **GIVEN** configuration specifies custom pattern for built-in hook
- **WHEN** hook initializes
- **THEN** hook SHALL use configured pattern instead of default
- **AND** allow fine-grained control over which tools are filtered

### Requirement: Event Context

The system SHALL provide rich context to event handlers for informed decisions.

#### Scenario: Tool event context
- **WHEN** tool event handler is invoked
- **THEN** context SHALL include:
  - `event_type` - the event type (e.g., "tool:after")
  - `tool_name` - name of the tool
  - `tool_source` - where tool came from (kiln, just, rune, upstream)
  - `arguments` - tool call arguments
  - `result` - tool result (for after/error events)
  - `duration_ms` - execution time
  - `is_error` - whether tool failed

#### Scenario: Note event context
- **WHEN** note event handler is invoked
- **THEN** context SHALL include:
  - `event_type` - the event type (e.g., "note:parsed")
  - `note_path` - path to the note
  - `frontmatter` - parsed YAML frontmatter
  - `blocks` - AST block structure (for parsed events)
  - `raw_content` - original markdown (for modified events)

#### Scenario: Store custom data in context
- **GIVEN** handler stores data in context via `ctx.set("key", value)`
- **WHEN** later handler in chain executes
- **THEN** stored data SHALL be accessible via `ctx.get("key")`
- **AND** enable correlation across handler chain

### Requirement: Hook Discovery and Loading

The system SHALL automatically discover and load Rune hook scripts from configured directories.

#### Scenario: Discover hooks in kiln
- **WHEN** system initializes
- **THEN** system SHALL scan `KILN/.crucible/hooks/` for `.rn` files
- **AND** compile valid hook scripts
- **AND** register handlers for declared event types

#### Scenario: Hook script format
- **GIVEN** Rune script in hooks directory
- **WHEN** script exports `on_<event_type>` function or uses `#[hook]` attribute
- **THEN** system SHALL register as handler for that event type
- **AND** pattern defaults to `*` (all) unless specified

#### Scenario: Hot reload hook scripts
- **GIVEN** hook script file changes
- **WHEN** file watcher detects change
- **THEN** system SHALL recompile script
- **AND** re-register handlers
- **AND** log reload success or failure

### Requirement: Event Emission from Hooks

The system SHALL allow hooks to emit new events, enabling event-driven workflows.

#### Scenario: Hook emits derived event
- **GIVEN** hook processes tool result
- **WHEN** hook calls `ctx.emit("custom:event", payload)`
- **THEN** system SHALL queue new event
- **AND** process after current event chain completes
- **AND** other hooks MAY subscribe to custom event type

#### Scenario: Hook emits to external system
- **GIVEN** hook configured with webhook URL
- **WHEN** hook calls `ctx.webhook(url, payload)`
- **THEN** system SHALL POST payload to URL asynchronously
- **AND** not block event chain on response
