# internal-agent-system Specification

## Purpose

Provide a complete internal agent system for Crucible that enables chat functionality without requiring external ACP agents. Users can use local LLMs (Ollama) or direct API access (OpenAI-compatible) with agent cards providing system prompts.

## ADDED Requirements

### Requirement: LLM Provider Abstraction

The system SHALL support multiple LLM backends through a unified provider interface, enabling users to choose between local and cloud-based models.

#### Scenario: Chat with Ollama backend
- **GIVEN** user has Ollama running locally
- **AND** config specifies `[llm.ollama]` with url and model
- **WHEN** user starts chat with `--internal` flag
- **THEN** system SHALL connect to Ollama API
- **AND** system SHALL send messages using Ollama chat format
- **AND** responses SHALL be displayed in chat interface

#### Scenario: Chat with OpenAI-compatible backend
- **GIVEN** user has OpenAI API key configured
- **AND** config specifies `[llm.openai]` with url and model
- **WHEN** user starts chat with `--internal` and provider override
- **THEN** system SHALL authenticate with API key
- **AND** system SHALL use OpenAI chat completions format
- **AND** token usage SHALL be tracked from API response

#### Scenario: API key resolution
- **GIVEN** user needs to authenticate with an LLM provider
- **WHEN** system resolves API key
- **THEN** system SHALL check environment variable first (e.g., OPENAI_API_KEY)
- **AND** system SHALL check file reference from config second
- **AND** system SHALL use plaintext config value last (with warning)
- **AND** missing key for providers that require it SHALL produce clear error

### Requirement: Context Management

The system SHALL automatically manage conversation context to stay within model token limits, with manual override capability.

#### Scenario: Automatic context compaction
- **GIVEN** conversation has accumulated messages
- **AND** estimated token count exceeds 80% of model context window
- **WHEN** user sends next message
- **THEN** system SHALL compact context using active strategy
- **AND** system prompt SHALL always be preserved
- **AND** most recent messages SHALL be prioritized
- **AND** user SHALL be notified of compaction

#### Scenario: Manual compaction via /compact
- **GIVEN** user wants to free context space early
- **WHEN** user enters `/compact` command
- **THEN** system SHALL immediately run context compaction
- **AND** system SHALL report tokens before and after

#### Scenario: View context usage via /context
- **GIVEN** user wants to see current token usage
- **WHEN** user enters `/context` command
- **THEN** system SHALL display estimated context tokens
- **AND** system SHALL display actual tokens from last API response
- **AND** system SHALL display model's context window size

### Requirement: Internal Agent Handle

The system SHALL implement the AgentHandle trait using direct LLM calls, enabling internal agents as an alternative to ACP.

#### Scenario: Create internal agent from agent card
- **GIVEN** agent card exists with system prompt
- **AND** LLM provider is configured
- **WHEN** user starts chat with `--agent <name>`
- **THEN** system SHALL load agent card
- **AND** system SHALL inject system prompt into conversation
- **AND** agent card metadata SHALL be available via `/agent` command

#### Scenario: Switch agent mid-session
- **GIVEN** user is in active chat session with internal backend
- **WHEN** user enters `/agent <other-name>`
- **THEN** system SHALL load new agent card
- **AND** system SHALL update system prompt
- **AND** conversation history SHALL be preserved
- **AND** context SHALL be recomputed with new system prompt

### Requirement: Backend Selection

The system SHALL allow users to choose between internal and ACP backends at startup, with clear constraints on compatibility.

#### Scenario: Start with internal backend (default)
- **GIVEN** config has `default_backend = "internal"`
- **WHEN** user runs `cru chat`
- **THEN** system SHALL use CrucibleAgentHandle
- **AND** system SHALL load default agent card

#### Scenario: Start with ACP backend
- **GIVEN** ACP agent is available
- **WHEN** user runs `cru chat --acp claude-code`
- **THEN** system SHALL use CrucibleAcpClient
- **AND** system SHALL NOT allow `--agent` flag (incompatible)

#### Scenario: Reject incompatible options
- **GIVEN** user specifies both ACP and agent card
- **WHEN** user runs `cru chat --acp claude-code --agent general`
- **THEN** system SHALL display error message
- **AND** system SHALL explain that agent cards cannot be used with ACP
- **AND** system SHALL NOT start chat session

### Requirement: LLM Configuration

The system SHALL support flexible LLM configuration with provider-specific settings.

#### Scenario: Configure Ollama provider
- **GIVEN** user adds `[llm.ollama]` section to config
- **WHEN** config specifies url and model
- **THEN** system SHALL use specified Ollama endpoint
- **AND** system SHALL use specified model name
- **AND** no API key SHALL be required

#### Scenario: Configure OpenAI-compatible provider
- **GIVEN** user adds `[llm.openai]` section to config
- **WHEN** config specifies url, model, and API key source
- **THEN** system SHALL use specified endpoint
- **AND** system SHALL resolve API key per priority rules
- **AND** system SHALL include Authorization header in requests
