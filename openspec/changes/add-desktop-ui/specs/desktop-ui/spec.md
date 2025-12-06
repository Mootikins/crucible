## ADDED Requirements

### Requirement: GPUI Application Shell

The system SHALL provide a native desktop application using GPUI framework.

#### Scenario: Launch application
- **WHEN** user runs `cru-desktop`
- **THEN** application SHALL open native window
- **AND** display chat interface
- **AND** be ready for user input

#### Scenario: Window management
- **WHEN** application is running
- **THEN** window SHALL support resize
- **AND** window SHALL support minimize/maximize
- **AND** window SHALL remember position on restart (future)

### Requirement: Chat Interface

The system SHALL provide a chat interface for conversing with AI agents.

#### Scenario: Display message list
- **WHEN** chat view is active
- **THEN** messages SHALL display in scrollable list
- **AND** user messages SHALL be visually distinct from agent messages
- **AND** list SHALL auto-scroll to newest message

#### Scenario: Stream agent response
- **WHEN** agent sends streaming response
- **THEN** tokens SHALL appear incrementally
- **AND** UI SHALL remain responsive during streaming
- **AND** user MAY scroll during streaming

#### Scenario: Send message
- **WHEN** user presses Cmd+Enter (or Ctrl+Enter)
- **THEN** input content SHALL be sent to agent
- **AND** input field SHALL be cleared
- **AND** message SHALL appear in list immediately

### Requirement: Text Input

The system SHALL provide multiline text input for composing messages.

#### Scenario: Basic text entry
- **WHEN** input is focused
- **THEN** user SHALL type text
- **AND** cursor SHALL be visible
- **AND** text SHALL wrap at container width

#### Scenario: Multiline input
- **WHEN** user presses Shift+Enter
- **THEN** new line SHALL be inserted
- **AND** input SHALL expand vertically (up to limit)

#### Scenario: Cancel input
- **WHEN** user presses Escape
- **THEN** input SHALL be cleared
- **OR** streaming response SHALL be cancelled (if active)

### Requirement: Markdown Rendering

The system SHALL render agent responses as formatted markdown.

#### Scenario: Render headings
- **WHEN** response contains markdown headings
- **THEN** headings SHALL display with appropriate size/weight
- **AND** H1 > H2 > H3 in visual hierarchy

#### Scenario: Render code blocks
- **WHEN** response contains fenced code blocks
- **THEN** code SHALL display in monospace font
- **AND** code SHALL have distinct background
- **AND** language label SHALL be visible (if specified)

#### Scenario: Render lists
- **WHEN** response contains lists
- **THEN** ordered lists SHALL show numbers
- **AND** unordered lists SHALL show bullets
- **AND** nested lists SHALL be indented

#### Scenario: Render inline formatting
- **WHEN** response contains inline formatting
- **THEN** bold text SHALL be bold
- **AND** italic text SHALL be italic
- **AND** inline code SHALL have distinct styling

#### Scenario: Unsupported blocks
- **WHEN** response contains unsupported block types
- **THEN** block SHALL render as plaintext
- **AND** no error SHALL be shown to user

### Requirement: ChatAgent Integration

The system SHALL consume the ChatAgent trait for backend communication.

#### Scenario: Connect to ACP agent
- **WHEN** application starts
- **THEN** ChatAgent adapter SHALL be initialized
- **AND** connection status SHALL be indicated (future)

#### Scenario: Handle agent errors
- **WHEN** agent returns error
- **THEN** error message SHALL display in chat
- **AND** user MAY retry

### Requirement: Keyboard Shortcuts

The system SHALL provide keyboard shortcuts for common actions.

#### Scenario: Send message shortcut
- **GIVEN** input is focused with content
- **WHEN** user presses Cmd+Enter
- **THEN** message SHALL be sent

#### Scenario: New line shortcut
- **GIVEN** input is focused
- **WHEN** user presses Shift+Enter
- **THEN** new line SHALL be inserted

#### Scenario: Cancel shortcut
- **GIVEN** input is focused
- **WHEN** user presses Escape
- **THEN** input SHALL be cleared or streaming cancelled

## DEFERRED Requirements

### Requirement: Notes Browser (Phase 2)
- File tree sidebar
- Markdown preview pane
- Navigation between notes

### Requirement: Editor (Phase 3)
- Editable text with cursor/selection
- Live preview
- Save to file

### Requirement: Graph View (Phase 4)
- Canvas-based link visualization
- Interactive node navigation
- Zoom/pan controls

### Requirement: Vim/Helix Motions (Future)
- Keybind DSL parser
- Configurable motion system
- Support vim and helix paradigms

### Requirement: Conversation Persistence (Future)
- Save chat history to file
- Load previous conversations
- Search history
