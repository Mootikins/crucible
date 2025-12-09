## ADDED Requirements

### Requirement: MCP Bridge Client

The system SHALL connect to external MCP servers and aggregate their tools into Crucible's unified tool interface.

#### Scenario: Connect to stdio MCP server
- **GIVEN** configuration specifies an MCP server with stdio transport
- **WHEN** bridge initializes
- **THEN** system SHALL spawn the server process
- **AND** establish MCP connection via stdio
- **AND** discover available tools via `tools/list`
- **AND** register tools with configured namespace prefix

#### Scenario: Connect to HTTP+SSE MCP server
- **GIVEN** configuration specifies an MCP server with HTTP+SSE transport
- **WHEN** bridge initializes
- **THEN** system SHALL connect to the server endpoint
- **AND** establish SSE connection for notifications
- **AND** discover available tools via `tools/list`

#### Scenario: Handle tool list changes
- **WHEN** upstream server sends `toolListChanged` notification
- **THEN** bridge SHALL re-fetch tool list
- **AND** update registered tools
- **AND** emit event for downstream consumers

#### Scenario: Route tool call to upstream
- **GIVEN** tool call targets an upstream MCP tool
- **WHEN** tool is invoked
- **THEN** bridge SHALL forward request to correct upstream server
- **AND** return result through interceptor pipeline

### Requirement: Tool Selector

The system SHALL filter and transform tools from upstream MCP servers based on configuration.

#### Scenario: Whitelist specific tools
- **GIVEN** configuration includes `allowed_tools` list
- **WHEN** discovering tools from upstream
- **THEN** only whitelisted tools SHALL be registered
- **AND** other tools SHALL be ignored

#### Scenario: Blacklist specific tools
- **GIVEN** configuration includes `blocked_tools` list
- **WHEN** discovering tools from upstream
- **THEN** blacklisted tools SHALL NOT be registered
- **AND** other tools SHALL be available

#### Scenario: Namespace tool names
- **GIVEN** configuration specifies `prefix` for upstream server
- **WHEN** registering tools
- **THEN** tool names SHALL be prefixed (e.g., `gh_create_issue`)
- **AND** original names SHALL be used when forwarding calls

### Requirement: Interceptor Pipeline

The system SHALL process tool calls through a configurable pipeline of interceptors for transformation and enrichment.

#### Scenario: Pre-call interception
- **WHEN** tool call is received
- **THEN** system SHALL execute pre-call interceptors in order
- **AND** interceptors MAY modify the request
- **AND** interceptors MAY reject the request with error
- **AND** pipeline continues to execution if all interceptors pass

#### Scenario: Post-call interception
- **WHEN** tool execution completes
- **THEN** system SHALL execute post-call interceptors in order
- **AND** interceptors MAY transform the result
- **AND** final transformed result SHALL be returned to caller

#### Scenario: Interceptor error handling
- **WHEN** interceptor fails
- **THEN** system SHALL log the error
- **AND** continue with original request/result (fail-open)
- **AND** include warning in response metadata

### Requirement: TOON Transform Interceptor

The system SHALL provide a built-in interceptor to convert JSON tool results to TOON format for token efficiency.

#### Scenario: Transform JSON object to TOON
- **GIVEN** TOON interceptor is enabled
- **WHEN** tool returns JSON object result
- **THEN** interceptor SHALL convert to TOON notation
- **AND** preserve all data with reduced token usage

#### Scenario: Pass through non-JSON content
- **GIVEN** TOON interceptor is enabled
- **WHEN** tool returns plain text or binary content
- **THEN** interceptor SHALL pass through unchanged

### Requirement: Test Output Filter Interceptor

The system SHALL provide a built-in interceptor to extract summaries from test framework output.

#### Scenario: Filter cargo test output
- **GIVEN** test filter interceptor is enabled
- **WHEN** tool returns cargo test output
- **THEN** interceptor SHALL extract summary lines
- **AND** extract failure details (limited)
- **AND** discard verbose per-test output

#### Scenario: Detect test framework automatically
- **WHEN** tool returns test output
- **THEN** interceptor SHALL detect framework (cargo, pytest, jest, go test, rspec, mix)
- **AND** apply appropriate filter
- **AND** pass through unrecognized output unchanged

### Requirement: LLM Enrichment Interceptor

The system SHALL provide an interceptor that enriches tool results using auxiliary LLM calls.

#### Scenario: Enrich with prompt template
- **GIVEN** LLM enrichment interceptor is configured with prompt template
- **WHEN** tool result matches trigger pattern
- **THEN** interceptor SHALL call configured LLM provider
- **AND** include tool result in prompt context
- **AND** append LLM response to result

#### Scenario: Selective enrichment by tool pattern
- **GIVEN** interceptor configured with `triggers` pattern list
- **WHEN** tool name does not match any pattern
- **THEN** interceptor SHALL pass through unchanged
- **AND** not invoke LLM

### Requirement: Event Emitter Interceptor

The system SHALL provide an interceptor that publishes tool results to the event bus.

#### Scenario: Emit tool result event
- **GIVEN** event emitter interceptor is enabled
- **WHEN** tool execution completes
- **THEN** interceptor SHALL publish event with tool name, arguments, and result
- **AND** include timing metadata
- **AND** workflow system MAY consume these events

### Requirement: Rune Script Interceptor

The system SHALL support user-defined interceptors written in Rune scripts.

#### Scenario: Load Rune interceptor
- **GIVEN** Rune script defines `before_call` or `after_call` function
- **WHEN** interceptor pipeline initializes
- **THEN** system SHALL compile and register Rune interceptor
- **AND** execute in pipeline order

#### Scenario: Rune interceptor transforms result
- **GIVEN** Rune script defines `after_call(ctx, result)` function
- **WHEN** post-call pipeline executes
- **THEN** Rune function SHALL receive tool context and result
- **AND** return value SHALL replace result for next interceptor
