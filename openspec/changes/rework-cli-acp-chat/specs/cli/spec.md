# CLI Specification

## ADDED Requirements

### Requirement: Natural Language Chat Interface

The CLI SHALL provide a natural language chat interface as the default mode, using the Agent Client Protocol (ACP) to communicate with external AI agents.

#### Scenario: User starts chat with default agent
- **WHEN** user runs `cru chat` without arguments
- **THEN** the CLI spawns the default agent (claude-code)
- **AND** initializes an ACP session
- **AND** displays a prompt for user input

#### Scenario: User asks question with kiln context
- **WHEN** user inputs a natural language query
- **THEN** the CLI performs semantic search on the kiln
- **AND** enriches the query with top-5 search results
- **AND** sends the enriched prompt to the agent via ACP
- **AND** streams the agent's response to the terminal

#### Scenario: User selects specific agent
- **WHEN** user runs `cru chat --agent gemini`
- **THEN** the CLI spawns the gemini-cli agent instead of default
- **AND** proceeds with chat session normally

#### Scenario: Agent not found
- **WHEN** user runs `cru chat --agent nonexistent`
- **THEN** the CLI displays an error message
- **AND** provides installation instructions for supported agents
- **AND** exits with non-zero status code

---

### Requirement: Pipeline Processing Command

The CLI SHALL provide an explicit command to run the NotePipeline orchestrator on files in the kiln.

#### Scenario: Process entire kiln
- **WHEN** user runs `cru process`
- **THEN** the CLI scans all markdown files in the kiln
- **AND** processes each file through the 5-phase pipeline
- **AND** displays a progress bar during processing
- **AND** outputs summary statistics upon completion

#### Scenario: Process single file
- **WHEN** user runs `cru process path/to/note.md`
- **THEN** the CLI processes only that specific file
- **AND** displays the processing result (success, skipped, or no changes)

#### Scenario: Force reprocessing
- **WHEN** user runs `cru process --force`
- **THEN** the CLI bypasses change detection (Phase 1)
- **AND** reprocesses all files regardless of modification status

#### Scenario: Watch mode
- **WHEN** user runs `cru process --watch`
- **THEN** the CLI starts a file watcher on the kiln directory
- **AND** automatically processes files when they are created or modified
- **AND** continues watching until user terminates (Ctrl+C)

---

### Requirement: Kiln Status Display

The CLI SHALL provide a status command that displays statistics about the kiln and processing state.

#### Scenario: Show basic status
- **WHEN** user runs `cru status`
- **THEN** the CLI displays:
  - Total markdown files
  - Notes indexed
  - Total blocks
  - Embeddings generated
  - Last processing time
  - Current processing state

#### Scenario: Show detailed status
- **WHEN** user runs `cru status --detailed`
- **THEN** the CLI additionally displays:
  - Per-file processing status
  - Embedding model information
  - Storage backend statistics
  - Recent file changes

---

### Requirement: Semantic Search Command

The CLI SHALL provide a quick semantic search command for querying the kiln without starting a chat session.

#### Scenario: Search with query
- **WHEN** user runs `cru search "rust programming"`
- **THEN** the CLI performs semantic search
- **AND** displays top results with titles and scores
- **AND** limits results to default count (10)

#### Scenario: Custom result limit
- **WHEN** user runs `cru search "topic" -n 20`
- **THEN** the CLI returns up to 20 results instead of default

#### Scenario: Show content snippets
- **WHEN** user runs `cru search "topic" --show-content`
- **THEN** the CLI includes content snippets in results
- **AND** highlights matching portions

---

### Requirement: Configuration Management

The CLI SHALL provide configuration commands for managing settings.

#### Scenario: Show current configuration
- **WHEN** user runs `cru config show`
- **THEN** the CLI displays all current configuration values
- **AND** indicates which values are defaults vs. user-configured

#### Scenario: Initialize new configuration
- **WHEN** user runs `cru config init`
- **THEN** the CLI creates a default config file at `~/.config/crucible/config.toml`
- **AND** prompts for key settings (kiln path, agent preference)

#### Scenario: Set configuration value
- **WHEN** user runs `cru config set kiln.path ~/Documents/notes`
- **THEN** the CLI updates the config file
- **AND** validates the new value before saving

---

### Requirement: Background Processing

The CLI SHALL automatically process the kiln in the background on startup unless disabled.

#### Scenario: Auto-process on startup
- **WHEN** user runs any CLI command
- **THEN** the CLI spawns a background task to process the kiln
- **AND** the command proceeds without waiting for processing
- **AND** processing errors do not block command execution

#### Scenario: Skip background processing
- **WHEN** user runs `cru --no-process status`
- **THEN** the CLI skips background processing
- **AND** executes the command immediately with potentially stale data

#### Scenario: Processing timeout
- **WHEN** background processing exceeds the configured timeout (default 300s)
- **THEN** the CLI terminates the processing task
- **AND** logs a warning message
- **AND** continues with partial results

---

### Requirement: Context Enrichment

The CLI SHALL enrich user queries with relevant context from the kiln before sending to the agent.

#### Scenario: Semantic search context
- **WHEN** user asks a question in chat mode
- **THEN** the CLI performs semantic search
- **AND** selects the top 5 most relevant notes
- **AND** formats them as markdown context
- **AND** prepends context to user query before sending to agent

#### Scenario: Empty search results
- **WHEN** semantic search returns no results
- **THEN** the CLI sends the query to the agent without context
- **AND** logs that no relevant context was found

---

### Requirement: Core Facade Interface

The CLI SHALL access all core functionality through a trait-based facade pattern for testability and modularity.

#### Scenario: Initialize facade from config
- **WHEN** the CLI starts up
- **THEN** it creates a `CrucibleCore` facade instance
- **AND** initializes all trait-based dependencies (pipeline, storage, search)
- **AND** validates that all required services are available

#### Scenario: Commands use facade exclusively
- **WHEN** any command needs to access core functionality
- **THEN** it SHALL use the facade interface
- **AND** it SHALL NOT directly import storage or pipeline implementations
- **AND** all dependencies are injected via the facade

---

### Requirement: Error Handling

The CLI SHALL provide clear, actionable error messages for common failure scenarios.

#### Scenario: Kiln path not found
- **WHEN** the configured kiln path does not exist
- **THEN** the CLI displays an error message
- **AND** suggests running `cru config set kiln.path <path>`
- **AND** exits with non-zero status code

#### Scenario: Database initialization failure
- **WHEN** SurrealDB fails to initialize
- **THEN** the CLI displays the database error
- **AND** suggests troubleshooting steps (check permissions, disk space)
- **AND** offers to reinitialize with `--reset-db` flag

#### Scenario: Agent spawn failure
- **WHEN** ACP agent fails to spawn (not installed, wrong path)
- **THEN** the CLI displays agent name and expected location
- **AND** provides installation command for the agent
- **AND** exits with non-zero status code

---

### Requirement: Performance Constraints

The CLI SHALL meet the following performance targets for responsive user experience.

#### Scenario: Startup time
- **WHEN** user runs any CLI command
- **THEN** the CLI SHALL display first output within 2 seconds
- **AND** background processing SHALL NOT block command execution

#### Scenario: Semantic search latency
- **WHEN** user runs `cru search <query>`
- **THEN** results SHALL be displayed within 1 second for kilns with <10,000 notes

#### Scenario: Chat responsiveness
- **WHEN** user sends a message in chat mode
- **THEN** the first response chunk SHALL appear within 2 seconds
- **AND** subsequent chunks SHALL stream with <100ms latency

---

### Requirement: ACP Client Implementation

The CLI SHALL implement the ACP `Client` trait to communicate with external agents.

#### Scenario: File read request from agent
- **WHEN** agent requests to read a file via `fs_read_text_file`
- **THEN** the CLI reads the file from the kiln
- **AND** returns the markdown content to the agent

#### Scenario: File write request from agent
- **WHEN** agent requests to write a file via `fs_write_text_file`
- **THEN** the CLI writes the content to the kiln
- **AND** triggers pipeline processing on the modified file

#### Scenario: Session update from agent
- **WHEN** agent sends a `session_update` message
- **THEN** the CLI processes the update based on type:
  - MessageChunk: print to stdout
  - Thought: prefix with 💭 emoji
  - ToolCall: prefix with 🔧 emoji
  - Done: print newline and complete

#### Scenario: Permission request
- **WHEN** agent requests permission for an operation
- **THEN** the CLI auto-approves read operations (MVP)
- **AND** auto-approves write operations to kiln directory
- **AND** denies operations outside kiln directory

---

### Requirement: Graceful Degradation

The CLI SHALL continue operating with reduced functionality when non-critical components fail.

#### Scenario: Embedding service unavailable
- **WHEN** embedding service is not configured or fails
- **THEN** the CLI continues processing files
- **AND** skips Phase 4 (enrichment) for affected files
- **AND** logs a warning about missing embeddings
- **AND** semantic search returns empty results

#### Scenario: Background processing failure
- **WHEN** background processing encounters an error
- **THEN** the CLI logs the error
- **AND** continues executing the user's command
- **AND** displays a warning that data may be stale

---

### Requirement: Migration from REPL

The CLI SHALL provide guidance for users migrating from the old SurrealQL REPL mode.

#### Scenario: REPL deprecation warning
- **WHEN** a user attempts to run old REPL commands
- **THEN** the CLI displays a deprecation notice
- **AND** suggests equivalent chat or search commands
- **AND** provides link to migration guide

#### Scenario: Migration documentation
- **WHEN** the CLI package is installed
- **THEN** it SHALL include a MIGRATION.md document
- **AND** the document SHALL map old REPL patterns to new commands
- **AND** provide examples of natural language equivalents
