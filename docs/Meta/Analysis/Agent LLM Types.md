---
description: Architecture analysis of agent handles and LLM provider infrastructure
type: analysis
system: agents
status: review
updated: 2024-12-13
tags:
  - meta
  - analysis
  - agents
---

# Agent & LLM Types Analysis

## Executive Summary

The agent and LLM infrastructure is well-architected with clear separation across three crates:
- **crucible-agents** - Internal agent handles
- **crucible-llm** - LLM provider implementations
- **crucible-acp** - External agent protocol

---

## High Priority Issues

- [ ] **Tool call type duplication** (MEDIUM)
  - Location: `crucible-agents/src/handle.rs:106-115`
  - Issue: `ChatToolCall` vs `ToolCall` creates conversion overhead
  - Impact: Every tool call requires conversion
  - Recommendation: Document distinction or unify types

- [ ] **AgentRuntime vs InternalAgentHandle duplication** (MEDIUM)
  - Location: `crucible-llm/agent_runtime.rs` vs `crucible-agents/handle.rs:164-285`
  - Issue: Similar tool loop logic in two places
  - Recommendation: Refactor InternalAgentHandle to use AgentRuntime

- [ ] **TokenBudget not integrated with ContextManager** (LOW)
  - Location: SlidingWindowContext doesn't call TokenBudget::correct()
  - Recommendation: Add correction callback to ContextManager trait

---

## Medium Priority Issues

- [ ] **LayeredPromptBuilder unused** (LOW)
  - Location: `handle.rs:36` - field is `#[allow(dead_code)]`
  - Recommendation: Either use it or remove it

- [ ] **AcpSession incomplete** (LOW)
  - Location: `session.rs:87` - Returns "Not yet implemented"
  - Status: Intentional for TDD

---

## Core Types

### AgentHandle Trait (`crucible-core/traits/chat.rs:141`)
- **Purpose**: Runtime handle to active agent (ACP, internal, or direct LLM)
- **Key methods**: `send_message_stream()`, `set_mode()`, `is_connected()`
- **Status**: ✅ Well-designed streaming-first API

### InternalAgentHandle (`crucible-agents/handle.rs:21`)
- **Purpose**: Implements AgentHandle using direct LLM API
- **Components**:
  - `provider: Box<dyn TextGenerationProvider>`
  - `context: Box<dyn ContextManager>`
  - `tools: Option<Box<dyn ToolExecutor>>`
  - `token_budget: TokenBudget`
  - `mode: ChatMode`

### TextGenerationProvider Trait (`crucible-core/traits/llm.rs:712`)
- **Purpose**: LLM provider abstraction (Ollama, OpenAI, Anthropic)
- **Implementations**: OllamaTextProvider, OpenAITextProvider
- **Status**: ✅ Comprehensive and well-designed

---

## Message Flow

```
User Input (String)
  ↓
AgentHandle::send_message_stream()
  ↓
Add to ContextManager as LlmMessage::user()
  ↓
Build ChatCompletionRequest
  ↓
TextGenerationProvider::generate_chat_completion_stream()
  ↓
Stream ChatCompletionChunk
  ↓
Convert ToolCall → ChatToolCall (handle.rs:106-115)
  ↓
Execute tools if finish_reason == "tool_calls"
  ↓
Add results as LlmMessage::tool()
  ↓
Loop until no more tool calls
  ↓
Yield ChatChunk to caller
```

---

## Architecture Strengths

1. **SOLID principles**: Clean separation between traits (core) and implementations
2. **Streaming-first API**: AgentHandle prioritizes streaming
3. **Token budget management**: Drift correction via exponential moving average
4. **Permission model**: ChatMode provides clear read-only vs write-enabled separation

---

## Recommendations

### Short Term
1. Document ChatToolCall vs ToolCall distinction
2. Add TokenBudget correction to SlidingWindowContext
3. Either use LayeredPromptBuilder or mark for removal

### Medium Term
1. Refactor InternalAgentHandle to use AgentRuntime
2. Extract streaming delta accumulation to helpers
3. Clarify ACP file ops vs MCP tools in documentation
