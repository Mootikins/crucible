# Internal Agent System - Architecture Design

**Date:** 2025-12-12
**Status:** Design Complete, Ready for Implementation
**Goal:** Enable chat with local LLMs and API providers without ACP, with full tool parity.

---

## Overview

The Internal Agent System enables users to chat with local LLMs (Ollama) and API providers (OpenAI, Anthropic) directly, without requiring external ACP agents. Internal agents have full access to tools via MCP gateway, identical to ACP agents.

### Key Principles

- **Local-first:** Default to internal agent with Ollama
- **Tool parity:** Internal agents access same tools as ACP agents
- **Unified initialization:** Both backends initialize identically for orchestration
- **Maximum decoupling:** Traits in core, implementations injected via DI
- **Config-driven:** Named provider instances, flexible defaults

---

## Crate Structure

### New Crate: `crucible-agents`

```
crucible-agents/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── handle.rs           # InternalAgentHandle
    ├── context/
    │   ├── mod.rs
    │   ├── manager.rs      # ContextManager trait re-export
    │   └── sliding.rs      # SlidingWindowContext
    ├── prompt/
    │   ├── mod.rs
    │   └── builder.rs      # LayeredPromptBuilder
    └── token.rs            # TokenBudget (heuristic + drift)
```

### Traits Added/Moved to `crucible-core`

```
crucible-core/src/traits/
├── chat.rs      # AgentHandle (existing, add streaming)
├── llm.rs       # TextGenerationProvider (MOVE from crucible-llm)
├── context.rs   # ContextManager (NEW)
└── tools.rs     # ToolExecutor (NEW)
```

### Dependency Flow

```
crucible-core (all traits)
    │
    ├──→ crucible-llm (provider impls: Ollama, OpenAI, Anthropic)
    │
    ├──→ crucible-agents (InternalAgentHandle, SlidingWindowContext)
    │
    ├──→ crucible-tools (ToolExecutor impl via MCP)
    │
    └──→ crucible-cli / crucible-web (wire everything together)
```

---

## Core Trait Definitions

### AgentHandle (Updated)

`crucible-core/src/traits/chat.rs`:

```rust
use futures::stream::BoxStream;

#[async_trait]
pub trait AgentHandle: Send + Sync {
    /// Stream response chunks (primary method)
    fn send_message_stream<'a>(
        &'a mut self,
        message: &'a str,
    ) -> BoxStream<'a, ChatResult<ChatChunk>>;

    /// Collect stream into full response (default impl)
    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse> {
        use futures::StreamExt;
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut stream = self.send_message_stream(message);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            content.push_str(&chunk.delta);
            if let Some(calls) = chunk.tool_calls {
                tool_calls.extend(calls);
            }
        }
        Ok(ChatResponse { content, tool_calls })
    }

    async fn set_mode(&mut self, mode: ChatMode) -> ChatResult<()>;
    fn is_connected(&self) -> bool;
}

/// Chunk from streaming response
#[derive(Debug, Clone)]
pub struct ChatChunk {
    /// Incremental text content
    pub delta: String,
    /// True when this is the final chunk
    pub done: bool,
    /// Tool calls (populated in final chunk if any)
    pub tool_calls: Option<Vec<ToolCall>>,
}
```

### TextGenerationProvider (Move to Core)

`crucible-core/src/traits/llm.rs`:

```rust
/// Provider-agnostic tool call request
pub struct ToolRequest {
    pub definitions: Vec<ToolDefinition>,
    pub choice: ToolChoice,  // Auto, Required, None, Specific
}

/// Provider-agnostic tool call response
pub struct ToolCallResponse {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,  // Already parsed
}

#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    /// Generate streaming chat completion
    fn generate_chat_completion_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
        tools: Option<ToolRequest>,
    ) -> BoxStream<'a, Result<ChatCompletionChunk>>;

    /// Generate non-streaming chat completion
    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
        tools: Option<ToolRequest>,
    ) -> Result<ChatCompletionResponse>;

    fn provider_name(&self) -> &str;
    fn default_model(&self) -> &str;
    async fn health_check(&self) -> Result<bool>;
}
```

### ContextManager (New)

`crucible-core/src/traits/context.rs`:

```rust
pub trait ContextManager: Send + Sync {
    fn set_system_prompt(&mut self, prompt: String);
    fn add_message(&mut self, msg: LlmMessage);
    fn get_messages(&self) -> &[LlmMessage];
    fn trim_to_budget(&mut self, max_tokens: usize);
    fn clear(&mut self);
    fn token_estimate(&self) -> usize;

    // Future stack operations (Rune-accessible)
    // fn checkpoint(&mut self, name: &str);
    // fn rollback(&mut self, name: &str) -> bool;
    // fn pop(&mut self, n: usize);
}
```

### ToolExecutor (New)

`crucible-core/src/traits/tools.rs`:

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, name: &str, args: serde_json::Value) -> ToolResult<serde_json::Value>;
    fn available_tools(&self) -> Vec<ToolDefinition>;
    fn tool_definitions_for_llm(&self) -> Vec<LlmToolDefinition>;
}
```

---

## InternalAgentHandle Implementation

### Core Struct

`crucible-agents/src/handle.rs`:

```rust
pub struct InternalAgentHandle {
    provider: Box<dyn TextGenerationProvider>,
    context: Box<dyn ContextManager>,
    tools: Option<Box<dyn ToolExecutor>>,
    prompt_builder: LayeredPromptBuilder,
    token_budget: TokenBudget,
    mode: ChatMode,
    model: String,
}

impl InternalAgentHandle {
    pub fn new(
        provider: Box<dyn TextGenerationProvider>,
        context: Box<dyn ContextManager>,
        tools: Option<Box<dyn ToolExecutor>>,
        prompt_builder: LayeredPromptBuilder,
        model: String,
        max_context_tokens: usize,
    ) -> Self { ... }
}
```

### Tool Execution Loop

Inside `send_message_stream`:

```rust
// Pseudo-flow:
loop {
    let request = self.build_request();
    let stream = self.provider.generate_chat_completion_stream(request, tool_request);

    // Yield content chunks
    while let Some(chunk) = stream.next().await {
        yield ChatChunk { delta: chunk.delta, ... };
    }

    // If tool calls, execute and continue
    if tool_calls.is_empty() {
        break;
    }

    for call in tool_calls {
        let result = self.tools.execute(&call.name, call.args).await?;
        self.context.add_message(LlmMessage::tool(call.id, result));
    }
    // Loop continues - LLM sees tool results
}
```

### SlidingWindowContext

`crucible-agents/src/context/sliding.rs`:

```rust
pub struct SlidingWindowContext {
    messages: VecDeque<LlmMessage>,
    system_prompt: Option<LlmMessage>,  // Never trimmed
}

impl ContextManager for SlidingWindowContext {
    fn trim_to_budget(&mut self, max_tokens: usize) {
        // Keep system_prompt + trim oldest messages until under budget
        while self.token_estimate() > max_tokens && self.messages.len() > 1 {
            self.messages.pop_front();
        }
    }
}
```

---

## Layered Prompt Builder

`crucible-agents/src/prompt/builder.rs`:

```rust
pub struct LayeredPromptBuilder {
    base_prompt: String,
    agents_md: Option<String>,      // From cwd AGENTS.md/CLAUDE.md
    agent_card: Option<String>,     // From agent card file
    user_customization: Option<String>,  // Future
}

impl LayeredPromptBuilder {
    pub fn new() -> Self {
        Self {
            base_prompt: "You are a helpful assistant.".into(),
            agents_md: None,
            agent_card: None,
            user_customization: None,
        }
    }

    pub fn with_agents_md(mut self, path: &Path) -> Self {
        // Try AGENTS.md, fall back to CLAUDE.md
        self.agents_md = Self::load_file(path.join("AGENTS.md"))
            .or_else(|| Self::load_file(path.join("CLAUDE.md")));
        self
    }

    pub fn with_agent_card(mut self, card: &AgentCard) -> Self {
        self.agent_card = card.system_prompt.clone();
        self
    }

    /// Build final system prompt (layers concatenated)
    pub fn build(&self) -> String {
        [
            Some(&self.base_prompt),
            self.agents_md.as_ref(),
            self.agent_card.as_ref(),
            self.user_customization.as_ref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
    }
}
```

**Prompt layer order (bottom to top):**

```
┌─────────────────────────────────┐
│ User customization (future)     │ ← highest priority
├─────────────────────────────────┤
│ Agent card system prompt        │ ← if specified
├─────────────────────────────────┤
│ AGENTS.md / CLAUDE.md           │ ← if present in cwd
├─────────────────────────────────┤
│ Base prompt (minimal default)   │ ← always present
└─────────────────────────────────┘
```

---

## Token Budget Management

`crucible-agents/src/token.rs`:

```rust
pub struct TokenBudget {
    max_tokens: usize,
    estimated_used: usize,      // heuristic (chars / 4)
    last_actual: Option<usize>, // from API response
    drift_factor: f32,          // calibration multiplier
}

impl TokenBudget {
    pub fn estimate_tokens(text: &str) -> usize {
        text.len() / 4  // simple heuristic
    }

    pub fn update_from_response(&mut self, usage: TokenUsage) {
        self.last_actual = Some(usage.prompt_tokens as usize);
        // Adjust drift_factor based on estimate vs actual
        if self.estimated_used > 0 {
            self.drift_factor = usage.prompt_tokens as f32 / self.estimated_used as f32;
        }
    }

    pub fn adjusted_estimate(&self, text: &str) -> usize {
        (Self::estimate_tokens(text) as f32 * self.drift_factor) as usize
    }
}
```

---

## Configuration Structure

### Config File Example

`crucible.toml`:

```toml
[chat]
default_backend = "internal"    # "internal" or "acp"
default_provider = "local"      # Which llm.providers.* to use
default_agent = "assistant"     # Agent card name (optional)
default_acp_agent = "claude"    # For ACP backend

[llm]
default = "local"               # Default provider name

[llm.providers.local]
type = "ollama"
endpoint = "http://localhost:11434"
default_model = "llama3.2"
timeout_secs = 120

[llm.providers.cloud]
type = "ollama"
endpoint = "https://llama.krohnos.io"
default_model = "qwen3-4b"

[llm.providers.openai-work]
type = "openai"
api_key = "${OPENAI_WORK_KEY}"
default_model = "gpt-4o"

[llm.providers.openai-personal]
type = "openai"
api_key = "${OPENAI_KEY}"
default_model = "gpt-4o-mini"
```

### CLI Flags

```
cru chat                           # Uses config defaults (internal)
cru chat --provider cloud          # Override provider
cru chat --acp claude              # Force ACP backend
cru chat --agent researcher        # Use specific agent card
cru chat --model llama3.2:70b      # Override model
```

---

## Unified Session Initialization

Both ACP and internal agents initialize identically:

```rust
// crucible-cli/src/commands/chat.rs

pub async fn run_chat(args: ChatArgs, config: &Config) -> Result<()> {
    // 1. Resolve tools (same for both backends)
    let tools: Box<dyn ToolExecutor> = create_tool_executor(
        &config,
        args.kiln_path.as_ref(),
    ).await?;

    // 2. Create agent handle (polymorphic)
    let handle: Box<dyn AgentHandle> = match resolve_backend(&args, config)? {
        Backend::Internal { provider_name, model } => {
            let provider = resolve_provider(config, Some(&provider_name))?;
            let context = SlidingWindowContext::new();
            let prompt = LayeredPromptBuilder::new()
                .with_agents_md(&std::env::current_dir()?)
                .with_agent_card_if_present(&args.agent, config)?;

            Box::new(InternalAgentHandle::new(
                provider,
                Box::new(context),
                Some(tools),
                prompt,
                model,
                config.llm.max_context_tokens,
            ))
        }
        Backend::Acp { agent_name } => {
            Box::new(CrucibleAcpClient::new(agent_name, tools))
        }
    };

    // 3. Run session (identical for both)
    let session_config = SessionConfig::from_args(&args, config);
    let mut session = ChatSession::new(handle, session_config);
    session.run().await
}
```

---

## Testing Strategy (TDD)

### Test Layers

```
Unit Tests (crucible-agents)
├── context/sliding_test.rs      # Trim behavior, token estimation
├── prompt/builder_test.rs       # Layer merging, file loading
├── handle_test.rs               # Mock provider, tool execution loop
└── token_test.rs                # Budget tracking, drift correction

Integration Tests (crucible-agents/tests/)
├── provider_integration.rs      # Real Ollama (ignored by default)
├── tool_loop_integration.rs     # Multi-turn tool calling
└── streaming_integration.rs     # BoxStream behavior

Contract Tests (crucible-cli/tests/)
├── session_parity.rs            # ACP vs Internal same behavior
└── config_resolution.rs         # Provider selection logic
```

### Example Tests

```rust
#[test]
fn trim_keeps_system_prompt() {
    let mut ctx = SlidingWindowContext::new();
    ctx.set_system_prompt("You are helpful.".into());
    ctx.add_message(LlmMessage::user("msg1".repeat(1000)));
    ctx.add_message(LlmMessage::user("msg2".repeat(1000)));

    ctx.trim_to_budget(500);

    assert!(ctx.get_messages()[0].content.contains("helpful"));
    assert_eq!(ctx.get_messages().len(), 2);  // system + 1 msg
}

#[test]
fn tool_loop_executes_until_no_calls() {
    let mock_provider = MockProvider::new()
        .respond_with_tool_call("search", json!({"q": "test"}))
        .then_respond_with_text("Found results.");

    let mock_tools = MockToolExecutor::new()
        .on("search", |_| json!({"results": ["a", "b"]}));

    let mut handle = InternalAgentHandle::new(
        Box::new(mock_provider),
        Box::new(SlidingWindowContext::new()),
        Some(Box::new(mock_tools)),
        LayeredPromptBuilder::new(),
        "test-model".into(),
        4096,
    );

    let response = block_on(handle.send_message("search for test"))?;

    assert!(response.content.contains("Found results"));
    assert_eq!(mock_tools.call_count("search"), 1);
}
```

---

## Implementation Tasks (Topologically Sorted)

### Phase 1: Core Traits (Parallel)

| Task | Description | Deps |
|------|-------------|------|
| 1A | Move TextGenerationProvider trait to crucible-core | - |
| 1B | Add ContextManager trait to crucible-core | - |
| 1C | Add ToolExecutor trait to crucible-core | - |
| 1D | Update AgentHandle with BoxStream streaming | - |

### Phase 2: Crate Setup (Parallel, after Phase 1)

| Task | Description | Deps |
|------|-------------|------|
| 2A | Create crucible-agents crate skeleton | 1A-D |
| 2B | Update crucible-llm to use traits from core | 1A |

### Phase 3: Implementations (Sequential dependencies)

| Task | Description | Deps |
|------|-------------|------|
| 3A | Implement SlidingWindowContext | 2A, 1B |
| 3B | Implement LayeredPromptBuilder | 2A |
| 3C | Implement TokenBudget | 2A |
| 3D | Implement InternalAgentHandle | 3A, 3B, 3C |

**Parallelization:** 3A, 3B, 3C can run in parallel; 3D waits for all.

### Phase 4: Integration (Sequential dependencies)

| Task | Description | Deps |
|------|-------------|------|
| 4A | Config schema updates for named providers | 2A |
| 4B | Provider resolution logic | 4A, 2B |
| 4C | Unified session initialization | 3D, 4B |
| 4D | CLI flags & command updates | 4C |

**Parallelization:** 4A, 4B can start in parallel.

### Phase 5: Polish (Parallel, after Phase 4)

| Task | Description | Deps |
|------|-------------|------|
| 5A | Streaming implementation in providers | 4D |
| 5B | Tool format abstraction per provider | 4D |

---

## Migration Path

### Step 1: Add Internal Backend (Non-breaking)

- Internal agent works alongside ACP
- Default remains ACP (no user impact)
- Flag: `cru chat --internal` for opt-in testing

### Step 2: Update Config Schema (Non-breaking)

- Add `[llm.providers.*]` sections
- Add `[chat].default_backend` option
- Existing configs continue working (ACP default)

### Step 3: Flip Default (Breaking, announced)

- `default_backend = "internal"` becomes default
- Users with no Ollama get helpful error message
- `cru chat --acp <agent>` for ACP users

### Step 4: Deprecation Notices (Optional)

- Warn if using old config patterns
- Suggest migration to named providers

### Backwards Compatibility

```rust
fn resolve_backend(config: &Config) -> Backend {
    if let Some(backend) = &config.chat.default_backend {
        // New style - explicit backend
        parse_backend(backend)
    } else if config.llm.providers.is_empty() {
        // Legacy - no providers configured, try internal with defaults
        Backend::Internal {
            provider_name: "default".into(),
            model: "llama3.2".into(),
        }
    } else {
        // Has providers but no explicit default - use first
        Backend::Internal { ... }
    }
}
```

---

## Future Considerations (Not in this change)

- **Context stack operations:** `checkpoint`, `rollback`, `pop` for Rune plugins
- **Summarization strategy:** LLM-based context compaction
- **Multi-provider routing:** Different providers for different tasks
- **Agent orchestration:** Multiple agents coordinating via internal system

---

## SOLID Compliance Summary

| Principle | Application |
|-----------|-------------|
| **S**ingle Responsibility | Separate modules: handle (runtime), context (history), prompt (building), token (budget) |
| **O**pen/Closed | `ContextManager` trait - new strategies without modifying SlidingWindow |
| **L**iskov Substitution | `InternalAgentHandle` fully substitutes for any `AgentHandle` |
| **I**nterface Segregation | Traits stay minimal; `ContextManager` in agents crate, not core |
| **D**ependency Inversion | All concrete types injected via `Box<dyn Trait>` |

---

## References

- [Zed Editor LLM Integration](https://github.com/zed-industries/zed)
- [BoxStream Pattern](https://docs.rs/futures/latest/futures/stream/type.BoxStream.html)
- [async-openai](https://github.com/64bit/async-openai)
- [OpenSpec: internal-agent-system](../../../openspec/changes/internal-agent-system/proposal.md)
