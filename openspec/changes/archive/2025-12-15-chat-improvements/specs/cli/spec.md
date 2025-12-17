# CLI Specification

## ADDED Requirements

### Requirement: Clean Chat Output Display

The CLI SHALL display chat messages and responses with minimal visual noise, removing decorative boxes and excessive formatting.

#### Scenario: Clean startup message
- **WHEN** user starts a chat session
- **THEN** the CLI displays startup information without border boxes
- **AND** uses simple line separators or spacing instead
- **AND** provides essential information only (agent, mode, context count)

#### Scenario: Clean agent response display
- **WHEN** agent sends a response
- **THEN** the CLI displays the response without decorative borders
- **AND** uses subtle formatting (indentation, spacing) for visual separation
- **AND** maintains readability with appropriate line breaks

---

### Requirement: In-Chat Semantic Search

The CLI SHALL provide a `/search` command within chat sessions for manual semantic search queries.

#### Scenario: Manual search command
- **WHEN** user types `/search rust programming`
- **THEN** the CLI performs semantic search on the kiln
- **AND** displays top results with titles and relevance scores
- **AND** formats results for easy reading in chat context

#### Scenario: Search with result limit
- **WHEN** user types `/search rust --limit 3`
- **THEN** the CLI returns only the top 3 results
- **AND** respects the user-specified limit

---

### Requirement: File Reference System with @-mentions

The CLI SHALL support file references using `@` prefix with fuzzy selection for integrating files into chat context.

#### Scenario: File reference with fuzzy selection
- **WHEN** user types `@` in chat input
- **THEN** the CLI displays a fuzzy searchable list of files
- **AND** allows user to select from kiln files or current directory
- **AND** inserts the selected file reference into the message

#### Scenario: Context enrichment with referenced files
- **WHEN** user sends a message with `@filename.md` references
- **THEN** the CLI includes the full content of referenced files in agent context
- **AND** prioritizes referenced files in context enrichment
- **AND** indicates which files were referenced to the user

#### Scenario: Multiple file references
- **WHEN** user includes multiple `@` references in one message
- **THEN** the CLI includes all referenced files in context
- **AND** manages context size by prioritizing recent or mentioned files

---

### Requirement: Transparent Tool and Edit Display

The CLI SHALL show tool calls and file edits in real-time during agent execution for better transparency.

#### Scenario: Display tool calls
- **WHEN** agent executes a tool call
- **THEN** the CLI displays the tool name and parameters
- **AND** shows execution status (running, completed, failed)
- **AND** displays results in a clean, readable format

#### Scenario: Display file edits
- **WHEN** agent modifies a file in auto-approve or act mode
- **THEN** the CLI shows the file path and operation type
- **AND** displays a brief summary of changes
- **AND** indicates success or failure of the edit operation

---

### Requirement: Auto-Approve Mode

The CLI SHALL provide an auto-approve mode that automatically approves safe operations without user confirmation prompts.

#### Scenario: Enable auto-approve mode
- **WHEN** user enables auto-approve mode
- **THEN** the CLI automatically approves read operations within kiln
- **AND** automatically approves safe write operations (new files, edits to owned files)
- **AND** displays a clear indicator that auto-approve is active

#### Scenario: Auto-approve safety boundaries
- **WHEN** agent requests potentially dangerous operation in auto-approve mode
- **THEN** the CLI still prompts for confirmation for high-risk operations
- **AND** defines safe operations (reads, edits within kiln, non-executable files)
- **AND** blocks or prompts for system files, executables, or cross-directory operations

---

### Requirement: Seamless Mode Switching

The CLI SHALL support Shift+Tab hotkey for cycling through modes (plan ↔ normal ↔ auto-approve) during active chat sessions.

#### Scenario: Hotkey mode switching
- **WHEN** user presses Shift+Tab during chat
- **THEN** the CLI cycles to the next mode in sequence
- **AND** updates the prompt indicator to show current mode
- **AND** provides brief feedback about the mode change

#### Scenario: Mode switching behavior
- **WHEN** switching from plan to normal mode
- **THEN** the CLI enables standard agent capabilities
- **AND** when switching to auto-approve, enables automatic operation approvals
- **AND** maintains conversation context across mode switches

---

### Requirement: Enhanced User Experience Features

The CLI SHALL provide additional UX improvements for better chat usability and productivity.

#### Scenario: Command history search
- **WHEN** user presses Ctrl+R in chat
- **THEN** the CLI displays searchable command history
- **AND** allows selection and execution of previous commands

#### Scenario: Multiline input support
- **WHEN** user presses Ctrl+J during message composition
- **THEN** the CLI inserts a newline without sending the message
- **AND** allows multiline message composition
- **AND** sends message only on Enter

#### Scenario: Session file tracking
- **WHEN** user types `/files`
- **THEN** the CLI lists all files modified during the current session
- **AND** shows file paths without timestamps
- **AND** indicates which files were created vs modified

#### Scenario: Context information display
- **WHEN** user types `/context`
- **THEN** the CLI shows current context usage (tokens/limit)
- **AND** lists all files currently included in agent context
- **AND** shows context sources (search results vs @ references)

#### Scenario: Unified mode switching
- **WHEN** user types `/mode plan|normal|auto`
- **THEN** the CLI switches to the specified mode
- **AND** updates the prompt indicator
- **AND** provides feedback about the mode change

#### Scenario: Agent switching
- **WHEN** user types `/agent [agent-name]`
- **THEN** the CLI switches to the specified agent in the same session
- **AND** maintains conversation context
- **AND** shows agent switch confirmation

#### Scenario: Command fuzzy completion
- **WHEN** user types `/` at the beginning of input
- **THEN** the CLI displays fuzzy-matching list of available commands
- **AND** allows selection with Tab or arrow keys
- **AND** shows brief command descriptions

#### Scenario: Session management commands
- **WHEN** user types `/clear`
- **THEN** the CLI clears conversation context while maintaining session
- **AND** when user types `/reset`
- **THEN** the CLI starts fresh with same agent but empty context
- **AND** when user types `/export`
- **THEN** the CLI saves conversation to markdown file

#### Scenario: Operation undo in auto-approve
- **WHEN** user types `/undo` in auto-approve mode
- **THEN** the CLI reverses the last file operation
- **AND** provides confirmation of what was undone
- **AND** maintains session context

#### Scenario: Keyboard shortcuts
- **WHEN** user uses common keyboard shortcuts
- **THEN** the CLI supports Ctrl+C for interruption, Ctrl+D for exit
- **AND** supports Tab completion for commands and file paths
- **AND** provides help with `/help`

---

## MODIFIED Requirements

### Requirement: Enhanced Chat Session Display

The CLI SHALL update the chat session initialization to incorporate clean display principles and new interactive features.

#### Scenario: Updated chat session initialization
- **WHEN** user starts chat with `cru chat`
- **THEN** the CLI SHALL display minimal startup information without decorative boxes
- **AND** SHALL show a spinner during background processing instead of progress bar
- **AND** SHALL display a counting number as changed files are detected (Phase 1)
- **AND** SHALL show fractional progress (e.g., "5/123") for files going through the pipeline
- **AND** SHALL show current mode, agent, and available commands after processing completes
- **AND** SHALL display keyboard shortcut hints for new features

#### Scenario: Enhanced context enrichment feedback
- **WHEN** context enrichment includes file references
- **THEN** the CLI SHALL indicate which files were explicitly referenced via `@`
- **AND** SHALL provide feedback about context sources without showing detailed token usage
- **AND** SHALL prioritize user-initiated file references in context optimization