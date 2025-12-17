## ADDED Requirements

### Requirement: ChatAgent Trait
The system SHALL define a ChatAgent trait that any AI backend can implement for use in chat sessions.

#### Scenario: Send message to agent
- **WHEN** ChatSession calls send_message on ChatAgent
- **THEN** agent SHALL process message and return response
- **AND** response includes content and optional tool calls

#### Scenario: Set agent mode
- **WHEN** ChatSession calls set_mode with new mode
- **THEN** agent SHALL update permissions accordingly
- **AND** plan mode SHALL restrict to read-only operations
- **AND** act mode SHALL allow write operations

### Requirement: ChatSession Orchestration
The system SHALL provide ChatSession that manages conversation flow independent of agent backend.

#### Scenario: Start interactive session
- **WHEN** user runs chat command without query
- **THEN** ChatSession SHALL enter interactive mode
- **AND** display prompt for user input
- **AND** continue until user exits

#### Scenario: Handle mode cycling
- **WHEN** user presses Shift+Tab or types /mode
- **THEN** ChatSession SHALL cycle to next mode
- **AND** update prompt indicator
- **AND** notify agent of mode change

### Requirement: Slash Command Handling
The system SHALL parse and execute slash commands within chat sessions.

#### Scenario: Execute search command
- **WHEN** user types /search query
- **THEN** ChatSession SHALL perform semantic search
- **AND** display results without sending to agent

#### Scenario: Execute mode commands
- **WHEN** user types /plan, /act, or /auto
- **THEN** ChatSession SHALL switch to requested mode
- **AND** update visual indicators
