# Workflow Markup Specification

## Overview

This specification defines prose-friendly workflow markup for Crucible, enabling workflows to be written in natural markdown prose with lightweight syntax for agents, data flow, and execution context. Workflows are parsed into executable DAGs, and execution traces are stored as compact toon-format session markers.

## ADDED Requirements

### Requirement: Parse Workflow Nodes from Markdown Headings

The system SHALL extract workflow nodes from markdown headings, where each heading represents a step/phase in the workflow, with optional agent and channel annotations.

#### Scenario: Parse simple heading as workflow node
- **GIVEN** markdown document with heading `## Process File`
- **WHEN** workflow parser processes document
- **THEN** system SHALL create workflow node with name "Process File"
- **AND** node SHALL have no agent assigned (None)
- **AND** node SHALL have no channel assigned (None)
- **AND** node SHALL be marked as optional (not critical)

#### Scenario: Parse heading with agent annotation
- **GIVEN** markdown heading `## Validate Cart @cart-service`
- **WHEN** workflow parser processes heading
- **THEN** system SHALL create workflow node with name "Validate Cart"
- **AND** node SHALL have agent "cart-service"
- **AND** agent name SHALL be extracted after `@` symbol
- **AND** whitespace around agent name SHALL be trimmed

#### Scenario: Parse heading with channel annotation
- **GIVEN** markdown heading `## Review Changes #code-review`
- **WHEN** workflow parser processes heading
- **THEN** system SHALL create workflow node with name "Review Changes"
- **AND** node SHALL have channel "code-review"
- **AND** channel name SHALL be extracted after `#` symbol
- **AND** whitespace SHALL be trimmed

#### Scenario: Parse heading with agent and channel
- **GIVEN** markdown heading `## Deploy Service @k8s-operator #production`
- **WHEN** workflow parser processes heading
- **THEN** system SHALL create workflow node with name "Deploy Service"
- **AND** node SHALL have agent "k8s-operator"
- **AND** node SHALL have channel "production"
- **AND** both annotations SHALL be extracted correctly

#### Scenario: Parse heading with critical marker
- **GIVEN** markdown heading `## Process Payment @payment-service !`
- **WHEN** workflow parser processes heading
- **THEN** system SHALL create workflow node with name "Process Payment"
- **AND** node SHALL be marked as critical (required=true)
- **AND** failure in this node SHALL halt workflow execution
- **AND** `!` marker SHALL be stripped from node name

#### Scenario: Parse heading with optional marker
- **GIVEN** markdown heading `## Deploy Staging ?`
- **WHEN** workflow parser processes heading
- **THEN** system SHALL create workflow node with name "Deploy Staging"
- **AND** node SHALL be marked as optional (required=false)
- **AND** failure in this node SHALL not halt workflow
- **AND** `?` marker SHALL be stripped from node name

#### Scenario: Parse heading hierarchy
- **GIVEN** markdown with nested headings:
  ```markdown
  ## Build Pipeline
  ### Quick Filter
  ### Parse
  ### Store
  ```
- **WHEN** workflow parser processes document
- **THEN** system SHALL create parent node "Build Pipeline"
- **AND** SHALL create child nodes "Quick Filter", "Parse", "Store"
- **AND** child nodes SHALL reference parent node ID
- **AND** heading level SHALL determine hierarchy depth

### Requirement: Parse Data Flow Notation

The system SHALL extract data flow edges from prose containing `->` notation, representing directed edges between workflow steps with optional type annotations.

#### Scenario: Parse simple data flow
- **GIVEN** prose text `file.md → parsed_content`
- **WHEN** workflow parser extracts data flow
- **THEN** system SHALL create edge with input "file.md"
- **AND** edge SHALL have output "parsed_content"
- **AND** neither SHALL have type annotations
- **AND** edge direction SHALL be left-to-right

#### Scenario: Parse data flow with type annotation
- **GIVEN** prose text `project_config::Config → schema.sql::SQL`
- **WHEN** workflow parser extracts data flow
- **THEN** system SHALL create edge with input "project_config"
- **AND** input type SHALL be "Config"
- **AND** output SHALL be "schema.sql"
- **AND** output type SHALL be "SQL"
- **AND** `::` SHALL delimit name and type

#### Scenario: Parse multi-step flow chain
- **GIVEN** prose text `file.md → AST → enriched_blocks → database`
- **WHEN** workflow parser extracts data flow
- **THEN** system SHALL create three edges:
  - file.md → AST
  - AST → enriched_blocks
  - enriched_blocks → database
- **AND** intermediate nodes SHALL be implicit
- **AND** flow SHALL maintain left-to-right order

#### Scenario: Parse multi-output flow
- **GIVEN** prose text:
  ```markdown
  Takes input → creates:
  - output_a
  - output_b::Type
  - output_c
  ```
- **WHEN** workflow parser extracts data flow
- **THEN** system SHALL create three edges from "input":
  - input → output_a (no type)
  - input → output_b (type: Type)
  - input → output_c (no type)
- **AND** bullet list SHALL indicate multiple outputs

#### Scenario: Parse conditional flow
- **GIVEN** prose text `If hash_changed → proceed to Parse`
- **WHEN** workflow parser extracts data flow
- **THEN** system SHALL create conditional edge to "Parse" node
- **AND** edge condition SHALL be "hash_changed"
- **AND** `If` keyword SHALL indicate conditional
- **AND** target node SHALL be "Parse"

### Requirement: Build Workflow DAG

The system SHALL construct a directed acyclic graph (DAG) from parsed workflow nodes and edges, validating structure and detecting cycles.

#### Scenario: Build simple DAG
- **GIVEN** markdown with headings and data flows:
  ```markdown
  ## Step A
  input → output_a

  ## Step B
  output_a → output_b
  ```
- **WHEN** workflow parser builds DAG
- **THEN** system SHALL create graph with 2 nodes
- **AND** SHALL create edge from Step A → Step B
- **AND** edge data SHALL reference "output_a"
- **AND** graph SHALL be validated as acyclic

#### Scenario: Detect cycle in workflow
- **GIVEN** markdown defining circular flow:
  ```markdown
  ## Step A
  input → middle

  ## Step B
  middle → output

  ## Step C
  output → input
  ```
- **WHEN** workflow parser builds DAG
- **THEN** system SHALL detect cycle: A → B → C → A
- **AND** SHALL return validation error with cycle path
- **AND** SHALL not create invalid DAG

#### Scenario: Build DAG with parallel branches
- **GIVEN** markdown:
  ```markdown
  ## Initialize
  config → params

  ## Branch A
  params → result_a

  ## Branch B
  params → result_b

  ## Merge
  result_a, result_b → final_output
  ```
- **WHEN** workflow parser builds DAG
- **THEN** system SHALL create graph with parallel branches
- **AND** "Initialize" SHALL have edges to both "Branch A" and "Branch B"
- **AND** "Merge" SHALL have edges from both branches
- **AND** topological sort SHALL respect dependencies

#### Scenario: Resolve node references by name
- **GIVEN** data flow `output_a → Step B`
- **WHEN** workflow parser builds edges
- **THEN** system SHALL find node with name "Step B"
- **AND** SHALL create edge to that node ID
- **AND** SHALL handle case-insensitive matching
- **AND** SHALL return error if node not found

### Requirement: Parse Toon-Format Session Markers

The system SHALL parse `session-toon` code blocks containing execution traces in toon-format, validating syntax and converting to structured session data.

#### Scenario: Parse valid session marker
- **GIVEN** code block:
  ````markdown
  ```session-toon
  execution[2]{phase,agent,status,duration_ms}:
   Planning,product-manager,success,4500
   Design,architect,success,6200
  ```
  ````
- **WHEN** parser processes code block
- **THEN** system SHALL parse toon-format successfully
- **AND** SHALL create 2 session records
- **AND** first record SHALL have phase="Planning", agent="product-manager"
- **AND** second record SHALL have status="success", duration_ms=6200

#### Scenario: Parse session marker with all standard fields
- **GIVEN** code block with fields:
  `execution[1]{phase,agent,channel,input,output,duration_ms,status,tokens_used}`
- **WHEN** parser processes session marker
- **THEN** system SHALL extract all 8 standard fields
- **AND** SHALL validate required fields (phase, status)
- **AND** SHALL allow optional fields to be empty
- **AND** SHALL store as structured SessionMarker object

#### Scenario: Invalid toon syntax in session marker
- **GIVEN** code block with malformed toon:
  ```session-toon
  execution[2{phase,agent}:
   Planning,product-manager
  ```
- **WHEN** parser processes code block
- **THEN** system SHALL return parse error
- **AND** error SHALL indicate missing `]` in array declaration
- **AND** error SHALL include line number and position
- **AND** document parsing SHALL continue (non-fatal error)

#### Scenario: Session marker with custom fields
- **GIVEN** code block with additional fields:
  `execution[1]{phase,agent,custom_metric,error_code}`
- **WHEN** parser processes session marker
- **THEN** system SHALL parse custom fields
- **AND** SHALL store in SessionMarker metadata
- **AND** custom fields SHALL be queryable
- **AND** SHALL not require schema definition

### Requirement: Store and Query Session Markers

The system SHALL store session markers in database with indexing on standard fields, enabling efficient querying by agent, phase, channel, status, and time range.

#### Scenario: Store session marker in database
- **GIVEN** parsed SessionMarker object
- **WHEN** system stores in database
- **THEN** session SHALL be linked to workflow document ID
- **AND** SHALL be linked to workflow node/phase ID
- **AND** SHALL store timestamp of execution
- **AND** SHALL index on: agent, channel, status, phase
- **AND** SHALL preserve all custom fields

#### Scenario: Query sessions by agent
- **GIVEN** multiple sessions executed by different agents
- **WHEN** user queries `sessions.where(agent = "product-manager")`
- **THEN** system SHALL return only sessions with agent="product-manager"
- **AND** SHALL use indexed lookup (no full scan)
- **AND** results SHALL be ordered by timestamp descending

#### Scenario: Query sessions by status
- **GIVEN** sessions with status: success, failed, timeout
- **WHEN** user queries `sessions.where(status = "failed")`
- **THEN** system SHALL return only failed sessions
- **AND** SHALL include failure details if available
- **AND** results SHALL be grouped by phase

#### Scenario: Query sessions by time range
- **GIVEN** sessions executed over multiple days
- **WHEN** user queries `sessions.where(timestamp > "2025-11-20")`
- **THEN** system SHALL return sessions after date
- **AND** SHALL use timestamp index
- **AND** SHALL support date range queries (between)

#### Scenario: Aggregate session metrics
- **GIVEN** multiple sessions for same phase
- **WHEN** user queries `sessions.aggregate(phase, avg(duration_ms))`
- **THEN** system SHALL return average duration per phase
- **AND** SHALL support sum, avg, min, max aggregations
- **AND** SHALL group by phase, agent, channel, or status

### Requirement: Execute Basic Workflows

The system SHALL execute workflows by traversing DAG in topological order, passing data between nodes, invoking agents, and recording session markers.

#### Scenario: Execute linear workflow
- **GIVEN** workflow with 3 sequential steps: A → B → C
- **WHEN** user executes workflow
- **THEN** system SHALL execute steps in order: A, B, C
- **AND** SHALL pass output from A as input to B
- **AND** SHALL pass output from B as input to C
- **AND** SHALL record session marker after each step

#### Scenario: Execute workflow with parallel branches
- **GIVEN** workflow with fork: A → (B, C) → D
- **WHEN** user executes workflow
- **THEN** system SHALL execute A first
- **AND** SHALL execute B and C in parallel (if supported)
- **AND** SHALL wait for both B and C to complete
- **AND** SHALL execute D after both complete
- **AND** SHALL pass outputs from B and C to D

#### Scenario: Invoke agent during workflow step
- **GIVEN** workflow node `## Validate @cart-service`
- **WHEN** workflow execution reaches this node
- **THEN** system SHALL invoke agent "cart-service" via tool system
- **AND** SHALL pass node inputs as agent parameters
- **AND** SHALL capture agent outputs
- **AND** SHALL record agent name in session marker

#### Scenario: Handle critical step failure
- **GIVEN** workflow with critical step marked `!`
- **WHEN** critical step fails (returns error)
- **THEN** system SHALL halt workflow execution
- **AND** SHALL record session marker with status="failed"
- **AND** SHALL include error message and stack trace
- **AND** SHALL not execute subsequent steps

#### Scenario: Handle optional step failure
- **GIVEN** workflow with optional step marked `?`
- **WHEN** optional step fails
- **THEN** system SHALL log warning
- **AND** SHALL record session marker with status="skipped"
- **AND** SHALL continue to next step
- **AND** downstream steps SHALL handle missing input

#### Scenario: Write session markers during execution
- **GIVEN** workflow execution in progress
- **WHEN** each step completes
- **THEN** system SHALL append session marker to workflow document
- **AND** marker SHALL include: phase, agent, status, duration, tokens
- **AND** marker SHALL be formatted as `session-toon` code block
- **AND** marker SHALL be inserted after corresponding heading

### Requirement: Workflow Parser Integration

The system SHALL integrate workflow parsing into existing extension system, registering WorkflowExtension and ToonExtension with appropriate priority.

#### Scenario: Register WorkflowExtension
- **GIVEN** parser initialization
- **WHEN** extensions are registered
- **THEN** system SHALL register WorkflowExtension
- **AND** priority SHALL be 70 (after basic markdown)
- **AND** SHALL implement SyntaxExtension trait
- **AND** SHALL return NoteContent::Workflow blocks

#### Scenario: Register ToonExtension
- **GIVEN** parser initialization
- **WHEN** extensions are registered
- **THEN** system SHALL register ToonExtension
- **AND** priority SHALL be 75 (after workflow)
- **AND** SHALL parse only `session-toon` code blocks
- **AND** SHALL return NoteContent::SessionMarker blocks

#### Scenario: Parse workflow document end-to-end
- **GIVEN** markdown document with workflows and session markers
- **WHEN** CrucibleParser processes document
- **THEN** system SHALL parse headings into workflow nodes
- **AND** SHALL extract data flows as edges
- **AND** SHALL parse session-toon blocks as session markers
- **AND** SHALL return ParsedNote with all content types
- **AND** workflow nodes SHALL reference session markers

#### Scenario: Handle parsing errors gracefully
- **GIVEN** workflow document with invalid syntax
- **WHEN** parser processes document
- **THEN** system SHALL collect parse errors
- **AND** SHALL continue parsing other content
- **AND** SHALL return partial workflow with error annotations
- **AND** errors SHALL include line numbers and context

### Requirement: CLI Commands for Workflows

The system SHALL provide CLI commands for executing workflows, viewing execution history, and querying session markers.

#### Scenario: Execute workflow via CLI
- **GIVEN** workflow document at `workflows/deploy.md`
- **WHEN** user runs `cru workflow run workflows/deploy.md`
- **THEN** system SHALL load and parse workflow
- **AND** SHALL validate DAG structure
- **AND** SHALL execute workflow from start node
- **AND** SHALL display progress in terminal
- **AND** SHALL write session markers to document

#### Scenario: View workflow execution history
- **GIVEN** workflow with multiple past executions
- **WHEN** user runs `cru workflow history workflows/deploy.md`
- **THEN** system SHALL query session markers for document
- **AND** SHALL display table of executions:
  - Execution time, status, duration, agent
- **AND** SHALL sort by timestamp descending
- **AND** SHALL support pagination for long histories

#### Scenario: Query session markers by filter
- **GIVEN** multiple workflows with session markers
- **WHEN** user runs `cru workflow sessions --agent architect --status failed`
- **THEN** system SHALL query sessions matching both filters
- **AND** SHALL display matching sessions across all workflows
- **AND** SHALL show: workflow name, phase, agent, status, error
- **AND** SHALL support JSON output format

#### Scenario: Validate workflow without execution
- **GIVEN** workflow document
- **WHEN** user runs `cru workflow validate workflows/deploy.md`
- **THEN** system SHALL parse workflow
- **AND** SHALL check for cycles in DAG
- **AND** SHALL verify all referenced nodes exist
- **AND** SHALL validate data flow type consistency
- **AND** SHALL report validation errors with line numbers

## CHANGED Requirements

(None - this is a new feature with no modifications to existing specs)

## REMOVED Requirements

(None - no existing functionality removed)

## Dependencies

### Internal Dependencies
- `crucible-parser` - Extension system for workflow and toon parsing
- `crucible-core` - Domain types for workflows and session markers
- `crucible-db` - Storage and indexing for session markers
- `crucible-pipeline` - Process workflow documents like other markdown

### External Dependencies
- `toon-format = "0.1"` - Parse and encode toon-format data
- `petgraph = "0.6"` OR `daggy = "0.8"` - DAG data structure
- `serde = "1.0"` - Serialization for workflow structures

## Open Questions

1. **DAG Library**: Should we use `petgraph` (full-featured, larger) or `daggy` (simpler, focused on DAGs)?
   - **Recommendation**: Start with `daggy` for simplicity, migrate to `petgraph` if advanced algorithms needed

2. **Execution Model**: Should workflow execution be synchronous or async by default?
   - **Recommendation**: Async by default to support parallel branches and agent invocations

3. **Session Marker Mutability**: Should session markers be append-only or allow updates?
   - **Recommendation**: Append-only for audit integrity, new marker for status updates

4. **Type System**: Should we enforce type checking for data flow (e.g., `::Config` → `::SQL`)?
   - **Recommendation**: Optional type hints only, no runtime enforcement in v1

5. **Cross-Document References**: Should workflows reference steps in other documents (e.g., `[[Deploy]]#Phase2`)?
   - **Recommendation**: Defer to v2, start with single-document workflows

## Future Enhancements

### Advanced Flow Control
- Loop constructs: `while condition → step`
- Error handlers: `on_error → fallback_step`
- Timeout specifications: `step @agent timeout:30s`

### Workflow Composition
- Reference workflows: `[[Shared Workflow]]` as sub-workflow
- Parameterized workflows: `workflow(env: production)`
- Workflow templates: Fill in variables before execution

### Visualization
- Render DAG as Mermaid diagram in CLI
- Generate SVG/PNG workflow diagrams
- Show execution progress in real-time

### Agent-Generated Workflows
- "Create deployment workflow for this project"
- Agent writes workflow markdown with proper markup
- Interactive workflow builder in future GUI
