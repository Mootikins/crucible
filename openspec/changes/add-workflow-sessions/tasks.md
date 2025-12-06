# Implementation Tasks

## Phase 1: Core Types

- [ ] 1.1 Create `crucible-core/src/session/` module structure
- [ ] 1.2 Define `SessionLog` struct with frontmatter fields
- [ ] 1.3 Define `Phase` struct (name, agent, channel, timestamp)
- [ ] 1.4 Define `ToolCall` struct (name, args, result, success)
- [ ] 1.5 Define `Callout` enum (Decision, Error, User, Subagent)
- [ ] 1.6 Define `SubagentRef` struct (session_id, agent, model, tokens, mode)
- [ ] 1.7 Define `SessionStatus` enum (Active, Paused, Completed, Failed)
- [ ] 1.8 Add unit tests for all types

## Phase 2: Frontmatter Schema

- [ ] 2.1 Create `SessionFrontmatter` struct matching spec
- [ ] 2.2 Implement serde serialization/deserialization
- [ ] 2.3 Add validation for required fields
- [ ] 2.4 Add frontmatter generation from SessionLog
- [ ] 2.5 Add unit tests for frontmatter round-trip

## Phase 3: Session Parser

- [ ] 3.1 Create session parser in crucible-parser
- [ ] 3.2 Parse heading-per-phase structure
- [ ] 3.3 Extract `@agent` and `#channel` from headings
- [ ] 3.4 Parse tool call lists
- [ ] 3.5 Parse callout blocks (decision, error, user, subagent)
- [ ] 3.6 Handle subagent modes (inline, link, embed)
- [ ] 3.7 Add integration tests with example sessions

## Phase 4: Session Writer

- [ ] 4.1 Implement SessionLog → Markdown serialization
- [ ] 4.2 Format phases with proper headings
- [ ] 4.3 Format tool calls as lists
- [ ] 4.4 Format callouts with proper syntax
- [ ] 4.5 Handle subagent references by mode
- [ ] 4.6 Add round-trip tests (parse → serialize → parse)

## Phase 5: CLI Commands

- [ ] 5.1 Add `cru session` subcommand group
- [ ] 5.2 Implement `cru session list` with filters
- [ ] 5.3 Implement `cru session show <id>`
- [ ] 5.4 Implement `cru session resume <id>`
- [ ] 5.5 Add `--format json` support for list/show
- [ ] 5.6 Add help text and examples

## Phase 6: Resumption Logic

- [ ] 6.1 Create ResumeContext from parsed session
- [ ] 6.2 Apply context strategy for long sessions
- [ ] 6.3 Prioritize decisions and recent phases
- [ ] 6.4 Integrate with chat session startup
- [ ] 6.5 Test resumption with mock agent

## Phase 7: Codification

- [ ] 7.1 Implement auto-extract: phases → workflow steps
- [ ] 7.2 Implement `cru session codify` command
- [ ] 7.3 Add `--refine` flag for agent improvement
- [ ] 7.4 Add `--interactive` flag for user review
- [ ] 7.5 Output valid workflow-markup format
- [ ] 7.6 Test with example sessions

## Phase 8: TOON Index

- [ ] 8.1 Define TOON schema for session index
- [ ] 8.2 Implement session → TOON encoding
- [ ] 8.3 Store index alongside session files
- [ ] 8.4 Implement query by agent/channel/status
- [ ] 8.5 Implement aggregation queries
- [ ] 8.6 Add index update on session save

## Phase 9: RL Case Generation

- [ ] 9.1 Implement `cru session learn` command
- [ ] 9.2 Extract failure contexts for puzzles
- [ ] 9.3 Extract user interventions as corrections
- [ ] 9.4 Format as puzzle scenario
- [ ] 9.5 Export to JSONL format
- [ ] 9.6 Document puzzle format for training

## Phase 10: Integration

- [ ] 10.1 Update chat session to write session logs
- [ ] 10.2 Configure session save threshold
- [ ] 10.3 Configure default subagent mode
- [ ] 10.4 Add session directory to config
- [ ] 10.5 Update example-config.toml
- [ ] 10.6 End-to-end test: chat → log → resume

## Future TODOs (not this change)

- Task list serialization (research OpenCode/Claude Code first)
- Rune extensibility for custom codification workflows
- Federated session sync (A2A)
- Multi-user session support
- LoRA training pipeline integration
- Session visualization in future GUI
