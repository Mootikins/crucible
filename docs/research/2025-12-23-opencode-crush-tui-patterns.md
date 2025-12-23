# OpenCode and Crush TUI Pattern Research

**Date:** 2025-12-23
**Purpose:** Analyze TUI architecture and patterns from OpenCode (Go) and Crush/SST OpenCode (TypeScript) for Crucible TUI implementation

## Executive Summary

Examined two major AI coding assistant TUI implementations:
1. **opencode-ai/opencode** - Go-based using Bubble Tea (Charm ecosystem)
2. **sst/opencode** (Crush) - TypeScript-based using custom OpenTUI framework with SolidJS

Both implement rich terminal UIs with conversation views, tool call visualization, and streaming token rendering.

---

## 1. OpenCode-AI (Go + Bubble Tea)

**Repository:** https://github.com/opencode-ai/opencode
**Stack:** Go, Bubble Tea, Lip Gloss, Glamour (markdown rendering)

### Libraries Used

| Library | Purpose | Notes |
|---------|---------|-------|
| `charmbracelet/bubbletea` | TUI framework (Elm architecture) | Core event loop, Model-View-Update pattern |
| `charmbracelet/lipgloss` | Styling & layout | CSS-like styling for terminal |
| `charmbracelet/bubbles` | Pre-built components | Textarea, viewport, spinner, etc. |
| `charmbracelet/glamour` | Markdown rendering | Converts markdown to styled terminal output |
| `charmbracelet/x/ansi` | ANSI utilities | Text truncation, width calculation |
| `lrstanley/bubblezone` | Mouse region tracking | Clickable regions |
| `alecthomas/chroma/v2` | Syntax highlighting | Code block rendering |

### Component Architecture

```
appModel (root)
├── pages (map[PageID]tea.Model)
│   ├── ChatPage
│   │   ├── messagesCmp (viewport with messages)
│   │   ├── editorCmp (textarea input)
│   │   └── sidebar
│   └── LogsPage
├── status (StatusCmp - footer bar)
└── dialogs (overlays)
    ├── permissions
    ├── help
    ├── quit
    ├── sessionDialog
    ├── commandDialog
    ├── modelDialog
    ├── filepicker
    └── themeDialog
```

**Key Patterns:**

1. **Page-based routing** - Main app switches between chat/logs pages
2. **Overlay dialogs** - Rendered as centered overlays using `layout.PlaceOverlay()`
3. **Viewport scrolling** - Messages rendered in `bubbles/viewport` for scroll management
4. **Component isolation** - Each component implements `tea.Model` interface
5. **Message passing** - Custom message types for inter-component communication

### Message Rendering

**File:** `internal/tui/components/chat/message.go`

```go
type uiMessage struct {
    ID          string
    messageType uiMessageType  // user, assistant, tool
    position    int
    height      int
    content     string
}
```

**Rendering Flow:**

1. **User Messages** - Blue left border, attachment badges
2. **Assistant Messages** - Primary color border, markdown rendering via Glamour
3. **Tool Messages** - Nested rendering with custom borders

**Message Styling:**
```go
style := styles.BaseStyle().
    Width(width - 1).
    BorderLeft(true).
    Foreground(t.TextMuted()).
    BorderForeground(t.Primary()).
    BorderStyle(lipgloss.ThickBorder())
```

### Tool Call Visualization

**File:** `internal/tui/components/chat/message.go`

**Tool States:**
- **Building** - "Building tool call..." with spinner
- **Waiting** - "Waiting for response..." (italic, muted)
- **Completed** - Show params + response with syntax highlighting

**Tool Rendering Pattern:**
```go
func renderToolMessage(toolCall, allMessages, messagesService, focused, nested, width, pos) {
    // 1. Tool name + icon
    toolNameText := baseStyle.Foreground(t.TextMuted()).
        Render(fmt.Sprintf("%s: ", toolName(toolCall.Name)))

    // 2. If not finished - show progress
    if !toolCall.Finished {
        progressText := "Building command..." // per-tool action
    }

    // 3. Tool params (truncated to fit)
    params := renderToolParams(width-2-lipgloss.Width(toolNameText), toolCall)

    // 4. Tool response (with syntax highlighting for code)
    responseContent := renderToolResponse(toolCall, response, width-2)

    // Combine with left border
}
```

**Tool-Specific Rendering:**

- **Bash** - Show command + output in code block
- **Edit** - Show diff with colored additions/deletions
- **View** - Show file content with language-specific highlighting
- **Glob/Grep** - Show summary (X matches)

### Streaming Token Handling

**Pattern:** Reactive message updates via PubSub

```go
case pubsub.Event[message.Message]:
    if msg.Type == pubsub.UpdatedEvent {
        // Update message in list
        m.messages[i] = msg.Payload

        // Clear cache for this message
        delete(m.cachedContent, msg.Payload.ID)

        // Re-render
        m.renderView()

        // Auto-scroll to bottom if last message
        if isLastMessage {
            m.viewport.GotoBottom()
        }
    }
```

**Cache Strategy:**
- Cache rendered messages by ID + width
- Invalidate on update or window resize
- Prevents re-rendering unchanged messages

### Conversation List Rendering

**File:** `internal/tui/components/chat/list.go`

**Approach:** Bottom-anchored viewport

```go
type messagesCmp struct {
    viewport      viewport.Model
    messages      []message.Message
    uiMessages    []uiMessage
    cachedContent map[string]cacheItem
}

func (m *messagesCmp) renderView() {
    m.uiMessages = make([]uiMessage, 0)
    pos := 0

    for _, msg := range m.messages {
        // Render user/assistant/tool messages
        // Track position for each message
        pos += msg.height + 1
    }

    // Join all messages vertically
    m.viewport.SetContent(lipgloss.JoinVertical(lipgloss.Top, messages...))
}
```

**Viewport Management:**
- Use `bubbles/viewport` component
- Set content height dynamically
- `viewport.GotoBottom()` on new messages
- `viewport.Height = height - 2` (reserve space for input/status)

### Input Box Handling

**File:** `internal/tui/components/chat/editor.go`

**Pattern:** Bubble Tea textarea with custom keybindings

```go
type editorCmp struct {
    textarea    textarea.Model
    attachments []message.Attachment
    deleteMode  bool
}

// Key handling
case key.Matches(msg, editorMaps.Send):
    value := m.textarea.Value()
    if len(value) > 0 && value[len(value)-1] == '\\' {
        // Backslash at end = add newline
        m.textarea.SetValue(value[:len(value)-1] + "\n")
    } else {
        // Send message
        return m, m.send()
    }
```

**Features:**
- Multi-line support with `\` escape
- Attachment management (images, files)
- Ctrl+E to open $EDITOR for long messages
- Auto-focus on startup

### Status Bar Implementation

**File:** `internal/tui/components/core/status.go`

**Layout:** Fixed bottom bar with segments

```
[Help Widget] [Info/Error Message         ] [Diagnostics] [Token Info] [Model Name]
```

**Segments:**
1. **Help** - `ctrl+? help` badge
2. **Messages** - Error/warning/info with auto-clear timeout
3. **Diagnostics** - LSP errors/warnings count with icons
4. **Tokens** - Context usage (110K, 1.2M) + cost
5. **Model** - Current model name

**Dynamic Width:**
```go
availableWidth := max(0, m.width -
    lipgloss.Width(helpWidget) -
    lipgloss.Width(model) -
    lipgloss.Width(diagnostics) -
    tokenInfoWidth)

// Message fills remaining space
status += infoStyle.Width(availableWidth).Render(msg)
```

### Dashboard/Splash Screen

**File:** `internal/tui/components/chat/chat.go`

**Pattern:** Show when `len(messages) == 0`

```go
func (m *messagesCmp) View() string {
    if len(m.messages) == 0 {
        return m.initialScreen()
    }
    // ... normal view
}

func (m *messagesCmp) initialScreen() string {
    return lipgloss.JoinVertical(
        lipgloss.Top,
        logo(m.width),           // ASCII art
        repo(m.width),           // GitHub URL
        cwd(m.width),            // Current directory
        lspsConfigured(m.width), // LSP server list
    )
}
```

### Theme System

**File:** `internal/tui/theme/theme.go`

**Pattern:** Interface-based themes with AdaptiveColor

```go
type Theme interface {
    Primary() lipgloss.AdaptiveColor
    Secondary() lipgloss.AdaptiveColor
    Text() lipgloss.AdaptiveColor
    Background() lipgloss.AdaptiveColor
    BorderFocused() lipgloss.AdaptiveColor
    // ... 50+ color methods
}

// Access current theme
t := theme.CurrentTheme()
style := lipgloss.NewStyle().Foreground(t.Primary())
```

**Themes Included:**
- OpenCode (default)
- Catppuccin
- Dracula
- Gruvbox
- Tokyo Night
- Monokai
- OneDark
- Flexoki
- Tron

**Theme Manager:**
- `theme.SetTheme(themeName)` - Switch themes
- Broadcasts `ThemeChangedMsg` to all components
- Components re-render with new colors

### Keybinding System

**Pattern:** Centralized keybind definitions + help rendering

```go
type keyMap struct {
    Logs          key.Binding
    Quit          key.Binding
    Help          key.Binding
    SwitchSession key.Binding
    Commands      key.Binding
    // ...
}

var keys = keyMap{
    Quit: key.NewBinding(
        key.WithKeys("ctrl+c"),
        key.WithHelp("ctrl+c", "quit"),
    ),
    // ...
}
```

**Help Dialog:**
- `layout.KeyMapToSlice()` extracts bindings via reflection
- Aggregates bindings from all active components
- Renders as centered overlay

### Dialog/Popup Patterns

**File:** `internal/tui/components/dialog/*.go`

**Overlay Pattern:**
```go
if a.showQuit {
    overlay := a.quit.View()
    row := lipgloss.Height(appView) / 2 - lipgloss.Height(overlay) / 2
    col := lipgloss.Width(appView) / 2 - lipgloss.Width(overlay) / 2
    appView = layout.PlaceOverlay(col, row, overlay, appView, true)
}
```

**Modal Behavior:**
- Set `showXDialog` flag
- Intercept keypress events
- Block underlying components from receiving keys
- Return message to close dialog

**Dialog Types:**
- **Confirm** - Yes/No prompts
- **Select** - List selection (sessions, commands, models, themes)
- **Input** - Text entry (session rename, command args)
- **Permission** - Allow/Deny tool execution
- **Help** - Keybinding reference

---

## 2. SST OpenCode (TypeScript + OpenTUI)

**Repository:** https://github.com/sst/opencode
**Stack:** Bun, SolidJS, OpenTUI (custom TUI framework)

### Libraries Used

| Library | Purpose | Notes |
|---------|---------|-------|
| `@opentui/core` | Custom TUI rendering engine | Box model, scrolling, mouse events |
| `@opentui/solid` | SolidJS integration | Reactive rendering for TUI |
| `solid-js` | Reactive primitives | Signals, effects, context |
| `hono` | HTTP framework | Server-side API |
| `@opencode-ai/sdk` | API client | Types for messages/tools |
| `diff` | Diff parsing | Patch file rendering |
| `strip-ansi` | ANSI escape handling | Clean terminal output |

### Component Architecture

**File:** `packages/opencode/src/cli/cmd/tui/routes/session/index.tsx`

**Pattern:** SolidJS components with OpenTUI primitives

```tsx
<box flexDirection="row">
  <box flexGrow={1} paddingBottom={1} paddingTop={1}>
    <Header />
    <scrollbox ref={scroll} stickyScroll={true} stickyStart="bottom">
      <For each={messages()}>
        {(message) => (
          <Switch>
            <Match when={message.role === "user"}>
              <UserMessage message={message} parts={parts} />
            </Match>
            <Match when={message.role === "assistant"}>
              <AssistantMessage message={message} parts={parts} />
            </Match>
          </Switch>
        )}
      </For>
    </scrollbox>
    <Prompt onSubmit={() => toBottom()} />
    <Footer />
  </box>
  <Show when={sidebarVisible()}>
    <Sidebar sessionID={sessionID} />
  </Show>
</box>
```

**Key Differences from Bubble Tea:**

1. **JSX-like syntax** - Declarative component tree
2. **Reactive signals** - `createSignal()`, `createMemo()`, `createEffect()`
3. **Box model layout** - Flexbox-like positioning
4. **No Update() pattern** - Direct state updates trigger re-render

### Message Rendering

**Pattern:** Part-based rendering with dynamic components

```tsx
function AssistantMessage(props: { message, parts }) {
  const PART_MAPPING = {
    text: TextPart,
    tool: ToolPart,
    reasoning: ReasoningPart,
  }

  return (
    <For each={props.parts}>
      {(part) => (
        <Dynamic component={PART_MAPPING[part.type]} part={part} />
      )}
    </For>
  )
}

function TextPart(props: { part }) {
  return (
    <box paddingLeft={3} marginTop={1}>
      <code
        filetype="markdown"
        streaming={true}
        content={props.part.text}
        conceal={ctx.conceal()}
        fg={theme.text}
      />
    </box>
  )
}
```

**Streaming Support:**
- `streaming={true}` flag on `<code>` component
- Reactive updates via SolidJS signals
- Auto-scroll on content change

### Tool Visualization

**Pattern:** Tool registry with custom renderers

```tsx
const ToolRegistry = (() => {
  const state: Record<string, ToolRegistration> = {}

  function register<T>(input: ToolRegistration<T>) {
    state[input.name] = input
  }

  return {
    register,
    container(name: string) {
      return state[name]?.container // "inline" | "block"
    },
    render(name: string) {
      return state[name]?.render
    },
  }
})()

// Register tool renderer
ToolRegistry.register<typeof BashTool>({
  name: "bash",
  container: "block",
  render(props) {
    return (
      <>
        <ToolTitle icon="#" fallback="Writing command...">
          {props.input.description || "Shell"}
        </ToolTitle>
        <text fg={theme.text}>$ {props.input.command}</text>
        <text fg={theme.text}>{props.metadata.output}</text>
      </>
    )
  },
})
```

**Tool States:**
- `pending` - Show fallback message
- `completed` - Render input + metadata + output
- `error` - Red error text

**Tool Containers:**
- **inline** - Renders on single line with padding (Read, Glob, Grep)
- **block** - Renders in bordered box (Bash, Edit, Write)

### Conversation Scrolling

**Pattern:** `scrollbox` with sticky bottom

```tsx
<scrollbox
  ref={(r) => (scroll = r)}
  stickyScroll={true}
  stickyStart="bottom"
  flexGrow={1}
  scrollAcceleration={scrollAcceleration()}
  verticalScrollbarOptions={{
    visible: showScrollbar(),
    trackOptions: {
      backgroundColor: theme.backgroundElement,
      foregroundColor: theme.border,
    },
  }}
>
  {/* messages */}
</scrollbox>
```

**Features:**
- Sticky bottom scroll (auto-scroll on new messages)
- Configurable scroll acceleration (MacOS-style or custom speed)
- Optional scrollbar with themed colors
- Mouse wheel + keyboard navigation

### Input Component

**File:** `packages/opencode/src/cli/cmd/tui/component/prompt/index.tsx`

**Pattern:** Custom textarea with autocomplete

```tsx
export function Prompt(props: PromptProps) {
  const [input, setInput] = createSignal("")
  const [files, setFiles] = createStore<FilePart[]>([])

  return (
    <box flexDirection="column">
      <Show when={files.length}>
        <FileAttachments files={files} />
      </Show>
      <box flexDirection="row">
        <text fg={theme.primary}>{">"}</text>
        <textarea
          ref={(r) => (textarea = r)}
          onPaste={handlePaste}
          onSubmit={() => submit()}
        />
      </box>
      <Autocomplete ref={autocomplete} />
    </box>
  )
}
```

**Features:**
- File attachment drag-and-drop
- Command autocomplete (slash commands)
- Multi-line editing
- History navigation (up/down arrows)
- Paste event handling (images → base64)

### Status Indicators

**Pattern:** Computed signals + spinner

```tsx
const status = createMemo(() => {
  const lastMessage = messages().at(-1)
  if (!lastMessage || lastMessage.role !== "assistant") return null

  if (hasToolsWithoutResponse(messages())) {
    return "Waiting for tool response..."
  } else if (hasUnfinishedToolCalls(messages())) {
    return "Building tool call..."
  } else if (!lastMessage.time.completed) {
    return "Generating..."
  }
  return null
})

return (
  <Show when={status()}>
    <text fg={theme.primary}>{spinner()} {status()}</text>
  </Show>
)
```

**Spinner:**
- Custom spinner frames and colors
- Uses `createEffect()` to advance frame index
- Multiple styles (pulse, dots, line)

### Keybinding System

**Pattern:** Context-based keybind manager

```tsx
const keybind = useKeybind()

keybind.register({
  id: "messages_page_down",
  key: "ctrl+d",
  description: "Page down",
  handler: () => scroll.scrollBy(scroll.height / 2)
})

// Display in UI
<text>
  {keybind.print("messages_page_down")}
  <span style={{ fg: theme.textMuted }}> to scroll down</span>
</text>
```

**Command Palette:**
```tsx
command.register(() => [
  {
    title: "Rename session",
    value: "session.rename",
    keybind: "session_rename",
    category: "Session",
    onSelect: (dialog) => {
      dialog.replace(() => <DialogSessionRename />)
    },
  },
  // ...
])
```

### Theme System

**Pattern:** Context provider with theme object

```tsx
const { theme, syntax } = useTheme()

// Theme colors
theme.text
theme.primary
theme.background
theme.diffAddedBg

// Syntax highlighting style
syntax() // returns Chroma style name
```

**Dynamic Theme Loading:**
- Themes stored in config
- Hot-reload on theme change
- Separate syntax highlighting styles

### Dialog System

**Pattern:** Stack-based dialog manager

```tsx
const dialog = useDialog()

// Show dialog
dialog.replace(() => <DialogConfirm
  title="Confirm"
  message="Are you sure?"
/>)

// In DialogConfirm
const result = await DialogConfirm.show(dialog, "Title", "Message")
if (result) {
  // User confirmed
}

dialog.clear()
```

**Dialog Stack:**
- Push/pop/replace operations
- Auto-dismiss on escape
- Centered overlay rendering
- Focus trapping

---

## 3. Key Takeaways for Crucible

### Architecture Patterns

1. **Component isolation** - Each UI section is self-contained Model/Component
2. **Message-based communication** - Custom message types for events
3. **Viewport scrolling** - Use viewport component for message list
4. **Bottom-anchored rendering** - Auto-scroll to bottom on new messages
5. **Caching strategy** - Cache rendered messages, invalidate on change

### Rendering Strategies

1. **Markdown rendering** - Use Glamour (Go) or similar library
2. **Syntax highlighting** - Chroma for code blocks
3. **Diff rendering** - Custom diff formatter with colors
4. **Truncation** - Intelligent text truncation with ANSI width awareness
5. **Progressive rendering** - Render visible content first, lazy-load details

### Streaming Patterns

1. **PubSub events** - Message update events trigger re-render
2. **Incremental updates** - Only re-render changed messages
3. **Cache invalidation** - Clear cache for updated message
4. **Auto-scroll logic** - Scroll to bottom only for new/last messages

### Tool Call Visualization

1. **State-based rendering** - Different UI for pending/running/completed
2. **Tool-specific renderers** - Custom formatting per tool type
3. **Nested tool support** - Recursive rendering for sub-agents
4. **Inline vs block** - Compact inline for simple tools, block for output
5. **Response truncation** - Limit output height (e.g., 10 lines max)

### Input Handling

1. **Multi-line support** - Backslash escape or explicit key combo
2. **Attachment management** - Visual badges for files/images
3. **Editor integration** - Ctrl+E to open $EDITOR
4. **Command palette** - Slash commands with fuzzy search
5. **History navigation** - Up/down arrows through past prompts

### Status Bar Best Practices

1. **Segmented layout** - Fixed sections with dynamic content area
2. **Icon usage** - Visual indicators for errors/warnings
3. **Token tracking** - Show context usage + cost
4. **Auto-dismiss** - Clear temporary messages after timeout
5. **Truncation** - Ensure messages fit available width

### Dialog/Overlay Patterns

1. **Centered positioning** - Calculate center based on overlay size
2. **Modal behavior** - Block underlying input
3. **Stack management** - Support nested dialogs
4. **Keyboard navigation** - Arrow keys, enter, escape
5. **Focus management** - Return focus after close

---

## 4. Code Examples

### Message Rendering (Bubble Tea Style)

```go
func renderUserMessage(msg message.Message, width int) string {
    t := theme.CurrentTheme()
    style := styles.BaseStyle().
        Width(width - 1).
        BorderLeft(true).
        BorderForeground(t.Secondary()).
        BorderStyle(lipgloss.ThickBorder())

    content := toMarkdown(msg.Content(), width)
    return style.Render(content)
}

func renderAssistantMessage(msg message.Message, width int) string {
    t := theme.CurrentTheme()
    style := styles.BaseStyle().
        Width(width - 1).
        BorderLeft(true).
        BorderForeground(t.Primary()).
        BorderStyle(lipgloss.ThickBorder())

    parts := []string{
        toMarkdown(msg.Content(), width),
    }

    // Add tool calls
    for _, toolCall := range msg.ToolCalls() {
        parts = append(parts, renderToolCall(toolCall, width))
    }

    return style.Render(lipgloss.JoinVertical(lipgloss.Left, parts...))
}
```

### Viewport Scrolling Pattern

```go
type messagesCmp struct {
    viewport viewport.Model
    messages []message.Message
}

func (m *messagesCmp) renderView() {
    messages := make([]string, 0)

    for _, msg := range m.messages {
        rendered := renderMessage(msg, m.width)
        messages = append(messages, rendered, "") // Add spacing
    }

    content := lipgloss.JoinVertical(lipgloss.Top, messages...)
    m.viewport.SetContent(content)
}

func (m *messagesCmp) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
    switch msg := msg.(type) {
    case NewMessageMsg:
        m.messages = append(m.messages, msg.Message)
        m.renderView()
        m.viewport.GotoBottom() // Auto-scroll
        return m, nil
    }
    // ...
}
```

### Tool Rendering Pattern

```go
func renderToolCall(toolCall ToolCall, width int) string {
    t := theme.CurrentTheme()

    // Tool header
    header := lipgloss.NewStyle().
        Foreground(t.TextMuted()).
        Render(fmt.Sprintf("%s %s", toolIcon(toolCall.Name), toolCall.Name))

    // Tool params
    params := renderToolParams(toolCall, width-4)

    // Tool output
    var output string
    if toolCall.Response != nil {
        output = renderToolOutput(toolCall, width-4)
    } else if !toolCall.Finished {
        output = lipgloss.NewStyle().
            Italic(true).
            Foreground(t.TextMuted()).
            Render("Building tool call...")
    }

    return lipgloss.JoinVertical(
        lipgloss.Left,
        header,
        params,
        output,
    )
}
```

### Status Bar Layout

```go
func (m *statusCmp) View() string {
    t := theme.CurrentTheme()

    // Fixed segments
    help := renderHelpBadge()
    diagnostics := renderDiagnostics(m.lspClients)
    tokens := renderTokenInfo(m.session)
    model := renderModel()

    // Calculate available width for message
    fixedWidth := lipgloss.Width(help) +
                  lipgloss.Width(diagnostics) +
                  lipgloss.Width(tokens) +
                  lipgloss.Width(model)
    availableWidth := max(0, m.width - fixedWidth)

    // Message fills remaining space
    message := lipgloss.NewStyle().
        Width(availableWidth).
        Background(t.Info()).
        Foreground(t.Background()).
        Render(m.info.Msg)

    // Join horizontally
    return lipgloss.JoinHorizontal(
        lipgloss.Left,
        help,
        message,
        diagnostics,
        tokens,
        model,
    )
}
```

### Dialog Overlay Pattern

```go
func (a appModel) View() string {
    // Base view
    appView := lipgloss.JoinVertical(
        lipgloss.Top,
        a.pages[a.currentPage].View(),
        a.status.View(),
    )

    // Overlay dialog if shown
    if a.showDialog {
        overlay := a.dialog.View()

        // Center the overlay
        row := lipgloss.Height(appView)/2 - lipgloss.Height(overlay)/2
        col := lipgloss.Width(appView)/2 - lipgloss.Width(overlay)/2

        appView = layout.PlaceOverlay(col, row, overlay, appView, true)
    }

    return appView
}

// Key handling - block underlying views
func (a appModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
    switch msg := msg.(type) {
    case tea.KeyMsg:
        if a.showDialog {
            // Only update dialog
            d, cmd := a.dialog.Update(msg)
            a.dialog = d
            return a, cmd
        }
        // Normal key handling
        // ...
    }
}
```

### Streaming Message Updates

```go
func (m *messagesCmp) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
    switch msg := msg.(type) {
    case StreamTokenMsg:
        // Find last assistant message
        lastMsg := m.messages[len(m.messages)-1]

        // Append token
        lastMsg.Content += msg.Token
        m.messages[len(m.messages)-1] = lastMsg

        // Clear cache for this message
        delete(m.cachedContent, lastMsg.ID)

        // Re-render
        m.renderView()

        // Auto-scroll if at bottom
        if m.viewport.AtBottom() {
            m.viewport.GotoBottom()
        }

        return m, nil
    }
}
```

---

## 5. Recommendations for Crucible TUI

### Must-Have Features

1. **Bubble Tea framework** - De facto standard for Go TUIs
2. **Viewport scrolling** - Use `bubbles/viewport` for message list
3. **Markdown rendering** - Use Glamour for message content
4. **Syntax highlighting** - Use Chroma for code blocks
5. **Theme system** - Interface-based themes with 8+ built-ins
6. **Command palette** - Fuzzy searchable commands (Ctrl+K)
7. **Status bar** - Token count, diagnostics, model name
8. **Dialog overlays** - Centered modal dialogs
9. **Mouse support** - Optional but nice for scrolling/clicking

### Architecture Recommendations

1. **Page-based routing** - Separate conversation/settings/logs pages
2. **Component isolation** - Each section implements `tea.Model`
3. **Message caching** - Cache rendered messages by ID+width
4. **PubSub events** - For message updates, tool calls, etc.
5. **Keybind registry** - Centralized keybinding definitions

### Tool Visualization Strategy

1. **Tool registry** - Map tool names to renderers
2. **Container types** - Inline (compact) vs block (detailed)
3. **State indicators** - Different styling for pending/running/completed
4. **Output truncation** - Max 10-15 lines, "... X more lines" footer
5. **Nested rendering** - Support sub-agents with indentation

### Input Component Design

1. **Multi-line textarea** - Use `bubbles/textarea`
2. **Backslash escape** - `\` at end of line = literal newline
3. **Ctrl+E editor** - Open $EDITOR for long messages
4. **Attachment UI** - Show file badges below input
5. **Placeholder text** - Rotate helpful examples

### Styling Best Practices

1. **Adaptive colors** - Use `lipgloss.AdaptiveColor` for light/dark terminals
2. **Border styles** - ThickBorder for messages, NormalBorder for dialogs
3. **Consistent spacing** - 1-line gap between messages
4. **Icon usage** - Unicode icons for status (✓, ✗, !, ?)
5. **Truncation** - Use `ansi.Truncate()` with "..." suffix

### Accessibility Considerations

1. **Keyboard-first** - All actions accessible via keyboard
2. **Mouse optional** - Don't require mouse for core functionality
3. **Screen reader** - Use semantic ANSI codes (bold, color)
4. **High contrast** - Ensure text readable on all backgrounds
5. **No flashing** - Avoid rapid color changes (seizure risk)

---

## 6. Reference Files

### OpenCode-AI (Go)

| File | Purpose |
|------|---------|
| `internal/tui/tui.go` | Main app model, page routing |
| `internal/tui/components/chat/chat.go` | Chat page, header, logo |
| `internal/tui/components/chat/list.go` | Message list with viewport |
| `internal/tui/components/chat/message.go` | Message rendering (user/assistant/tool) |
| `internal/tui/components/chat/editor.go` | Input textarea component |
| `internal/tui/components/core/status.go` | Status bar at bottom |
| `internal/tui/layout/layout.go` | Layout interfaces (Sizeable, Focusable) |
| `internal/tui/styles/styles.go` | Style helper functions |
| `internal/tui/theme/theme.go` | Theme interface and colors |
| `internal/tui/components/dialog/*.go` | Dialog overlays |

### SST OpenCode (TypeScript)

| File | Purpose |
|------|---------|
| `packages/opencode/src/cli/cmd/tui/routes/session/index.tsx` | Main session view (2000+ lines!) |
| `packages/opencode/src/cli/cmd/tui/component/prompt/index.tsx` | Input component with autocomplete |
| `packages/opencode/src/cli/ui.ts` | CLI UI utilities |
| `packages/opencode/src/server/tui.ts` | TUI server integration |

---

## Conclusion

Both implementations share core patterns despite different tech stacks:

1. **Viewport-based scrolling** with auto-scroll to bottom
2. **Message caching** for performance
3. **Tool registry** with custom renderers
4. **Theme systems** with 8+ color schemes
5. **Dialog overlays** centered on screen
6. **Status bars** with dynamic content
7. **Command palettes** for discoverability

**For Crucible:** The Bubble Tea approach (OpenCode-AI) is more idiomatic for Go and has better ecosystem support. The component architecture, caching strategy, and rendering patterns can be adopted directly.

**Next Steps:**
1. Set up Bubble Tea + Bubbles + Lip Gloss + Glamour
2. Implement basic message list with viewport
3. Add streaming token support
4. Build tool call visualization registry
5. Create theme system based on OpenCode's interface
6. Add command palette and dialogs
