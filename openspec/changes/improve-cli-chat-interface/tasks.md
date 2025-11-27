## 1. Visual Cleanup and Output Refactoring
- [x] 1.1 Remove boxes from startup messages in chat.rs
- [x] 1.2 Remove boxes from agent response display
- [x] 1.3 Implement clean message formatting with minimal separators (using ● indicator)
- [x] 1.4 Add subtle visual hierarchy with indentation and spacing
- [ ] 1.5 Update error message display to be less intrusive
- [ ] 1.6 Replace progress bar with spinner during startup processing
- [ ] 1.7 Add counting display for changed file detection (Phase 1)
- [ ] 1.8 Implement fractional progress display for pipeline processing

## 2. Enhanced Search Capabilities
- [ ] 2.1 Add `/search` command to chat interface
- [ ] 2.2 Integrate semantic search backend for `/search` command
- [ ] 2.3 Format search results for chat display
- [ ] 2.4 Add search result caching and context integration

## 3. File Reference System (@-mentions)
- [ ] 3.1 Implement @ file reference parsing in chat input
- [ ] 3.2 Add fuzzy file search and selection interface
- [ ] 3.3 Support kiln directory and current directory file selection
- [ ] 3.4 Integrate file references into agent context enrichment
- [ ] 3.5 Add visual indicators for referenced files in chat

## 4. Transparency and Visibility
- [x] 4.1 Show tool calls during agent execution (using ▷ indicator with indentation)
- [ ] 4.2 Display file edits and operations in real-time
- [ ] 4.3 Add operation progress indicators
- [ ] 4.4 Implement operation result summaries

## 5. Mode Management
- [x] 5.1 Add auto-approve mode with visual indicators (⚡ icon, /auto command)
- [x] 5.2 Implement Shift+Tab hotkey for mode switching (via BackTab -> /mode)
- [x] 5.3 Add mode status display in chat prompt (mode name + icon)
- [ ] 5.4 Update permission handling for auto-approve mode

## 6. User Experience Improvements
- [ ] 6.1 Add command history search (Ctrl+R)
- [ ] 6.2 Implement Ctrl+J for multiline input
- [ ] 6.3 Add `/clear`, `/reset`, `/export` commands
- [ ] 6.4 Add `/files` command to list files modified in current session
- [ ] 6.5 Add `/context` command to show context usage and files in context
- [x] 6.6 Add `/mode` command for unified mode switching
- [ ] 6.7 Add `/agent` command to switch agents within session
- [ ] 6.8 Implement `/` fuzzy matching for command completion
- [ ] 6.9 Improve error messages with actionable suggestions
- [ ] 6.10 Add `/undo` command for auto-approve mode
- [x] 6.11 Implement double Ctrl+C exit (first press shows warning, second within 2s exits)
- [ ] 6.12 Add markdown terminal rendering with ANSI escape codes

## 7. Integration and Testing
- [ ] 7.1 Update ACP client integration for new features
- [ ] 7.2 Add integration tests for file reference system
- [ ] 7.3 Write tests for mode switching functionality
- [ ] 7.4 Test visual improvements across different terminal sizes
- [ ] 7.5 Performance testing for file search operations