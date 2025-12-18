# CLI Capability Delta: TUI Fuzzy Search / Command Palette

## ADDED Requirements

### Requirement: Inline Popup Triggers (REQ-CLI-TUI-SEARCH-001)
The TUI SHALL open a popup above the prompt when the input begins with a trigger and support inline fuzzy filtering without leaving the prompt.

#### Scenario: Slash trigger
- Given the TUI is running
- When the user types `/` at the start of the input
- Then a popup appears above the prompt showing slash commands
- And typing refines the list with fuzzy matching within 200ms per keystroke

#### Scenario: At trigger for agents/files
- Given the TUI is running
- When the user types `@` at the start of the input
- Then a popup appears above the prompt showing agents and workspace/kiln file references
- And typing refines the list with fuzzy matching within 200ms per keystroke

#### Scenario: Navigation and confirm
- Given the popup is visible
- When the user presses Up/Down or Ctrl+P/Ctrl+N
- Then the highlighted row moves accordingly (wrap allowed)
- And pressing Enter or Tab selects the highlighted row; ESC closes without selection

### Requirement: Multi-Source Results (REQ-CLI-TUI-SEARCH-002)
The popup SHALL search across slash commands, agents, and files/notes using short, LLM-friendly identifiers.

#### Scenario: Commands appear
- Given client and agent-provided slash commands exist
- When the user types part of a command name or hint
- Then matching commands appear with their description and input/secondary hints

#### Scenario: Agents appear
- Given multiple agents are available locally
- When the user types part of an agent name
- Then matching agents appear with their display names/descriptions

#### Scenario: Files and notes
- Given files exist in the launch workspace and notes exist in a kiln
- When the user types part of a filename or note path
- Then matching workspace files appear as bare relative paths (no `file:` prefix)
- And matching kiln notes appear as `note:<path>` when a single/default kiln is active
- And when multiple kilns are configured, matching kiln notes appear as `note:<kiln>/<path>` (e.g., `note:main/project/foo.md`)

### Requirement: Palette Selection Actions (REQ-CLI-TUI-SEARCH-003)
Selecting a result SHALL perform a type-appropriate action and provide inline feedback.

#### Scenario: Execute slash command
- Given a command result is highlighted
- When the user confirms selection
- Then the command is executed (respecting namespacing/agent commands) and feedback is shown

#### Scenario: Switch to agent
- Given an agent result is highlighted
- When the user confirms selection
- Then the TUI switches/initiates chat with that agent and confirms the change

#### Scenario: File/note selection feedback
- Given a file or note result is highlighted
- When the user confirms selection
- Then the TUI inserts the reference token into the input buffer (workspace: bare relative path; kiln: `note:<kiln>/<path>`) or otherwise surfaces the path
- And the user receives inline feedback about the action
