# agent-system Specification

## Purpose
TBD - created by archiving change add-agent-system. Update Purpose after archive.
## Requirements
### Requirement: Agent Card Definition Format
The system SHALL support agent card definitions as markdown files with YAML frontmatter, enabling users to define reusable agent workflows.

#### Scenario: Create project-specific agent card
- **WHEN** user creates `.crucible/agents/code-reviewer.md` with valid frontmatter
- **THEN** system SHALL discover agent on next CLI invocation
- **AND** agent SHALL be available for query matching
- **AND** agent SHALL appear in `cru agents list` output

#### Scenario: Agent card with required frontmatter
- **WHEN** agent card includes name, description, and keywords in frontmatter
- **THEN** system SHALL parse and validate all required fields
- **AND** system SHALL extract markdown content as agent instructions
- **AND** agent SHALL be registered for use

#### Scenario: Agent card with ACP delegation
- **WHEN** agent card includes `acp_server: claude-code` in frontmatter
- **THEN** system SHALL recognize this as external execution target
- **AND** matched queries SHALL delegate to specified ACP agent
- **AND** ACP unavailability SHALL produce clear error message

### Requirement: Agent Registry and Discovery
The system SHALL automatically discover agent cards from configured directories, providing listing and validation capabilities.

#### Scenario: Automatic discovery at startup
- **WHEN** CLI initializes
- **THEN** system SHALL scan `~/.config/crucible/agents/` for system agents
- **AND** system SHALL scan `.crucible/agents/` for project agents
- **AND** valid agents SHALL be registered and available
- **AND** invalid agents SHALL log warnings without preventing startup

#### Scenario: Project agents override system agents
- **WHEN** project agent has same name as system agent
- **THEN** project agent SHALL take precedence
- **AND** `cru agents list` SHALL indicate source (project/system)
- **AND** matching SHALL use project version

### Requirement: Agent Query Matching
The system SHALL match user queries to agent cards using keyword matching, returning ranked results to help users find appropriate agents.

#### Scenario: Match query by keywords
- **WHEN** user query contains keywords from agent card
- **THEN** system SHALL return agents ranked by keyword relevance
- **AND** results SHALL include similarity scores
- **AND** exact matches SHALL rank higher than partial matches

#### Scenario: Multiple agents match query
- **WHEN** query matches multiple agent cards
- **THEN** system SHALL return all matches sorted by relevance
- **AND** each result SHALL include similarity score
- **AND** user can select from ranked options

### Requirement: CLI Agent Management
The system SHALL provide commands for discovering, inspecting, and validating agent cards.

#### Scenario: List all agents
- **WHEN** user runs `cru agents list`
- **THEN** output SHALL show agent names, descriptions, and sources
- **AND** output SHALL indicate ACP delegation targets if specified
- **AND** disabled agents SHALL NOT appear in list

#### Scenario: Show agent details
- **WHEN** user runs `cru agents show <name>`
- **THEN** output SHALL display full frontmatter metadata
- **AND** output SHALL show agent instructions
- **AND** output SHALL indicate source file path

#### Scenario: Validate agents
- **WHEN** user runs `cru agents validate`
- **THEN** system SHALL check all agent definitions for validity
- **AND** system SHALL report errors with file paths
- **AND** system SHALL exit with error code if any invalid

