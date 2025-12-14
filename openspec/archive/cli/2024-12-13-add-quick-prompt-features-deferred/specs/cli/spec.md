## ADDED Requirements

### Requirement: Shell Integration Script Generation

The CLI SHALL provide a command to generate shell integration scripts for zsh and bash that enable quick prompts in the user's shell.

#### Scenario: Generate zsh integration script
- **WHEN** user runs `crucible shell-integration zsh`
- **THEN** the CLI outputs a shell script for zsh
- **AND** the script can be sourced in `.zshrc` via `eval "$(crucible shell-integration zsh)"`
- **AND** the script sets up ZLE widgets for Tab/Enter handling
- **AND** the script defines prompt mode toggle functionality

#### Scenario: Generate bash integration script
- **WHEN** user runs `crucible shell-integration bash`
- **THEN** the CLI outputs a shell script for bash
- **AND** the script can be sourced in `.bashrc` via `eval "$(crucible shell-integration bash)"`
- **AND** the script sets up readline bindings for Tab/Enter handling
- **AND** the script defines prompt mode toggle functionality

#### Scenario: Integration script is idempotent
- **WHEN** user sources the integration script multiple times
- **THEN** the script does not create duplicate keybindings
- **AND** does not cause shell errors
- **AND** safely redefines functions and variables

---

### Requirement: Shell Prompt Mode Toggle

The shell integration script SHALL support a prompt mode that can be toggled via Tab key at the start of a line in the user's shell.

#### Scenario: Toggle prompt mode with Tab in zsh
- **WHEN** user presses Tab at the beginning of a line in zsh (empty LBUFFER or cursor at start)
- **THEN** the shell integration toggles prompt mode on/off
- **AND** updates the shell prompt to show visual indicator (e.g., `[crucible] $`)
- **AND** shows a brief message indicating mode state

#### Scenario: Toggle prompt mode with Tab in bash
- **WHEN** user presses Tab at the beginning of a line in bash
- **THEN** the shell integration toggles prompt mode on/off
- **AND** updates the shell prompt to show visual indicator
- **AND** shows a brief message indicating mode state

#### Scenario: Tab at middle of line performs completion
- **WHEN** user presses Tab when cursor is not at the beginning of the line
- **THEN** the shell performs normal tab completion
- **AND** does not toggle prompt mode

---

### Requirement: Quick Prompt Execution Command

The CLI SHALL provide a `quick-prompt` subcommand that executes quick prompt triggers from shell integration scripts.

#### Scenario: Execute note creation trigger
- **WHEN** shell integration calls `crucible quick-prompt "note: Meeting notes"`
- **THEN** the CLI recognizes the `note:` prefix
- **AND** routes to the note creation handler
- **AND** creates a note with the content "Meeting notes"
- **AND** outputs a confirmation message to stdout

#### Scenario: Execute agent prompt trigger
- **WHEN** shell integration calls `crucible quick-prompt "agent: explain CRDTs"`
- **THEN** the CLI recognizes the `agent:` prefix
- **AND** routes to the agent handler
- **AND** sends the prompt to the agent
- **AND** streams the agent response to stdout

#### Scenario: Unknown trigger falls back to agent
- **WHEN** shell integration calls `crucible quick-prompt "unknown: content"`
- **THEN** the CLI does not match any known trigger prefix
- **AND** routes the entire input to the agent handler
- **AND** sends "unknown: content" as the agent prompt

---

### Requirement: Shell Enter Handler Routing

The shell integration script SHALL route Enter key presses differently based on prompt mode state, sending prompt mode input to Crucible instead of executing as shell commands.

#### Scenario: Enter in prompt mode routes to Crucible
- **WHEN** user presses Enter while in prompt mode
- **THEN** the shell integration captures the input buffer
- **AND** calls `crucible quick-prompt "$input"`
- **AND** displays the output from Crucible
- **AND** does not execute input as a shell command
- **AND** resets prompt mode

#### Scenario: Enter in normal mode executes normally
- **WHEN** user presses Enter while not in prompt mode
- **THEN** the shell executes input as normal shell command
- **AND** maintains existing shell behavior

---

### Requirement: Visual Prompt Indicator

The shell integration script SHALL modify the shell prompt to indicate prompt mode state.

#### Scenario: Prompt indicator in zsh
- **WHEN** prompt mode is active in zsh
- **THEN** the shell prompt displays `[crucible]` prefix
- **AND** uses PS1 or precmd hook to update prompt
- **AND** updates immediately on mode toggle

#### Scenario: Prompt indicator in bash
- **WHEN** prompt mode is active in bash
- **THEN** the shell prompt displays `[crucible]` prefix
- **AND** uses PS1 to update prompt
- **AND** updates immediately on mode toggle

---

### Requirement: Trigger Registry System

The CLI SHALL provide an extensible trigger registry system that matches prefix patterns and routes to appropriate handlers.

#### Scenario: Register built-in triggers
- **WHEN** the CLI initializes the trigger registry
- **THEN** it registers built-in triggers: `note:`, `agent:`, `search:`
- **AND** each trigger is associated with a handler function
- **AND** triggers are matched by prefix (case-sensitive)

#### Scenario: Match trigger prefix
- **WHEN** quick-prompt receives input "note: content"
- **THEN** the trigger registry matches the `note:` prefix
- **AND** extracts the content after the prefix ("content")
- **AND** routes to the note creation handler

#### Scenario: No prefix match falls back to agent
- **WHEN** quick-prompt receives input without a recognized prefix
- **THEN** the trigger registry does not match any prefix
- **AND** routes the entire input to the agent handler

---

### Requirement: Note Creation Trigger

The CLI SHALL provide a `note:` trigger that creates a new note in the kiln.

#### Scenario: Create note with note: trigger
- **WHEN** user executes `crucible quick-prompt "note: Meeting notes from standup"`
- **THEN** the CLI creates a new note with title derived from content
- **AND** saves the note to the kiln
- **AND** outputs confirmation with note ID or path

#### Scenario: Note creation with empty content
- **WHEN** user executes `crucible quick-prompt "note: "`
- **THEN** the CLI creates a note with empty content
- **AND** uses a default title or timestamp-based title

---

### Requirement: Agent Query Trigger

The CLI SHALL provide an `agent:` trigger that sends a prompt directly to the configured agent.

#### Scenario: Query agent with agent: trigger
- **WHEN** user executes `crucible quick-prompt "agent: explain CRDTs"`
- **THEN** the CLI sends "explain CRDTs" to the default agent
- **AND** streams the agent response to stdout
- **AND** handles streaming tokens appropriately for shell output

#### Scenario: Agent query without prefix falls back to agent
- **WHEN** user executes `crucible quick-prompt "explain CRDTs"` (no prefix)
- **THEN** the CLI treats the entire input as an agent prompt
- **AND** sends "explain CRDTs" to the agent
- **AND** streams the response

---

### Requirement: Search Trigger

The CLI SHALL provide a `search:` trigger that performs semantic search on the kiln.

#### Scenario: Search with search: trigger
- **WHEN** user executes `crucible quick-prompt "search: CRDT implementation"`
- **THEN** the CLI performs semantic search for "CRDT implementation"
- **AND** displays top search results with titles and relevance
- **AND** formats results for shell output

#### Scenario: Search with no results
- **WHEN** user executes `crucible quick-prompt "search: nonexistent term"`
- **THEN** the CLI performs the search
- **AND** displays a message indicating no results found
