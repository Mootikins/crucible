# Tasks: TUI Fuzzy Search / Command Palette

## Phase 0: Spec Prep
- [ ] Confirm inline triggers: `/` for commands; `@` for agents + files/notes
- [ ] Confirm reference formats: workspace = bare relative path; kiln notes = `note:<path>` (single/default) and `note:<kiln>/<path>` when multiple kilns are configured
- [ ] Validate availability rules (hide/flag kiln refs if kiln tools unavailable)

## Phase 1: Palette UX & Data Plumbing
- [ ] Add popup state: trigger type, query slice, results, selection index, debounce
- [ ] Wire data providers: commands (registry), agents list, workspace file list (launch root), kiln notes (`note:<kiln>/<path>`), with availability flags
- [ ] Implement fuzzy matcher + scoring; per-type caps; fast refresh without blocking render

## Phase 2: Actions & Feedback
- [ ] Command: insert/execute respecting namespacing and input hints
- [ ] Agent: switch/connect and show confirmation
- [ ] File/note: insert reference token (workspace bare path; kiln `note:<kiln>/<path>`) and show feedback
- [ ] Handle unavailable items (e.g., kiln refs without tool) with inline messaging

## Phase 3: Tests & Docs
- [ ] Unit tests: trigger detection, matcher, grouping, nav/selection, action routing per type
- [ ] Render tests for popup rows (icons, prefixes, highlight, truncation)
- [ ] Update CLI/TUI docs/help to describe triggers, reference formats, and availability behavior
