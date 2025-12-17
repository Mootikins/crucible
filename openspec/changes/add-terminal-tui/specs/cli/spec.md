# CLI Capability Delta: Terminal TUI

## ADDED Requirements

### Requirement: Interactive TUI Mode (REQ-CLI-TUI-001)
The CLI SHALL provide an interactive terminal user interface for chat sessions using ratatui.

#### Scenario: User launches interactive chat
- Given the user runs `cru chat` without `--query`
- When the TUI initializes
- Then a full-screen terminal interface displays
- And the interface shows message history, input area, and status bar

#### Scenario: User sends a message
- Given the TUI is running
- When the user types a message and presses Enter
- Then the message appears in the history as a user message
- And a `MessageReceived` event is emitted via SessionHandle

### Requirement: Streaming Response Display (REQ-CLI-TUI-002)
The TUI SHALL display streaming responses token-by-token as `TextDelta` events arrive.

#### Scenario: Assistant streams response
- Given the user has sent a message
- When `TextDelta` events arrive from the ring buffer
- Then each delta is appended to the streaming message display
- And the display updates in real-time (< 50ms latency)

#### Scenario: Response completes
- Given streaming is in progress
- When an `AgentResponded` event arrives
- Then the streaming buffer is finalized as a complete message
- And the streaming indicator disappears

### Requirement: Mode Management (REQ-CLI-TUI-003)
The TUI SHALL support Plan/Act/AutoApprove mode switching with visual indication.

#### Scenario: User cycles mode
- Given the TUI shows current mode as "Plan"
- When the user presses Shift+Tab
- Then the mode cycles to "Act"
- And the status bar updates to show the new mode

#### Scenario: Mode slash commands
- Given the TUI is running
- When the user types `/plan`, `/act`, or `/auto`
- Then the mode changes to the specified mode

### Requirement: Keyboard Navigation (REQ-CLI-TUI-004)
The TUI SHALL support keyboard shortcuts for common operations.

#### Scenario: Scroll message history
- Given there are more messages than fit on screen
- When the user presses Up/Down or PgUp/PgDn
- Then the message history scrolls accordingly

#### Scenario: Cancel operation
- Given the assistant is generating a response
- When the user presses Ctrl+C once
- Then the current operation is cancelled
- And a cancellation message appears

#### Scenario: Exit application
- Given the TUI is running
- When the user presses Ctrl+C twice in quick succession
- Then the TUI exits gracefully

### Requirement: Tool Call Visualization (REQ-CLI-TUI-005)
The TUI SHALL display tool calls with progress indication.

#### Scenario: Tool call in progress
- Given the assistant invokes a tool
- When a `ToolCalled` event arrives
- Then the tool name displays with a spinner
- And tool arguments are shown (optionally collapsed)

#### Scenario: Tool completes
- Given a tool is in progress
- When a `ToolCompleted` event arrives
- Then the spinner is replaced with a checkmark or error icon
- And the result summary is displayed

### Requirement: Event-Based State Management (REQ-CLI-TUI-006)
The TUI SHALL derive display state from the SessionEvent ring buffer.

#### Scenario: State synchronization
- Given the TUI is running
- When the ring buffer contains new events
- Then the TUI polls and processes events incrementally
- And display state updates without full refresh

#### Scenario: Resume after disconnect
- Given a session was interrupted
- When the TUI reconnects to the same session
- Then message history is restored from the ring buffer
