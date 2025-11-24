# crucible-mock-agent

Mock Agent Client Protocol (ACP) agent for testing and as a foundation for building Crucible agents.

## Overview

This crate provides a configurable mock ACP agent that can:
- Handle the full ACP handshake (initialize, session/new)
- Send streaming responses with `session/update` notifications
- Simulate various agent behaviors (OpenCode, Claude, custom)
- Test error conditions and edge cases

## Usage

### As a Library

```rust
use crucible_mock_agent::{MockAgent, MockAgentConfig, AgentBehavior};

let config = MockAgentConfig {
    behavior: AgentBehavior::Streaming,
    protocol_version: 1,
    ..Default::default()
};

let mut agent = MockAgent::new(config);
agent.run().expect("Agent failed");
```

### As a Binary

```bash
# Basic streaming agent
crucible-mock-agent --behavior streaming

# Slow streaming (for timeout tests)
crucible-mock-agent --behavior streaming-slow

# Never completes (for hang detection tests)
crucible-mock-agent --behavior streaming-incomplete
```

## Behaviors

- **`OpenCode`**: Mimics OpenCode agent responses
- **`ClaudeAcp`**: Mimics Claude ACP agent responses
- **`Streaming`**: Sends 4 content chunks then final response
- **`StreamingSlow`**: Adds delays between chunks (timeout testing)
- **`StreamingIncomplete`**: Never sends final response (hang detection)

## Architecture

This mock agent correctly implements the ACP streaming protocol:
1. Client sends `session/prompt` request with id
2. Agent sends multiple `session/update` notifications (no id)
3. Agent sends final `PromptResponse` with matching id

This is also the foundation for building a real Crucible agent that can:
- Access knowledge base via semantic search
- Execute tool calls
- Manage sessions and context
- Stream responses in real-time
