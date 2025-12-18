# TUI Fuzzy Search / Command Palette

## Why

- Slash commands, agents, and files are growing—users need a single, quick entry point to find and run things without memorizing names.
- The existing TUI lacks a command palette; navigation is slower than in modern CLI/TUI tools (fzf-style workflows).
- We must search both the kiln and the workspace where the agent was launched so users can hop between repo files and knowledge base content.

## What

- Add a TUI command palette with fuzzy search across:
  - Slash commands (client + agent-provided, including hints/secondary options)
  - Agents (discovered/local)
  - Workspace files (bare relative paths) and kiln notes using `note:<path>` for single/default kilns and `note:<kiln>/<path>` when multiple kilns are configured
- Provide keyboard workflow: open palette, type to filter, arrow/Tab to select, Enter to execute the chosen item with type-specific actions.
- Show lightweight context per result (e.g., command hint/description, agent description, file path).

## Non-Goals

- No new semantic search in the palette (stick to fuzzy/substring and existing indices).
- No in-palette file preview or editing (open/inspect remains out of scope).
- No cross-workspace search beyond kiln + launch workspace.

## Success Criteria

- Palette opens instantly and filters interactively without freezing the TUI.
- A user can pick a slash command, agent, or file with a single flow and see the action happen (command execution, agent switch, or file selection feedback).
- File results include both kiln and launch workspace paths.
