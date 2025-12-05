# Implementation Tasks

## 1. Rename Types (AgentDefinition → AgentCard)

- [ ] 1.1 Rename `AgentDefinition` → `AgentCard` in types.rs
- [ ] 1.2 Rename `AgentRegistry` → `AgentCardRegistry` in mod.rs
- [ ] 1.3 Rename `AgentLoader` → `AgentCardLoader` in loader.rs
- [ ] 1.4 Rename `AgentFrontmatter` → `AgentCardFrontmatter` in types.rs
- [ ] 1.5 Rename `AgentQuery` → `AgentCardQuery` in types.rs
- [ ] 1.6 Rename `AgentMatch` → `AgentCardMatch` in types.rs
- [ ] 1.7 Rename `AgentStatus` → `AgentCardStatus` in types.rs
- [ ] 1.8 Update module re-exports in `crucible-core/src/lib.rs`

## 2. Simplify AgentCard Fields

- [ ] 2.1 Remove `Skill.experience_years` field
- [ ] 2.2 Remove `Skill.certifications` field
- [ ] 2.3 Remove `Skill.proficiency` field (keep name + category only)
- [ ] 2.4 Remove `Personality` struct entirely
- [ ] 2.5 Remove `PersonalityFrontmatter` struct
- [ ] 2.6 Simplify `Capability` (remove skill_level, keep name + description)
- [ ] 2.7 Update `AgentCardFrontmatter` to match simplified structure
- [ ] 2.8 Update tests to use simplified fields

## 3. Rename ChatAgent → AgentHandle

- [ ] 3.1 Rename `ChatAgent` trait → `AgentHandle` in traits/chat.rs
- [ ] 3.2 Update trait re-exports in traits/mod.rs
- [ ] 3.3 Update `crucible-cli/src/chat/mod.rs` imports
- [ ] 3.4 Update `crucible-cli/src/chat/session.rs` usage
- [ ] 3.5 Update `crucible-acp/src/client.rs` impl block
- [ ] 3.6 Search for remaining `ChatAgent` references

## 4. Delete AgentProvider Trait

- [ ] 4.1 Delete `crucible-core/src/traits/agent.rs`
- [ ] 4.2 Remove from `crucible-core/src/traits/mod.rs` exports
- [ ] 4.3 Remove `AgentProvider` re-export from `crucible-core/src/lib.rs`
- [ ] 4.4 Search for any remaining AgentProvider usage

## 5. Update Tests

- [ ] 5.1 Update `crucible-core/src/agent/tests.rs` for new names
- [ ] 5.2 Update `crucible-core/src/agent/integration_test.rs`
- [ ] 5.3 Run full test suite and fix failures
- [ ] 5.4 Run clippy and fix warnings

## 6. Documentation

- [ ] 6.1 Update doc comments in renamed files
- [ ] 6.2 Update any markdown docs referencing old names
- [ ] 6.3 Commit all changes with clear message
