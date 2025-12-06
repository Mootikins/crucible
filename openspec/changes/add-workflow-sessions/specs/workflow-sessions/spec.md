# Workflow Sessions Specification

## Overview

This specification defines session logging, resumption, and codification for Crucible. Sessions are logged as readable markdown with structured frontmatter, enabling resumption, pattern extraction, and learning from agent interactions.

## ADDED Requirements

### Requirement: Session Log Format

The system SHALL log agent sessions as markdown documents with structured frontmatter and heading-per-phase organization.

#### Scenario: Create new session log
- **GIVEN** agent starts new work session
- **WHEN** session begins
- **THEN** system SHALL create markdown file with session frontmatter
- **AND** frontmatter SHALL include: session_id, started, status, channel
- **AND** status SHALL be set to "active"
- **AND** file SHALL be created in configured sessions directory

#### Scenario: Log phase with agent and channel
- **GIVEN** active session log
- **WHEN** agent starts new phase "Research" in channel "dev"
- **THEN** system SHALL append heading `## Research @agent-name #dev`
- **AND** heading level SHALL be 2 for top-level phases
- **AND** agent name SHALL include version if known
- **AND** timestamp SHALL be recorded

#### Scenario: Log tool calls within phase
- **GIVEN** active phase in session
- **WHEN** agent makes tool calls
- **THEN** system SHALL append tool call list:
  ```markdown
  **Tool calls:**
  - `tool_name args` → result_summary
  ```
- **AND** result summary SHALL be truncated if over 100 chars
- **AND** failures SHALL be marked with error indicator

#### Scenario: Log decision callout
- **GIVEN** agent makes significant decision
- **WHEN** decision is recorded
- **THEN** system SHALL append decision callout:
  ```markdown
  > [!decision]
  > Decision description here.
  ```
- **AND** callout SHALL be placed after relevant context

#### Scenario: Log error callout
- **GIVEN** agent encounters error
- **WHEN** error is handled
- **THEN** system SHALL append error callout:
  ```markdown
  > [!error]
  > Error description and how it was handled.
  ```
- **AND** callout SHALL include resolution if available

#### Scenario: Log user intervention
- **GIVEN** human provides input during session
- **WHEN** input is received
- **THEN** system SHALL append user callout:
  ```markdown
  > [!user]
  > "Exact user input" - interpretation/action taken.
  ```
- **AND** user text SHALL be quoted

### Requirement: Subagent Session Handling

The system SHALL support three modes for logging nested agent work: inline, link, and embed.

#### Scenario: Log subagent inline
- **GIVEN** subagent mode is "inline"
- **WHEN** subagent completes work
- **THEN** system SHALL append subagent callout with full content:
  ```markdown
  > [!subagent] @agent@version (model)
  > mode: inline
  > [full subagent work here]
  ```
- **AND** subagent content SHALL be indented within callout

#### Scenario: Log subagent with link
- **GIVEN** subagent mode is "link"
- **WHEN** subagent starts work
- **THEN** system SHALL create separate session file
- **AND** system SHALL append subagent callout with wikilink:
  ```markdown
  > [!subagent] @agent@version (model)
  > mode: link
  > [[session-id]]
  > Summary: Brief summary of work done.
  ```
- **AND** parent session frontmatter SHALL list subagent in `subagents` array

#### Scenario: Log subagent with embed
- **GIVEN** subagent mode is "embed"
- **WHEN** subagent completes work
- **THEN** system SHALL create separate session file
- **AND** system SHALL append subagent callout with transclusion:
  ```markdown
  > [!subagent] @agent@version (model)
  > mode: embed
  > ![[session-id]]
  ```
- **AND** rendering SHALL show embedded content inline

### Requirement: Session Frontmatter Schema

The system SHALL maintain structured frontmatter with session metadata.

#### Scenario: Required frontmatter fields
- **GIVEN** new session file
- **WHEN** frontmatter is created
- **THEN** frontmatter SHALL include:
  - `type: session`
  - `session_id: <unique-id>`
  - `started: <ISO-8601-timestamp>`
  - `status: active`
- **AND** session_id SHALL be unique within vault

#### Scenario: Track participants
- **GIVEN** agents participating in session
- **WHEN** agent is added
- **THEN** frontmatter `participants` array SHALL include agent reference
- **AND** reference format SHALL be `name@version`
- **AND** primary model SHALL be stored in `model` field

#### Scenario: Track token usage
- **GIVEN** session with LLM interactions
- **WHEN** tokens are used
- **THEN** frontmatter `tokens_used` SHALL be updated
- **AND** per-subagent tokens SHALL be tracked in subagents array

#### Scenario: Track subagent sessions
- **GIVEN** session spawns subagent
- **WHEN** subagent session is created
- **THEN** parent frontmatter `subagents` array SHALL include:
  - `session: <subagent-session-id>`
  - `agent: <agent@version>`
  - `model: <model-used>`
  - `tokens: <tokens-used>`

### Requirement: Session Resumption

The system SHALL support resuming paused or incomplete sessions.

#### Scenario: Pause session
- **GIVEN** active session
- **WHEN** user pauses or session times out
- **THEN** system SHALL set status to "paused"
- **AND** SHALL set `resume_point` to current phase ID
- **AND** SHALL save session file

#### Scenario: Resume session
- **GIVEN** paused session file
- **WHEN** user runs `cru session resume <session-file>`
- **THEN** system SHALL load session markdown
- **AND** SHALL parse phases up to resume_point
- **AND** SHALL reconstruct agent context from logged content
- **AND** SHALL continue with agent at resume_point

#### Scenario: Resume with context limit
- **GIVEN** long session exceeding context window
- **WHEN** resuming
- **THEN** system SHALL apply context strategy (sliding window)
- **AND** SHALL prioritize recent phases and decisions
- **AND** SHALL preserve all `[!decision]` callouts

### Requirement: Workflow Codification

The system SHALL extract workflow definitions from session logs.

#### Scenario: Auto-extract workflow
- **GIVEN** completed session with multiple phases
- **WHEN** user runs `cru session codify <session-file>`
- **THEN** system SHALL parse phases as workflow steps
- **AND** SHALL extract `@agent` and `#channel` from headings
- **AND** SHALL infer data flow from tool call patterns
- **AND** SHALL output workflow-markup format

#### Scenario: Agent-refined codification
- **GIVEN** auto-extracted workflow
- **WHEN** user runs `cru session codify --refine`
- **THEN** system SHALL pass session and extract to agent
- **AND** agent SHALL clean up and improve workflow
- **AND** agent SHALL add missing type annotations
- **AND** agent SHALL clarify conditional flows

#### Scenario: Interactive codification
- **GIVEN** refined workflow
- **WHEN** user runs `cru session codify --interactive`
- **THEN** system SHALL present workflow in editor/pager
- **AND** user SHALL review and modify
- **AND** system SHALL validate modified workflow
- **AND** system SHALL save to specified output file

### Requirement: RL Case Generation

The system SHALL generate learning cases from session logs for training and improvement.

#### Scenario: Generate puzzle from failure
- **GIVEN** session with `[!error]` callouts
- **WHEN** user runs `cru session learn --type puzzle`
- **THEN** system SHALL extract failure context
- **AND** SHALL format as puzzle scenario
- **AND** puzzle SHALL include: context, failure, expected behavior
- **AND** puzzle SHALL be usable for prompt engineering

#### Scenario: Generate from user intervention
- **GIVEN** session with `[!user]` corrections
- **WHEN** generating learning cases
- **THEN** system SHALL extract before/after context
- **AND** SHALL format as correction example
- **AND** example SHALL show: agent behavior, user correction, correct behavior

#### Scenario: Export training data
- **GIVEN** multiple learning cases
- **WHEN** user runs `cru session learn --export <format>`
- **THEN** system SHALL support formats: jsonl, markdown, puzzle
- **AND** SHALL include metadata for filtering
- **AND** SHALL be usable for LoRA training

### Requirement: TOON Index Derivation

The system SHALL derive TOON format index from session markdown for efficient querying.

#### Scenario: Index session on save
- **GIVEN** session file saved
- **WHEN** indexing runs
- **THEN** system SHALL extract structured data from session
- **AND** SHALL encode as TOON format
- **AND** SHALL store in index for fast queries

#### Scenario: Query sessions by agent
- **GIVEN** indexed sessions
- **WHEN** user queries by agent
- **THEN** system SHALL use TOON index for fast lookup
- **AND** SHALL return matching sessions
- **AND** SHALL include session metadata

#### Scenario: Aggregate session metrics
- **GIVEN** multiple indexed sessions
- **WHEN** user requests aggregations
- **THEN** system SHALL compute: total tokens, avg duration, failure rate
- **AND** SHALL group by: agent, channel, status
- **AND** SHALL return efficiently from TOON index

### Requirement: CLI Commands

The system SHALL provide CLI commands for session management.

#### Scenario: List sessions
- **GIVEN** sessions directory with session files
- **WHEN** user runs `cru session list`
- **THEN** system SHALL list sessions with: id, status, started, participants
- **AND** SHALL support filtering by status, agent, channel
- **AND** SHALL support JSON output format

#### Scenario: Show session details
- **GIVEN** session file
- **WHEN** user runs `cru session show <session-id>`
- **THEN** system SHALL display session content
- **AND** SHALL render callouts with formatting
- **AND** SHALL show frontmatter metadata

#### Scenario: Resume session command
- **GIVEN** paused session
- **WHEN** user runs `cru session resume <session-id>`
- **THEN** system SHALL load and resume session
- **AND** SHALL display resume context
- **AND** SHALL continue interactive session

## CHANGED Requirements

(None - this is a new feature)

## REMOVED Requirements

(None - no existing functionality removed)

## Dependencies

### Internal Dependencies
- `crucible-core` - Session domain types
- `crucible-parser` - Parse session format
- `crucible-cli` - Session commands
- `workflow-markup` - Output format for codification

### External Dependencies
- `toon-format` - Derived index format
- Existing frontmatter parser

## Open Questions

1. **Task list location** - Where should in-progress task lists live during sessions?
   - Option A: Frontmatter `tasks` field
   - Option B: Dedicated `tasks` code block (JSON array)
   - Option C: Separate `.tasks.json` file (Claude Code approach)
   - **Research findings:**
     - Claude Code stores todos as JSON in `~/.claude/todos/` with fields: content, status, priority, id
     - OpenCode uses SQLite for session persistence, tracks tasks implicitly through conversation
     - Both use three states: pending, in_progress, completed
   - **Recommendation:** Option B - JSON code block in session markdown keeps tasks with context,
     readable, and parseable. Format: `{content, status, priority, id}`

2. **Session file naming** - What naming convention?
   - Option A: `YYYY-MM-DD-<slug>.md`
   - Option B: `<session-id>.md` (UUID)
   - Option C: `<channel>-<timestamp>.md`
   - Recommendation: A for human readability

3. **Auto-save threshold** - When to save during active session?
   - Option A: After each phase
   - Option B: After each tool call
   - Option C: Configurable interval/event
   - Recommendation: C with phase as default

4. **Subagent default mode** - What's the default for subagent logging?
   - Option A: inline (simple, but verbose)
   - Option B: link (clean, but scattered)
   - Option C: embed (best of both)
   - Recommendation: B for cleanliness, configurable

## Future Enhancements

### Rune Extensibility
- Custom callout handlers
- Custom codification workflows
- Federated session sync

### Advanced Learning
- Automatic puzzle generation pipeline
- LoRA training integration
- State graph extraction for Rune behaviors

### Collaboration
- Multi-user session support
- Session merging/branching
- Conflict resolution for concurrent edits
