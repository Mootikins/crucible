# Implementation Tasks

## Phase 1: Naming Clarification (DONE)

- [x] 1.1 Rename `AgentDefinition` → `AgentCard` in types.rs
- [x] 1.2 Rename `AgentRegistry` → `AgentCardRegistry` in mod.rs
- [x] 1.3 Rename `AgentLoader` → `AgentCardLoader` in loader.rs
- [x] 1.4 Rename `AgentFrontmatter` → `AgentCardFrontmatter` in types.rs
- [x] 1.5 Rename `AgentQuery` → `AgentCardQuery` in types.rs
- [x] 1.6 Rename `AgentMatch` → `AgentCardMatch` in types.rs
- [x] 1.7 Rename `AgentStatus` → `AgentCardStatus` in types.rs
- [x] 1.8 Rename `ChatAgent` trait → `AgentHandle` in traits/chat.rs
- [x] 1.9 Delete `AgentProvider` trait (unused placeholder)
- [x] 1.10 Simplify `AgentCard` fields (remove Personality, SkillLevel, Verbosity)
- [x] 1.11 Update all references across crates
- [x] 1.12 Update tests for new types

## Phase 2: Agent Card Format Specification

- [ ] 2.1 Create `openspec/specs/agent-cards.md` defining:
  - Required frontmatter fields (name, version, description)
  - Optional frontmatter fields (capabilities, tags, skills, required_tools)
  - System prompt extraction rules (# System Prompt section or full body)
  - File naming conventions
  - Directory structure (.crucible/agents/, ~/.config/crucible/agents/)
- [ ] 2.2 Create example agent cards in `examples/agent-cards/`
  - `code-reviewer.md` - Code review specialist
  - `researcher.md` - Research and summarization
  - `refactorer.md` - Code refactoring expert
- [ ] 2.3 Add validation for required fields in AgentCardLoader
- [ ] 2.4 Document frontmatter schema with examples

## Phase 3: CLI Integration

- [ ] 3.1 Add `cru agents` subcommand group
- [ ] 3.2 Implement `cru agents list` - List all registered agent cards
- [ ] 3.3 Implement `cru agents show <name>` - Display agent card details
- [ ] 3.4 Implement `cru agents validate` - Validate all agent cards
- [ ] 3.5 Auto-load agent cards on CLI startup from:
  - `.crucible/agents/` (project-specific, higher priority)
  - `~/.config/crucible/agents/` (system-wide, lower priority)

## Phase 4: Agent Card Integration with Chat

- [ ] 4.1 Add `@agent` syntax in chat to invoke agent cards
- [ ] 4.2 Inject agent card system prompt when invoked
- [ ] 4.3 Track which agent card is active in session state

## Phase 5: ACP Delegation (Future)

- [ ] 5.1 Add `acp_server` field to AgentCardFrontmatter
- [ ] 5.2 Create delegation.rs for routing to external ACP agents
- [ ] 5.3 Document delegation workflow

## Completed Commits

1. `90ffb69` - docs: add agent naming clarification design
2. `86eacef` - docs(openspec): update add-agent-system tasks
3. `fdac746` - refactor: agent naming clarification (AgentCard, AgentHandle)
