## ADDED Requirements

### Requirement: Agent Definition Format
The system SHALL support agent definitions as markdown files with YAML frontmatter, stored in `.crucible/agents/` (project-specific) or `~/.config/crucible/agents/` (system-wide).

#### Scenario: User creates project-specific agent
- **WHEN** user creates `my-kiln/.crucible/agents/code-reviewer.md` with valid frontmatter
- **THEN** system SHALL discover agent on next CLI invocation
- **AND** agent SHALL be available for spawning by primary agents
- **AND** agent SHALL appear in `cru agents list` output

#### Scenario: Agent definition with required frontmatter
- **WHEN** agent definition includes name, description, model, and permissions in frontmatter
- **THEN** system SHALL parse and validate all required fields
- **AND** system SHALL extract markdown content after frontmatter as agent prompt
- **AND** agent SHALL be registered in agent registry

#### Scenario: Invalid agent definition
- **WHEN** agent definition has missing required fields or invalid values
- **THEN** system SHALL fail validation during registry discovery
- **AND** system SHALL provide clear error message indicating which field is invalid
- **AND** invalid agent SHALL NOT be registered or available for use

#### Scenario: Project agents override system agents
- **WHEN** project agent has same name as system agent
- **THEN** project agent SHALL take precedence over system agent
- **AND** `cru agents list` SHALL show project agent with source path
- **AND** spawning agent by name SHALL use project version

### Requirement: Agent Registry and Discovery
The system SHALL automatically discover and register agents from configured directories at CLI startup, providing listing and validation capabilities.

#### Scenario: Automatic discovery at startup
- **WHEN** CLI initializes
- **THEN** system SHALL scan `~/.config/crucible/agents/` for system agents
- **AND** system SHALL scan `.crucible/agents/` for project agents
- **AND** all valid agents SHALL be registered in agent registry
- **AND** invalid agents SHALL log warnings but not prevent startup

#### Scenario: List available agents
- **WHEN** user runs `cru agents list`
- **THEN** system SHALL display all registered agents with names and descriptions
- **AND** output SHALL indicate agent source (system or project)
- **AND** output SHALL show agent model and permission requirements
- **AND** disabled agents SHALL NOT appear in list

#### Scenario: Validate agent definitions
- **WHEN** user runs `cru agents validate`
- **THEN** system SHALL check all agent definitions for validity
- **AND** system SHALL report any errors with file path and specific issue
- **AND** system SHALL exit with non-zero status if any validation fails
- **AND** system SHALL confirm success if all agents valid

#### Scenario: Get specific agent definition
- **WHEN** primary agent requests agent definition by name
- **THEN** system SHALL return agent definition if exists and enabled
- **AND** system SHALL return error if agent not found or disabled
- **AND** system SHALL include full prompt content and metadata

### Requirement: Agent Spawning and Execution
The system SHALL enable primary agents to spawn specialized subagents with isolated context, executing them as separate LLM calls and returning structured results.

#### Scenario: Primary agent spawns subagent
- **WHEN** primary agent invokes spawn_agent with agent name and task description
- **THEN** system SHALL load agent definition from registry
- **AND** system SHALL create new LLM session with agent prompt plus task description
- **AND** system SHALL execute subagent with isolated context (no parent conversation history)
- **AND** system SHALL return markdown result to primary agent

#### Scenario: Subagent execution context
- **WHEN** subagent executes
- **THEN** subagent SHALL receive only task description and agent prompt
- **AND** subagent SHALL NOT have access to primary agent's conversation history
- **AND** subagent SHALL have access to kiln read tools (read_note, semantic_search, list_notes)
- **AND** subagent SHALL respect permission constraints from spawn request

#### Scenario: Subagent result format
- **WHEN** subagent completes execution
- **THEN** result SHALL be structured markdown with frontmatter metadata
- **AND** frontmatter SHALL include execution time, model, token count, and status
- **AND** markdown content SHALL include task summary and detailed findings
- **AND** result SHALL be saved to session folder with wikilink from parent

#### Scenario: Subagent execution failure
- **WHEN** subagent encounters error during execution
- **THEN** system SHALL capture error message and stack trace
- **AND** system SHALL return error result to primary agent with failure details
- **AND** system SHALL save error result to session folder for debugging
- **AND** execution queue SHALL continue processing remaining tasks

### Requirement: Session Management and Storage
The system SHALL store agent sessions as markdown files in `.crucible/sessions/` with wikilink-based parent/child relationships, enabling session replay and debugging.

#### Scenario: Create session folder for primary agent
- **WHEN** primary agent session starts
- **THEN** system SHALL create folder `.crucible/sessions/YYYY-MM-DD-description/`
- **AND** system SHALL create `session.md` for primary agent conversation
- **AND** system SHALL create `metadata.json` with session-level information
- **AND** folder name SHALL be unique (timestamp plus sanitized query)

#### Scenario: Store subagent result with wikilink
- **WHEN** subagent completes and returns result
- **THEN** system SHALL save result as markdown file in session folder
- **AND** filename SHALL be `{agent-name}-{task-id}.md` for uniqueness
- **AND** primary agent's session.md SHALL include wikilink to subagent file
- **AND** subagent file frontmatter SHALL reference parent session ID

#### Scenario: Session folder structure
- **WHEN** session with multiple subagents completes
- **THEN** session folder SHALL contain session.md (primary)
- **AND** session folder SHALL contain one file per subagent execution
- **AND** session folder SHALL contain metadata.json with execution statistics
- **AND** all files SHALL be valid markdown with YAML frontmatter

#### Scenario: Session wikilink navigation
- **WHEN** user views session.md in markdown editor
- **THEN** wikilinks SHALL be clickable to navigate to subagent results
- **AND** subagent frontmatter SHALL reference parent session ID for backtracking
- **AND** bidirectional navigation SHALL be possible (parent ↔ child)

### Requirement: Permission Inheritance and Validation
The system SHALL enforce permission inheritance where subagents cannot exceed parent permissions, with validation at spawn time to prevent privilege escalation.

#### Scenario: Subagent inherits parent permissions
- **WHEN** primary agent spawns subagent without explicit permission constraints
- **THEN** subagent SHALL inherit parent's permission set
- **AND** subagent SHALL NOT have permissions parent doesn't have
- **AND** permission inheritance SHALL be logged in session metadata

#### Scenario: Subagent with constrained permissions
- **WHEN** primary agent spawns subagent with explicit permission constraints
- **THEN** system SHALL validate constraints are subset of parent permissions
- **AND** system SHALL grant only requested subset to subagent
- **AND** system SHALL reject spawn if requesting permissions parent doesn't have
- **AND** constraint SHALL be logged in subagent's frontmatter

#### Scenario: Default kiln read permissions
- **WHEN** any subagent spawns regardless of constraints
- **THEN** subagent SHALL always have FilesystemRead permission
- **AND** subagent SHALL always have SemanticSearch permission
- **AND** subagent SHALL have access to read_note, list_notes, semantic_search tools
- **AND** these baseline permissions SHALL NOT be removable

#### Scenario: Permission escalation attempt
- **WHEN** subagent requests permission parent doesn't have
- **THEN** system SHALL reject spawn request with clear error
- **AND** error SHALL specify which permission was invalid
- **AND** error SHALL list parent's current permissions
- **AND** primary agent SHALL receive error result to handle

### Requirement: Execution Queue with Sequential Processing
The system SHALL use a queue-based architecture to process agent tasks sequentially for MVP, designed to support future provider-specific concurrency limits.

#### Scenario: Single agent execution
- **WHEN** primary agent spawns one subagent
- **THEN** subagent SHALL execute immediately (no queuing)
- **AND** primary agent SHALL wait for result before continuing
- **AND** execution time SHALL be logged in metadata

#### Scenario: Multiple agents queued sequentially
- **WHEN** primary agent spawns multiple subagents in sequence
- **THEN** first subagent SHALL execute immediately
- **AND** subsequent subagents SHALL queue behind first
- **AND** each subagent SHALL execute only after previous completes
- **AND** primary agent SHALL receive results in spawn order

#### Scenario: Queue state visibility
- **WHEN** agents are queued for execution
- **THEN** user SHALL see progress updates indicating current agent
- **AND** queue depth SHALL be visible in session metadata
- **AND** execution order SHALL be deterministic (FIFO)

#### Scenario: Queue designed for future concurrency
- **WHEN** queue architecture is implemented
- **THEN** code SHALL support future max_concurrent parameter
- **AND** code SHALL support future provider-specific limits configuration
- **AND** changing from sequential to concurrent SHALL require minimal code changes

### Requirement: Depth Limiting
The system SHALL enforce a maximum agent depth of 2 levels (User → Primary → Subagents) to prevent runaway recursion and maintain simplicity for MVP.

#### Scenario: User spawns primary agent
- **WHEN** user starts chat session
- **THEN** primary agent SHALL have depth 0
- **AND** primary agent SHALL be allowed to spawn subagents
- **AND** depth SHALL be tracked in session metadata

#### Scenario: Primary spawns subagent
- **WHEN** primary agent spawns subagent
- **THEN** subagent SHALL have depth 1
- **AND** subagent SHALL be allowed to execute
- **AND** depth SHALL be incremented from parent

#### Scenario: Subagent attempts to spawn child
- **WHEN** subagent (depth 1) attempts to spawn another agent
- **THEN** system SHALL reject spawn request with clear error
- **AND** error SHALL state "Maximum agent depth (2) exceeded"
- **AND** error SHALL explain subagents cannot spawn their own subagents
- **AND** subagent SHALL receive error to handle gracefully

#### Scenario: Depth limit configuration
- **WHEN** depth limit is checked
- **THEN** limit SHALL be hardcoded to 2 for MVP
- **AND** limit SHALL be defined as constant for easy future adjustment
- **AND** error messages SHALL reference current limit dynamically

### Requirement: Progress Observability
The system SHALL provide abbreviated progress updates to users showing subagent actions, while storing full details in session markdown files for debugging.

#### Scenario: User sees subagent spawn notification
- **WHEN** primary agent spawns subagent
- **THEN** user SHALL see message "→ Running {agent-name} agent..."
- **AND** message SHALL appear immediately when spawn starts
- **AND** message SHALL be visually distinct (e.g., arrow prefix)

#### Scenario: User sees subagent completion summary
- **WHEN** subagent completes execution
- **THEN** user SHALL see abbreviated result summary (first 100 chars of Summary section)
- **AND** summary SHALL be indented to show it's subagent output
- **AND** full details SHALL be available in session markdown file

#### Scenario: User sees subagent error notification
- **WHEN** subagent encounters error
- **THEN** user SHALL see error message with agent name and error type
- **AND** message SHALL indicate error severity
- **AND** full error details SHALL be in session markdown file

#### Scenario: Progress updates in real-time
- **WHEN** subagent is executing
- **THEN** progress updates SHALL appear with < 100ms delay
- **AND** updates SHALL not block primary agent reasoning
- **AND** updates SHALL be thread-safe for concurrent future execution

### Requirement: Agent Model Selection
The system SHALL support per-agent model selection (sonnet, haiku, opus) enabling optimization of cost vs. capability for different agent types.

#### Scenario: Agent definition specifies model
- **WHEN** agent definition includes `model: haiku` in frontmatter
- **THEN** spawned subagent SHALL use haiku model for execution
- **AND** model choice SHALL be logged in result frontmatter
- **AND** token cost SHALL reflect model pricing

#### Scenario: Default model fallback
- **WHEN** agent definition omits model field
- **THEN** system SHALL default to sonnet model
- **AND** default SHALL be configurable via CLI config
- **AND** default choice SHALL be logged

#### Scenario: Model override at spawn time
- **WHEN** primary agent spawns subagent with explicit model override
- **THEN** override SHALL take precedence over agent definition
- **AND** override SHALL be validated (must be valid model name)
- **AND** override SHALL be logged in session metadata

### Requirement: Agent Enablement Control
The system SHALL support enabling/disabling agents via frontmatter flag, allowing temporary deactivation without deletion.

#### Scenario: Disabled agent not available
- **WHEN** agent definition has `enabled: false` in frontmatter
- **THEN** agent SHALL NOT appear in `cru agents list`
- **AND** agent SHALL NOT be spawnable by name
- **AND** spawn attempt SHALL return "agent not found" error

#### Scenario: Re-enabling disabled agent
- **WHEN** user changes `enabled: false` to `enabled: true` and restarts CLI
- **THEN** agent SHALL be rediscovered and registered
- **AND** agent SHALL become available for spawning
- **AND** agent SHALL appear in `cru agents list`

#### Scenario: Default enablement
- **WHEN** agent definition omits `enabled` field
- **THEN** agent SHALL default to enabled
- **AND** agent SHALL be available immediately

### Requirement: Session Metadata Collection
The system SHALL collect and store metadata about agent execution including timing, token usage, models used, and execution status.

#### Scenario: Session-level metadata
- **WHEN** session completes
- **THEN** metadata.json SHALL include session ID, start/end times, and status
- **AND** metadata SHALL include list of spawned subagents with IDs
- **AND** metadata SHALL include total token usage across all agents
- **AND** metadata SHALL include primary agent model and version

#### Scenario: Subagent-level metadata
- **WHEN** subagent execution completes
- **THEN** subagent markdown frontmatter SHALL include spawned_at and completed_at timestamps
- **AND** frontmatter SHALL include model, token count, and execution duration
- **AND** frontmatter SHALL include parent session reference
- **AND** frontmatter SHALL include permission set granted

#### Scenario: Error metadata
- **WHEN** subagent fails with error
- **THEN** frontmatter SHALL include error type and message
- **AND** frontmatter SHALL include stack trace if available
- **AND** frontmatter SHALL mark status as failed
- **AND** metadata SHALL be sufficient for debugging

### Requirement: CLI Commands for Agent Management
The system SHALL provide CLI commands for listing, validating, and inspecting agent definitions.

#### Scenario: List available agents
- **WHEN** user runs `cru agents list`
- **THEN** output SHALL show table with name, description, model, and source
- **AND** output SHALL indicate system vs project agents
- **AND** output SHALL show permission requirements
- **AND** output SHALL be sorted alphabetically by name

#### Scenario: Show agent details
- **WHEN** user runs `cru agents show {name}`
- **THEN** output SHALL display full frontmatter metadata
- **AND** output SHALL show agent prompt content
- **AND** output SHALL show source file path
- **AND** output SHALL indicate if agent is enabled

#### Scenario: Validate all agents
- **WHEN** user runs `cru agents validate`
- **THEN** system SHALL check all agent definitions in both locations
- **AND** system SHALL report validation errors with file paths
- **AND** system SHALL exit 0 if all valid, non-zero if any invalid
- **AND** output SHALL be machine-parsable for CI integration

#### Scenario: Validate specific agent
- **WHEN** user runs `cru agents validate {name}`
- **THEN** system SHALL validate only specified agent
- **AND** system SHALL show detailed validation results
- **AND** system SHALL suggest fixes for common errors

### Requirement: Integration with Primary Agent Chat Mode
The system SHALL integrate agent spawning seamlessly into the primary agent's chat interface, enabling task decomposition during natural conversation.

#### Scenario: Primary agent decides to spawn subagent
- **WHEN** primary agent determines task needs specialized expertise
- **THEN** primary agent SHALL call spawn_agent tool with agent name and task
- **AND** spawn SHALL execute without interrupting conversation flow
- **AND** primary agent SHALL incorporate result into response to user

#### Scenario: User sees transparent subagent usage
- **WHEN** primary agent uses subagents during task
- **THEN** user SHALL see abbreviated progress updates
- **AND** user SHALL see primary agent's synthesis of subagent results
- **AND** user SHALL be able to navigate session folder to see full subagent outputs

#### Scenario: Multi-step workflow with subagents
- **WHEN** primary agent executes multi-step workflow
- **THEN** primary agent SHALL spawn appropriate subagent for each step
- **AND** subagents SHALL execute sequentially in order
- **AND** primary agent SHALL synthesize results across all steps
- **AND** user SHALL see coherent response incorporating all subagent outputs

### Requirement: Agent Reflection and Self-Improvement
The system SHALL support optional reflection capabilities enabling agents to self-evaluate outputs and retry with improved approaches, following the Reflexion pattern proven to improve output quality.

#### Scenario: Agent with reflection enabled
- **WHEN** agent definition has `enable_reflection: true` in frontmatter
- **THEN** agent SHALL self-evaluate output before returning to parent
- **AND** if evaluation indicates failure or low quality, agent SHALL retry with refined approach
- **AND** maximum retries SHALL be limited by `max_retries` (default 1)
- **AND** reflection SHALL be optional and disabled by default

#### Scenario: Reflection loop with improvement
- **WHEN** agent completes execution with reflection enabled
- **THEN** agent SHALL critique own output via LLM-based evaluation
- **AND** critique SHALL use criteria specified in `reflection_criteria` frontmatter field
- **AND** if critique identifies issues, agent SHALL generate improved output
- **AND** reflection history SHALL be included in result metadata
- **AND** each iteration SHALL be logged with critique and improvement

#### Scenario: Reflection disabled (default)
- **WHEN** agent definition omits `enable_reflection` or sets to false
- **THEN** agent SHALL execute once and return result immediately
- **AND** no self-evaluation SHALL occur
- **AND** no additional token cost SHALL be incurred

#### Scenario: Max retries reached without success
- **WHEN** agent with reflection exceeds `max_retries` without passing self-evaluation
- **THEN** agent SHALL return best attempt with metadata indicating partial success
- **AND** all reflection iterations SHALL be included in result for debugging
- **AND** parent agent SHALL receive indication that reflection failed to achieve criteria

### Requirement: Human-in-the-Loop Approval Gates
The system SHALL support approval gates for agents to request user permission before executing sensitive or irreversible actions, ensuring user control over agent operations.

#### Scenario: Agent requires approval for action types
- **WHEN** agent definition has `requires_approval` list in frontmatter
- **THEN** agent SHALL pause execution before performing listed action types
- **AND** user SHALL see clear description of proposed action and impact
- **AND** user SHALL have options to approve, deny, or modify parameters
- **AND** approval timeout SHALL be configurable (default 300 seconds)

#### Scenario: User approves with modifications
- **WHEN** agent requests approval and user modifies parameters
- **THEN** agent SHALL execute with modified parameters
- **AND** modifications SHALL be logged in session metadata
- **AND** agent SHALL acknowledge parameter changes in result
- **AND** modified parameters SHALL be validated before execution

#### Scenario: User denies approval
- **WHEN** agent requests approval and user denies
- **THEN** agent SHALL abort action and return failure result
- **AND** denial reason SHALL be captured in result metadata
- **AND** primary agent SHALL receive denial to adjust strategy
- **AND** session SHALL record denial for audit trail

#### Scenario: Request approval mid-execution
- **WHEN** agent calls `request_approval(action, reason)` tool during execution
- **THEN** system SHALL pause agent and prompt user with action details
- **AND** agent execution SHALL resume after user responds
- **AND** timeout SHALL abort action if no response within configured period
- **AND** timeout behavior SHALL be configurable (abort or auto-approve)

#### Scenario: Approval for destructive operations
- **WHEN** agent attempts FilesystemWrite or DatabaseWrite with approval required
- **THEN** system SHALL show diff or preview of changes
- **AND** user SHALL see reversibility status (reversible/irreversible)
- **AND** irreversible actions SHALL require explicit confirmation
- **AND** user SHALL have option to request additional context before deciding

### Requirement: Enhanced Observability and Tracing
The system SHALL provide comprehensive tracing and debugging capabilities for multi-agent workflows, enabling developers to understand execution flow and diagnose failures.

#### Scenario: Trace ID generation and propagation
- **WHEN** subagent spawns
- **THEN** system SHALL generate unique trace_id for the request
- **AND** trace_id SHALL propagate to all logging and error messages
- **AND** trace_id SHALL be included in session metadata
- **AND** trace_id SHALL enable correlation across agent executions

#### Scenario: Parent chain tracking
- **WHEN** subagent executes
- **THEN** metadata SHALL include parent_chain array showing full spawning hierarchy
- **AND** parent_chain SHALL show [user → primary → subagent] path
- **AND** parent_chain SHALL enable tracing errors to root cause
- **AND** parent_chain SHALL be visible in session files

#### Scenario: Session trace visualization
- **WHEN** user runs `cru sessions trace {session-id}`
- **THEN** system SHALL display execution flow as tree structure
- **AND** tree SHALL show all agents with spawn order and timing
- **AND** failed agents SHALL be visually distinct in tree
- **AND** trace SHALL include execution duration and token usage per agent

#### Scenario: Error attribution in traces
- **WHEN** subagent encounters error during execution
- **THEN** error message SHALL include trace_id and parent_chain
- **AND** error SHALL clearly indicate which agent failed
- **AND** error SHALL show context of what parent was attempting
- **AND** trace SHALL enable debugging without full session replay

## MODIFIED Requirements

_No existing requirements modified - this is a new capability_

## REMOVED Requirements

_No existing requirements removed - this is a new capability_
