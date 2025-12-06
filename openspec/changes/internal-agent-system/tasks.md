# Implementation Tasks

## Phase 1: LLM Providers

- [ ] 1.1 Add `context_window()` method to `LlmProvider` trait in crucible-core
- [ ] 1.2 Create `crucible-llm/src/text_generation/mod.rs` module structure
- [ ] 1.3 Implement `OllamaProvider` with `LlmProvider` trait
- [ ] 1.4 Implement `OpenAiCompatibleProvider` with `LlmProvider` trait
- [ ] 1.5 Add API key resolution logic (env → file → plain)
- [ ] 1.6 Add unit tests for both providers (mocked HTTP)
- [ ] 1.7 Add integration test with local Ollama (ignored by default)

## Phase 2: Context Management

- [ ] 2.1 Create `ContextStrategy` trait in crucible-core
- [ ] 2.2 Create `ContextBudget` struct
- [ ] 2.3 Implement `SlidingWindowStrategy` in crucible-llm
- [ ] 2.4 Add token estimation (4 chars/token heuristic)
- [ ] 2.5 Add unit tests for context strategy

## Phase 3: CrucibleAgentHandle

- [ ] 3.1 Create `crucible-cli/src/agents/` module structure
- [ ] 3.2 Implement `CrucibleAgentHandle` struct
- [ ] 3.3 Add `ChannelContext` struct with optional fields (scaffolding for future)
- [ ] 3.4 Implement `AgentHandle` trait for `CrucibleAgentHandle`
- [ ] 3.5 Implement automatic context compaction in `send_message`
- [ ] 3.6 Implement token usage tracking from API responses
- [ ] 3.7 Create `AgentFactory` for spawning handles from cards
- [ ] 3.8 Add unit tests for handle (mocked provider)

## Phase 4: Configuration

- [ ] 4.1 Add `LlmConfig` struct to crucible-config
- [ ] 4.2 Add `[llm]` section parsing (ollama, openai subsections)
- [ ] 4.3 Update `ChatConfig` with `default_backend`, `default_agent`, `default_acp_agent`
- [ ] 4.4 Add config validation (provider URLs, model names)
- [ ] 4.5 Update example-config.toml with new sections

## Phase 5: CLI Integration

- [ ] 5.1 Add `--internal` and `--acp <agent>` flags to chat command
- [ ] 5.2 Implement backend selection logic in `ChatSession::new`
- [ ] 5.3 Add validation: error if `--acp` with `--agent` (incompatible)
- [ ] 5.4 Implement `/compact` slash command
- [ ] 5.5 Implement `/context` slash command (show token usage)
- [ ] 5.6 Implement `/agent` and `/agent <name>` slash commands
- [ ] 5.7 Add help text for new commands

## Phase 6: Documentation & Testing

- [ ] 6.1 Update CLI help for chat command
- [ ] 6.2 Add integration test: internal chat with mocked provider
- [ ] 6.3 Add integration test: agent card loading and system prompt injection
- [ ] 6.4 Manual testing with Ollama
- [ ] 6.5 Manual testing with OpenAI API

## Future TODOs (not this change)

- System keyring for API keys (`cru config set-key <provider>`)
- `/subagent` command for spawning child agents
- `SummarizationStrategy` for context compaction via LLM
- Kebab-case config migration
- Streaming response support
- Channel/workflow architecture:
  - Activate `ChannelContext` fields (channel_id, domain, isolation_level)
  - Channel routing/orchestration layer
  - Privacy boundaries between channels
  - Workflow pipeline definitions
