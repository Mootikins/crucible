## ADDED Requirements

### Requirement: ACP Client Communication
The system SHALL communicate with external ACP agents using the Agent Client Protocol for natural language interactions.

#### Scenario: Connect to external agent
- **WHEN** user starts chat session
- **THEN** system SHALL discover and connect to available ACP agent
- **AND** support claude-code, gemini-cli, and codex agents
- **AND** provide clear error if no agent available

#### Scenario: Stream agent responses
- **WHEN** agent generates response
- **THEN** system SHALL stream response chunks to user in real-time
- **AND** display tool calls as they occur
- **AND** handle connection interruptions gracefully

### Requirement: Context Enrichment
The system SHALL automatically enrich user queries with relevant knowledge base content before sending to agent.

#### Scenario: Enrich query with semantic search
- **WHEN** user asks question in chat
- **THEN** system SHALL perform semantic search for relevant notes
- **AND** include top N results (configurable, default 5)
- **AND** format context as markdown in agent prompt

#### Scenario: Skip enrichment when requested
- **WHEN** user provides `--no-context` flag
- **THEN** system SHALL send query without enrichment
- **AND** agent receives only user's original message

### Requirement: Tool Exposure via MCP
The system SHALL expose knowledge base tools to agents via Model Context Protocol.

#### Scenario: Agent uses read_note tool
- **WHEN** agent calls read_note with file path
- **THEN** system SHALL return note content
- **AND** handle missing files with clear error

#### Scenario: Agent uses semantic_search tool
- **WHEN** agent calls semantic_search with query
- **THEN** system SHALL return ranked results
- **AND** include similarity scores and snippets
