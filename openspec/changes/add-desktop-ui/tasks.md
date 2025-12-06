# Desktop UI Tasks

## Phase 1: MVP Chat Interface

### Setup
- [ ] Create `crucible-desktop` crate with GPUI + gpui-component dependencies
- [ ] Set up basic application shell (window, event loop)
- [ ] Configure gpui-component theme (dark mode)
- [ ] Configure build for cross-platform (macOS, Linux, Windows)

### Core Views
- [ ] Implement root `App` view with layout
- [ ] Implement `ChatView` container using gpui-component layout
- [ ] Implement `MessageList` using `VirtualList` or `Scrollable`
- [ ] Implement `MessageBubble` with user vs agent styling

### Text Input
- [ ] Use gpui-component `Input` or `Editor` for chat input
- [ ] Configure multiline behavior
- [ ] Wire up send action

### Markdown Rendering
- [ ] Use gpui-component's Markdown element for responses
- [ ] Test syntax highlighting in code blocks
- [ ] Add custom rendering for Obsidian extensions (wikilinks, callouts) if needed

### Backend Integration
- [ ] Wire up `ChatAgent` trait consumer
- [ ] Implement streaming token display
- [ ] Use gpui-component `Notification` for errors

### Keyboard Shortcuts
- [ ] Define actions (SendMessage, Cancel, NewLine)
- [ ] Bind Cmd+Enter to send
- [ ] Bind Shift+Enter to new line
- [ ] Bind Escape to cancel/clear

### Modals
- [ ] Use gpui-component `Dialog` for any modals needed
- [ ] Test keybind to open/close modal

### Distribution
- [ ] Add `[[bin]]` target for `cru-desktop`
- [ ] Create `.desktop` file for Linux
- [ ] Document build/install process

## Phase 2: Notes Browser (Future)

- [ ] File tree sidebar component
- [ ] Markdown preview pane
- [ ] File selection → preview update
- [ ] Wikilink navigation

## Phase 3: Editor (Future)

- [ ] Full text editing with cursor/selection
- [ ] Undo/redo stack
- [ ] Live preview pane
- [ ] Save to file

## Phase 4: Graph View (Future)

- [ ] Canvas-based rendering
- [ ] Node layout algorithm
- [ ] Link rendering
- [ ] Interactive pan/zoom
- [ ] Click to navigate

## Future Enhancements

- [ ] Vim/helix motion DSL parser
- [ ] Conversation persistence
- [ ] Settings modal
- [ ] Multiple chat sessions
- [ ] Model selector
- [ ] Theme customization
