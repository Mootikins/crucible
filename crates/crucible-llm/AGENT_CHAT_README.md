# Agent Chat with Tool Calling

This document describes the LLM agent/chat/tool calling implementation in Crucible.

## Architecture (SOLID Principles)

### Dependency Inversion Principle
```
┌─────────────────────────┐
│   crucible-core         │  ← Defines abstractions (traits)
│   - ChatProvider        │
│   - ToolExecutor        │
│   - LlmError, etc.      │
└───────────┬─────────────┘
            │ depends on (implements)
            ▼
┌─────────────────────────┐
│   crucible-llm          │  ← Concrete implementations
│   - OllamaChatProvider  │
│   - OpenAIChatProvider  │
│   - AgentRuntime        │
└─────────────────────────┘
```

**Key insight**: Core defines the interface, LLM provides the implementation. This follows the Dependency Inversion Principle (SOLID).

## Test-Driven Development

We followed strict TDD workflow:

1. **Red**: Write tests first defining the contract
2. **Green**: Implement to make tests pass
3. **Refactor**: Clean up while keeping tests green

### Test Results
```bash
# Chat provider tests (8 tests)
cargo test -p crucible-llm --test chat_provider_tests

# Agent runtime tests (7 tests)
cargo test -p crucible-llm --test agent_runtime_tests

Total: 15 passing tests ✅
```

## Components

### 1. ChatProvider Trait (crucible-core)
```rust
#[async_trait]
pub trait ChatProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> LlmResult<ChatResponse>;
    fn provider_name(&self) -> &str;
    fn default_model(&self) -> &str;
    async fn health_check(&self) -> LlmResult<bool>;
}
```

**Implementations**:
- `OllamaChatProvider` - For Ollama (local LLMs)
- `OpenAIChatProvider` - For OpenAI API

### 2. ToolExecutor Trait (crucible-core)
```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value>;

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>>;
}
```

### 3. AgentRuntime (crucible-llm)

The `AgentRuntime` coordinates between `ChatProvider` and `ToolExecutor` to implement the agent loop:

```rust
pub struct AgentRuntime {
    provider: Box<dyn ChatProvider>,
    executor: Box<dyn ToolExecutor>,
    conversation: Vec<ChatMessage>,
    max_iterations: usize,
    context: ExecutionContext,
}
```

**Agent Loop**:
1. Send messages to LLM
2. LLM responds (possibly with tool calls)
3. If tool calls present, execute them via ToolExecutor
4. Add tool results to conversation
5. Send back to LLM for final response
6. Repeat until done or max iterations

## Usage

### Quick Start

```bash
# Run interactive agent chat
cargo run -p crucible-llm --example simple_agent_chat
```

### Code Example

```rust
use crucible_llm::{create_chat_provider, AgentRuntime};
use crucible_config::ChatConfig;

// Create provider from config
let config = ChatConfig::default();
let provider = create_chat_provider(&config).await?;

// Create tool executor (your implementation)
let executor = Box::new(MyToolExecutor);

// Create runtime
let mut runtime = AgentRuntime::new(provider, executor)
    .with_max_iterations(10);

// Set system prompt
runtime.set_system_prompt("You are a helpful assistant".to_string());

// Send a message
let response = runtime.send_message("What's the weather?".to_string()).await?;
println!("Assistant: {}", response.message.content);
```

## Configuration

The agent uses configuration from `~/.config/crucible/config.toml`:

```toml
[chat]
provider = "ollama"  # or "openai"
model = "llama3.2"
endpoint = "https://llama.terminal.krohnos.io"
temperature = 0.7
max_tokens = 2048
timeout_secs = 120
```

### Default Configuration

- **Provider**: Ollama
- **Endpoint**: `https://llama.terminal.krohnos.io`
- **Model**: `llama3.2`
- **Temperature**: 0.7
- **Max Tokens**: 2048
- **Timeout**: 120 seconds

## Tool Calling Flow

```
User: "What time is it?"
  │
  ├─> LLM: ChatProvider.chat(request)
  │     └─> Response: ToolCall { name: "get_current_time", params: {} }
  │
  ├─> Execute: ToolExecutor.execute_tool("get_current_time", {})
  │     └─> Result: { "time": "2025-12-01T01:00:00Z" }
  │
  ├─> LLM: ChatProvider.chat(request_with_tool_result)
  │     └─> Response: "The current time is 1:00 AM UTC"
  │
  └─> User receives final response
```

## Features

### ✅ Implemented
- [x] ChatProvider trait with Ollama and OpenAI implementations
- [x] ToolExecutor trait for tool integration
- [x] AgentRuntime for autonomous agent behavior
- [x] Tool calling support (function calling)
- [x] Conversation history tracking
- [x] Max iterations to prevent infinite loops
- [x] Comprehensive error handling
- [x] Interactive chat example
- [x] Full test coverage (15 tests)

### 🚧 Future Enhancements
- [ ] Streaming responses
- [ ] Anthropic Claude provider
- [ ] Tool approval/confirmation UI
- [ ] Persistent conversation storage
- [ ] Multi-agent collaboration
- [ ] Advanced prompt engineering

## Testing

### Run All Tests
```bash
# Chat provider tests
cargo test -p crucible-llm --test chat_provider_tests

# Agent runtime tests
cargo test -p crucible-llm --test agent_runtime_tests

# All tests
cargo test -p crucible-llm
```

### Test Coverage
- **Chat providers**: 8 tests covering basic chat, tool calling, multi-turn conversations
- **Agent runtime**: 7 tests covering agent loop, tool execution, history management

## Error Handling

All operations return `LlmResult<T>` which is `Result<T, LlmError>`:

```rust
pub enum LlmError {
    HttpError(String),
    InvalidResponse(String),
    AuthenticationError(String),
    RateLimitExceeded { retry_after_secs: u64 },
    ProviderError { provider: String, message: String },
    ConfigError(String),
    Timeout { timeout_secs: u64 },
    ModelNotFound(String),
    InvalidToolCall(String),
    Internal(String),
}
```

## Performance Considerations

- **Async/await**: All I/O operations are async for efficiency
- **Max iterations**: Prevents infinite tool calling loops (default: 10)
- **Timeout**: Configurable per-request timeout (default: 120s)
- **Connection pooling**: reqwest client reuses connections

## Security

- **API keys**: Read from environment variables (OPENAI_API_KEY)
- **Tool execution**: Sandboxed via ToolExecutor trait
- **Input validation**: Tool parameters validated before execution
- **Rate limiting**: Handled by LlmError::RateLimitExceeded

## Contributing

When adding new features:

1. **Write tests first** (TDD)
2. **Follow SOLID principles** (define traits in core)
3. **Update documentation**
4. **Ensure all tests pass**

## License

Same as Crucible project.
