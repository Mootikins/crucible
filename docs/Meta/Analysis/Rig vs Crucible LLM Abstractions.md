# Rig vs Crucible LLM Abstraction Comparison

**Date**: 2024-12-24
**Analysis**: Comparison of Rig v0.27 and Crucible's LLM/agent abstraction layers
**Purpose**: Identify superior patterns and recommend which abstractions to adopt

---

## Executive Summary

**Recommendation**: Adopt several of Rig's design patterns while keeping Crucible's capability-based provider traits.

**Key Takeaways**:
- Rig has superior trait design (Prompt/Chat/Completion hierarchy)
- Rig's hook system is more flexible than Crucible's current approach
- Crucible's capability-based providers (CanEmbed/CanChat) are cleaner than Rig's all-in-one CompletionModel
- Rig's streaming implementation is more robust
- Rig's tool execution loop with hooks is production-ready
- Both handle context management differently - hybrid approach recommended

---

## 1. Trait Design & Separation of Concerns

### Rig's Hierarchy (SUPERIOR)

```rust
// Three-level abstraction pyramid
pub trait Prompt {
    fn prompt(&self, prompt: impl Into<Message>)
        -> impl IntoFuture<Output = Result<String, PromptError>>;
}

pub trait Chat {
    fn chat(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> impl IntoFuture<Output = Result<String, PromptError>>;
}

pub trait Completion<M: CompletionModel> {
    async fn completion(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> Result<CompletionRequestBuilder<M>, CompletionError>;
}
```

**Strengths**:
- ✅ Clear separation: simple prompt → chat with history → low-level builder
- ✅ Type-level contract enforcement (IntoFuture for high-level traits)
- ✅ Allows users to choose abstraction level
- ✅ Agents implement all three, providing flexibility

### Crucible's Current Approach

```rust
pub trait AgentHandle: Send + Sync {
    fn send_message_stream(&mut self, message: String)
        -> BoxStream<'static, ChatResult<ChatChunk>>;

    async fn send_message(&mut self, message: &str) -> ChatResult<ChatResponse>;
    fn is_connected(&self) -> bool;
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()>;
}
```

**Weaknesses**:
- ❌ Single monolithic trait mixes concerns (streaming, connection state, mode)
- ❌ No separation between simple prompt vs. chat with history
- ❌ Builder pattern not exposed at trait level

**Winner**: **Rig** - Better adherence to Interface Segregation Principle

---

## 2. Provider Abstraction

### Crucible's Capability-Based Design (SUPERIOR)

```rust
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn backend_type(&self) -> BackendType;
    fn capabilities(&self) -> ExtendedCapabilities;
    async fn health_check(&self) -> LlmResult<bool>;
}

pub trait CanEmbed: Provider {
    async fn embed(&self, text: &str) -> LlmResult<EmbeddingResponse>;
    async fn embed_batch(&self, texts: Vec<String>) -> LlmResult<Vec<EmbeddingResponse>>;
    fn embedding_dimensions(&self) -> usize;
    fn embedding_model(&self) -> &str;
}

pub trait CanChat: Provider {
    async fn chat(&self, request: ChatCompletionRequest) -> LlmResult<ChatCompletionResponse>;
    fn chat_stream<'a>(&'a self, request: ChatCompletionRequest)
        -> BoxStream<'a, LlmResult<ChatCompletionChunk>>;
    fn chat_model(&self) -> &str;
}

pub trait CanConstrainGeneration: Provider {
    fn supported_formats(&self) -> Vec<SchemaFormat>;
    async fn generate_constrained(&self, request: ConstrainedRequest)
        -> LlmResult<ConstrainedResponse>;
}
```

**Strengths**:
- ✅ Compile-time capability discovery
- ✅ FastEmbed can implement only CanEmbed
- ✅ Anthropic can implement only CanChat
- ✅ Ollama/OpenAI implement both
- ✅ Easy to add new capabilities (CanConstrainGeneration)
- ✅ No trait object casting needed

### Rig's Monolithic Approach

```rust
pub trait CompletionModel: Clone + WasmCompatSend + WasmCompatSync {
    type Response: WasmCompatSend + WasmCompatSync + Serialize + DeserializeOwned;
    type StreamingResponse: Clone + Unpin + GetTokenUsage;
    type Client;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self;

    async fn completion(&self, request: CompletionRequest)
        -> Result<CompletionResponse<Self::Response>, CompletionError>;

    async fn stream(&self, request: CompletionRequest)
        -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>;

    fn completion_request(&self, prompt: impl Into<Message>) -> CompletionRequestBuilder<Self>;
}
```

**Weaknesses**:
- ❌ All providers must implement completion + streaming (even if not supported)
- ❌ No separation for embedding-only providers
- ❌ Runtime feature flags needed instead of compile-time traits

**Winner**: **Crucible** - Better SOLID compliance (Interface Segregation)

---

## 3. Streaming Architecture

### Rig's Streaming (SUPERIOR)

```rust
pub enum RawStreamingChoice<R> {
    Message(String),
    ToolCall(RawStreamingToolCall),
    ToolCallDelta { id: String, delta: String },
    Reasoning { id: Option<String>, reasoning: String, signature: Option<String> },
    ReasoningDelta { id: Option<String>, reasoning: String },
    FinalResponse(R),
}

pub struct StreamingCompletionResponse<R> {
    inner: Abortable<StreamingResult<R>>,
    abort_handle: AbortHandle,
    pause_control: PauseControl,
    text: String,
    reasoning: String,
    tool_calls: Vec<ToolCall>,
    pub choice: OneOrMany<AssistantContent>,
    pub response: Option<R>,
}

impl StreamingCompletionResponse {
    pub fn cancel(&self) { self.abort_handle.abort(); }
    pub fn pause(&self) { self.pause_control.pause(); }
    pub fn resume(&self) { self.pause_control.resume(); }
}
```

**Strengths**:
- ✅ Strongly typed streaming chunks (Message, ToolCall, ToolCallDelta, Reasoning)
- ✅ Built-in cancellation (AbortHandle)
- ✅ Built-in pause/resume (PauseControl)
- ✅ Accumulates state (text, tool_calls) during streaming
- ✅ Supports reasoning traces (OpenAI o1, Gemini)
- ✅ Final response typed separately from chunks

### Crucible's Streaming

```rust
pub struct ChatChunk {
    pub delta: String,
    pub done: bool,
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

fn send_message_stream(&mut self, message: String)
    -> BoxStream<'static, ChatResult<ChatChunk>>;
```

**Weaknesses**:
- ❌ Single ChatChunk type (no structured variants)
- ❌ No built-in cancellation mechanism
- ❌ No pause/resume
- ❌ No reasoning trace support
- ❌ State accumulation happens in InternalAgentHandle (not reusable)

**Winner**: **Rig** - More flexible, production-ready streaming

---

## 4. Tool Execution & Hooks

### Rig's Hook System (SUPERIOR)

```rust
pub trait PromptHook<M>: Clone + WasmCompatSend + WasmCompatSync {
    async fn on_completion_call(&self, prompt: &Message, history: &[Message],
                                  cancel_sig: CancelSignal);
    async fn on_completion_response(&self, prompt: &Message,
                                     response: &CompletionResponse<M::Response>,
                                     cancel_sig: CancelSignal);
    async fn on_tool_call(&self, tool_name: &str, tool_call_id: Option<String>,
                          args: &str, cancel_sig: CancelSignal);
    async fn on_tool_result(&self, tool_name: &str, tool_call_id: Option<String>,
                            args: &str, result: &str, cancel_sig: CancelSignal);
}

pub trait StreamingPromptHook<M>: Clone + WasmCompatSend + WasmCompatSync {
    async fn on_stream_start(&self, prompt: &Message, history: &[Message],
                             cancel_sig: CancelSignal);
    async fn on_stream_chunk(&self, chunk: &StreamedAssistantContent<M::StreamingResponse>,
                             cancel_sig: CancelSignal);
    async fn on_stream_complete(&self, response: &OneOrMany<AssistantContent>,
                                cancel_sig: CancelSignal);
    async fn on_tool_call(&self, tool_name: &str, tool_call_id: Option<String>,
                          args: &str, cancel_sig: CancelSignal);
    async fn on_tool_result(&self, tool_name: &str, tool_call_id: Option<String>,
                            args: &str, result: &str, cancel_sig: CancelSignal);
}
```

**Strengths**:
- ✅ Separate hooks for streaming vs non-streaming
- ✅ Unified cancellation via CancelSignal
- ✅ Per-request hooks (attached via `.with_hook()`)
- ✅ All lifecycle events covered
- ✅ Empty default implementations (optional overrides)

### Crucible's Approach

```rust
// No hook trait - hardcoded in InternalAgentHandle
async fn execute_tool_calls(
    context: &Arc<Mutex<Box<dyn ContextManager>>>,
    tools: &Arc<Box<dyn ToolExecutor>>,
    tool_calls: &[LlmToolCall],
) -> ChatResult<()> {
    // Tool execution is hardcoded, no hooks
}
```

**Weaknesses**:
- ❌ No hook abstraction
- ❌ Can't observe tool execution without modifying core code
- ❌ No cancellation mechanism
- ❌ ACP traits (SessionManager, ToolBridge) don't provide lifecycle hooks

**Winner**: **Rig** - Production-grade observability

---

## 5. Context Management

### Rig's Approach

```rust
pub struct Agent<M: CompletionModel> {
    pub model: Arc<M>,
    pub preamble: Option<String>,
    pub static_context: Vec<Document>,
    pub dynamic_context: Arc<RwLock<Vec<(usize, Box<dyn VectorStoreIndexDyn>)>>>,
    pub tool_server_handle: ToolServerHandle,
    // ...
}

// Context resolution happens in completion():
async fn completion(&self, prompt: Message, chat_history: Vec<Message>)
    -> Result<CompletionRequestBuilder<M>, CompletionError>
{
    // RAG: sample from dynamic_context based on prompt
    let dynamic_docs = fetch_rag_documents(&self.dynamic_context, &prompt).await?;
    // Build request with static + dynamic context
    self.model.completion_request(prompt)
        .documents(self.static_context.clone())
        .documents(dynamic_docs)
        .tools(tools)
}
```

**Strengths**:
- ✅ Static vs dynamic context separation
- ✅ RAG built into agent abstraction
- ✅ Context resolved lazily (per-request)
- ✅ Vector store agnostic (VectorStoreIndexDyn)

### Crucible's Approach

```rust
pub trait ContextManager: Send + Sync {
    fn set_system_prompt(&mut self, prompt: String);
    fn add_message(&mut self, message: LlmMessage);
    fn get_messages(&self) -> Vec<LlmMessage>;
    fn trim_to_budget(&mut self, max_tokens: usize);
    fn token_estimate(&self) -> usize;
}

pub struct SlidingWindowContext {
    system_prompt: Option<String>,
    messages: VecDeque<LlmMessage>,
    max_context_tokens: usize,
}
```

**Strengths**:
- ✅ Explicit token budget management
- ✅ Sliding window for long conversations
- ✅ Trait-based (can implement custom strategies)

**Weaknesses**:
- ❌ No RAG built-in (must be handled externally)
- ❌ No static vs dynamic context distinction
- ❌ Tightly coupled to InternalAgentHandle

**Winner**: **Rig** - Better RAG integration. Crucible has better token budget tracking.

---

## 6. Multi-Turn Tool Execution

### Rig's Loop (SUPERIOR)

```rust
async fn send(self) -> Result<PromptResponse, PromptError> {
    let mut current_max_depth = 0;

    loop {
        if current_max_depth > self.max_depth + 1 {
            return Err(PromptError::MaxDepthError { ... });
        }
        current_max_depth += 1;

        // Call completion
        let resp = agent.completion(prompt, chat_history).await?.send().await?;
        usage += resp.usage;

        // Partition into tool calls vs text
        let (tool_calls, texts) = resp.choice.iter().partition(is_tool_call);

        chat_history.push(Message::Assistant { content: resp.choice });

        if tool_calls.is_empty() {
            return Ok(PromptResponse::new(merged_texts, usage));
        }

        // Execute tools concurrently with buffer_unordered
        let tool_results = stream::iter(tool_calls)
            .map(|call| async { execute_tool(call).await })
            .buffer_unordered(self.concurrency)
            .collect()
            .await?;

        chat_history.push(Message::User { content: tool_results });
    }
}
```

**Strengths**:
- ✅ Automatic multi-turn loops (configurable depth)
- ✅ Concurrent tool execution (buffer_unordered with configurable concurrency)
- ✅ Accumulates token usage across turns
- ✅ Proper error on max depth exceeded
- ✅ Hooks fired at each lifecycle stage
- ✅ Cancellation checked between turns

### Crucible's Approach

```rust
// InternalAgentHandle - single loop iteration, no automatic retry
loop {
    if tool_iteration >= max_tool_iterations {
        yield Err(ChatError::Internal("Max iterations exceeded"));
        return;
    }

    // Stream completion
    let chunks: Vec<_> = provider.generate_chat_completion_stream(request).collect().await;

    // Accumulate chunks
    // ...

    // Execute tools if present
    if !accumulated_tool_calls.is_empty() && finish_reason == "tool_calls" {
        if let Some(tool_executor) = tools {
            Self::execute_tool_calls(&context, tool_executor, &accumulated_tool_calls).await?;
        }
        // Continue loop
    } else {
        break; // Done
    }
}
```

**Weaknesses**:
- ❌ Sequential tool execution (no concurrency)
- ❌ Max iterations hardcoded (DEFAULT_MAX_TOOL_ITERATIONS = 10)
- ❌ No hooks for observability
- ❌ Token usage not tracked across turns

**Winner**: **Rig** - Production-ready agentic loop

---

## 7. Error Handling

### Rig's Error Hierarchy (SUPERIOR)

```rust
#[derive(Debug, Error)]
pub enum CompletionError {
    #[error("HttpError: {0}")]
    HttpError(#[from] http_client::Error),
    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("ProviderError: {0}")]
    ProviderError(String),
    // ...
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("CompletionError: {0}")]
    CompletionError(#[from] CompletionError),
    #[error("ToolCallError: {0}")]
    ToolError(#[from] ToolSetError),
    #[error("MaxDepthError: (reached limit: {max_depth})")]
    MaxDepthError { max_depth: usize, chat_history: Box<Vec<Message>>, prompt: Box<Message> },
    #[error("PromptCancelled")]
    PromptCancelled { chat_history: Box<Vec<Message>> },
}

#[derive(Debug, Error)]
pub enum ToolSetError {
    #[error("ToolCallError: {0}")]
    ToolCallError(#[from] ToolError),
    #[error("ToolNotFoundError: {0}")]
    ToolNotFoundError(String),
    #[error("Tool call interrupted")]
    Interrupted,
}
```

**Strengths**:
- ✅ Three-level error hierarchy (Completion → Prompt → ToolSet)
- ✅ MaxDepthError includes full context for recovery
- ✅ PromptCancelled preserves chat history
- ✅ Interrupted for graceful cancellation

### Crucible's Errors

```rust
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ChatError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Communication error: {0}")]
    Communication(String),
    #[error("Mode change error: {0}")]
    ModeChange(String),
    #[error("Unknown command: {0}")]
    UnknownCommand(String),
    // ... 10 variants total
}
```

**Weaknesses**:
- ❌ Flat structure (no composition)
- ❌ String-based error details (not structured)
- ❌ No context preservation for recovery

**Winner**: **Rig** - Better structured error handling

---

## 8. Type Safety & Builder Patterns

### Rig's Request Builder (SUPERIOR)

```rust
pub struct CompletionRequestBuilder<M: CompletionModel> {
    model: M,
    prompt: Message,
    preamble: Option<String>,
    chat_history: Vec<Message>,
    documents: Vec<Document>,
    tools: Vec<ToolDefinition>,
    temperature: Option<f64>,
    max_tokens: Option<u64>,
    tool_choice: Option<ToolChoice>,
    additional_params: Option<serde_json::Value>,
}

impl CompletionRequestBuilder {
    pub fn preamble(mut self, preamble: String) -> Self { ... }
    pub fn document(mut self, document: Document) -> Self { ... }
    pub fn tool(mut self, tool: ToolDefinition) -> Self { ... }
    pub fn temperature(mut self, temperature: f64) -> Self { ... }

    pub fn build(self) -> CompletionRequest { ... }
    pub async fn send(self) -> Result<CompletionResponse<M::Response>, CompletionError> { ... }
}
```

**Strengths**:
- ✅ Fluent builder API
- ✅ Type-safe model tracking (M: CompletionModel)
- ✅ Can build() for inspection or send() directly
- ✅ Agent.completion() returns builder (low-level control)
- ✅ Agent.prompt() returns PromptRequest (high-level control)

### Crucible's Request Types

```rust
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub tools: Option<Vec<Tool>>,
}

impl ChatCompletionRequest {
    pub fn new(model: String, messages: Vec<LlmMessage>) -> Self { ... }
}

// No builder - direct construction
let request = ChatCompletionRequest {
    model: "gpt-4".to_string(),
    messages: vec![...],
    temperature: Some(0.7),
    // ...
};
```

**Weaknesses**:
- ❌ No builder pattern
- ❌ Direct struct construction (verbose)
- ❌ No type-level model tracking

**Winner**: **Rig** - Better API ergonomics

---

## 9. Agent Configuration

### Rig's AgentBuilder (SUPERIOR)

```rust
let agent = openai.agent("gpt-4o")
    .preamble("You are a helpful assistant")
    .context("Static document 1")
    .context("Static document 2")
    .dynamic_context(5, vector_store_index)  // RAG: top-5 results
    .tool(weather_tool)
    .tool(calculator_tool)
    .dynamic_tools(3, tool_embeddings, toolset)  // RAG: top-3 tools
    .temperature(0.7)
    .max_tokens(1000)
    .tool_choice(ToolChoice::Auto)
    .build();
```

**Strengths**:
- ✅ Fluent builder with all configuration options
- ✅ Static + dynamic context built-in
- ✅ Static + dynamic tools built-in
- ✅ Clear separation of concerns

### Crucible's Approach

```rust
let handle = InternalAgentHandle::new(
    provider,
    context,
    tools,
    prompt_builder,
    model,
    max_context_tokens,
);
```

**Weaknesses**:
- ❌ Constructor with 6 parameters (not fluent)
- ❌ No built-in RAG configuration
- ❌ Tools/context passed as already-constructed objects

**Winner**: **Rig** - Better UX

---

## 10. Testing & Mocking

### Rig's Approach

```rust
// Implement CompletionModel for mocks
struct MockModel { responses: Vec<String> }

impl CompletionModel for MockModel {
    type Response = MockResponse;
    type StreamingResponse = MockStreamingResponse;
    type Client = ();

    async fn completion(&self, request: CompletionRequest)
        -> Result<CompletionResponse<Self::Response>, CompletionError>
    {
        // Return canned response
    }
}

// Test agents with mock models
#[test]
fn test_agent_with_mock() {
    let mock = MockModel::new();
    let agent = AgentBuilder::new(mock)
        .preamble("Test")
        .build();

    let result = agent.prompt("Hello").await.unwrap();
    assert_eq!(result, "Mocked response");
}
```

**Strengths**:
- ✅ Easy to implement mocks (single trait: CompletionModel)
- ✅ Agents are testable in isolation

### Crucible's Approach

```rust
// Must implement Provider + CanChat + CanEmbed + ...
struct MockProvider { ... }

#[async_trait]
impl Provider for MockProvider { ... }

#[async_trait]
impl CanChat for MockProvider { ... }

// Testing requires constructing full handle
#[test]
async fn test_handle() {
    let provider = Box::new(MockProvider::new());
    let context = Box::new(SlidingWindowContext::new(1000));
    let handle = InternalAgentHandle::new(provider, context, None, ...);

    // Test via handle
}
```

**Weaknesses**:
- ❌ Must implement multiple traits for mocks
- ❌ Handle construction verbose in tests

**Winner**: **Rig** - Simpler test setup

---

## Comparison Summary Table

| Aspect | Rig | Crucible | Winner |
|--------|-----|----------|--------|
| **Trait Design** | 3-level hierarchy (Prompt/Chat/Completion) | Monolithic AgentHandle | **Rig** |
| **Provider Abstraction** | Monolithic CompletionModel | Capability-based (CanEmbed/CanChat) | **Crucible** |
| **Streaming** | Typed chunks, pause/resume, cancellation | Simple ChatChunk, no control | **Rig** |
| **Tool Hooks** | PromptHook + StreamingPromptHook | None (hardcoded) | **Rig** |
| **Context Management** | Static + dynamic RAG | SlidingWindowContext (token budget) | **Hybrid** |
| **Multi-Turn Loop** | Automatic, concurrent tools, depth limit | Manual, sequential tools | **Rig** |
| **Error Handling** | 3-level hierarchy, structured | Flat, string-based | **Rig** |
| **Builder Pattern** | Fluent CompletionRequestBuilder | Direct construction | **Rig** |
| **Agent Configuration** | Fluent AgentBuilder | Constructor params | **Rig** |
| **Testing** | Easy mocks | Multi-trait mocks | **Rig** |

---

## Recommended Adoption Strategy

### Phase 1: Core Trait Refactor

**Adopt from Rig**:
1. **Trait Hierarchy**: Replace `AgentHandle` with `Prompt`, `Chat`, `Completion` traits
2. **Streaming Types**: Adopt `RawStreamingChoice`, `StreamingCompletionResponse` with pause/cancel
3. **Hook System**: Add `PromptHook` and `StreamingPromptHook` traits

**Keep from Crucible**:
1. **Provider Traits**: Keep `Provider`, `CanEmbed`, `CanChat`, `CanConstrainGeneration`
2. **Capability Discovery**: Keep compile-time capability checking

**Implementation**:

```rust
// New trait hierarchy (Rig-inspired)
pub trait Prompt: Send + Sync {
    fn prompt(&self, prompt: impl Into<Message>)
        -> impl IntoFuture<Output = Result<String, PromptError>>;
}

pub trait Chat: Send + Sync {
    fn chat(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> impl IntoFuture<Output = Result<String, PromptError>>;
}

pub trait Completion<P: CanChat> {
    async fn completion(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> Result<CompletionRequestBuilder<P>, CompletionError>;
}

// Keep capability-based providers
pub trait Provider: Send + Sync { ... }
pub trait CanEmbed: Provider { ... }
pub trait CanChat: Provider { ... }

// Agent implements all three high-level traits
impl<P: CanChat> Prompt for Agent<P> {
    fn prompt(&self, prompt: impl Into<Message>) -> PromptRequest<'_, P, ()> {
        PromptRequest::new(self, prompt)
    }
}

impl<P: CanChat> Chat for Agent<P> {
    async fn chat(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> Result<String, PromptError>
    {
        PromptRequest::new(self, prompt)
            .with_history(chat_history)
            .await
    }
}

impl<P: CanChat> Completion<P> for Agent<P> {
    async fn completion(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> Result<CompletionRequestBuilder<P>, CompletionError>
    {
        // Build request with RAG, tools, etc.
    }
}
```

### Phase 2: Streaming Refactor

**Adopt from Rig**:

```rust
// Typed streaming chunks
pub enum StreamingChoice {
    Message(String),
    ToolCall(ToolCall),
    ToolCallDelta { id: String, delta: String },
    ReasoningTrace { id: String, reasoning: String },
    FinalResponse(ChatCompletionResponse),
}

// Streaming response with control
pub struct StreamingResponse<P: CanChat> {
    inner: Abortable<BoxStream<'static, Result<StreamingChoice, ChatError>>>,
    abort_handle: AbortHandle,
    pause_control: PauseControl,
    // Accumulated state
    text: String,
    tool_calls: Vec<ToolCall>,
    pub choice: AssistantContent,
    pub response: Option<ChatCompletionResponse>,
}

impl<P: CanChat> StreamingResponse<P> {
    pub fn cancel(&self) { self.abort_handle.abort(); }
    pub fn pause(&self) { self.pause_control.pause(); }
    pub fn resume(&self) { self.pause_control.resume(); }
}

// Streaming traits
pub trait StreamingPrompt<P: CanChat> {
    fn stream_prompt(&self, prompt: impl Into<Message>) -> StreamingPromptRequest<P, ()>;
}

pub trait StreamingChat<P: CanChat> {
    fn stream_chat(&self, prompt: impl Into<Message>, chat_history: Vec<Message>)
        -> StreamingPromptRequest<P, ()>;
}
```

### Phase 3: Hook System

**Adopt from Rig**:

```rust
pub trait PromptHook<P: CanChat>: Clone + Send + Sync {
    async fn on_completion_call(&self, prompt: &Message, history: &[Message],
                                  cancel_sig: CancelSignal) {}

    async fn on_completion_response(&self, prompt: &Message,
                                     response: &ChatCompletionResponse,
                                     cancel_sig: CancelSignal) {}

    async fn on_tool_call(&self, tool_name: &str, tool_call_id: Option<String>,
                          args: &str, cancel_sig: CancelSignal) {}

    async fn on_tool_result(&self, tool_name: &str, tool_call_id: Option<String>,
                            args: &str, result: &str, cancel_sig: CancelSignal) {}
}

impl<P: CanChat> PromptHook<P> for () {}

pub trait StreamingPromptHook<P: CanChat>: Clone + Send + Sync {
    async fn on_stream_start(&self, prompt: &Message, history: &[Message],
                             cancel_sig: CancelSignal) {}

    async fn on_stream_chunk(&self, chunk: &StreamingChoice,
                             cancel_sig: CancelSignal) {}

    async fn on_stream_complete(&self, response: &AssistantContent,
                                cancel_sig: CancelSignal) {}

    async fn on_tool_call(&self, tool_name: &str, tool_call_id: Option<String>,
                          args: &str, cancel_sig: CancelSignal) {}

    async fn on_tool_result(&self, tool_name: &str, tool_call_id: Option<String>,
                            args: &str, result: &str, cancel_sig: CancelSignal) {}
}

impl<P: CanChat> StreamingPromptHook<P> for () {}
```

### Phase 4: Builder Patterns

**Adopt from Rig**:

```rust
pub struct CompletionRequestBuilder<P: CanChat> {
    provider: Arc<P>,
    prompt: Message,
    preamble: Option<String>,
    chat_history: Vec<Message>,
    documents: Vec<Document>,
    tools: Vec<ToolDefinition>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    additional_params: Option<serde_json::Value>,
}

impl<P: CanChat> CompletionRequestBuilder<P> {
    pub fn preamble(mut self, preamble: String) -> Self { ... }
    pub fn document(mut self, doc: Document) -> Self { ... }
    pub fn tool(mut self, tool: ToolDefinition) -> Self { ... }
    pub fn temperature(mut self, temp: f32) -> Self { ... }

    pub fn build(self) -> ChatCompletionRequest { ... }

    pub async fn send(self) -> Result<ChatCompletionResponse, CompletionError> {
        self.provider.chat(self.build()).await
    }
}

pub struct AgentBuilder<P: CanChat> {
    provider: Arc<P>,
    name: Option<String>,
    preamble: Option<String>,
    static_context: Vec<Document>,
    dynamic_context: Vec<(usize, Box<dyn VectorStoreIndex>)>,
    tools: ToolSet,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl<P: CanChat> AgentBuilder<P> {
    pub fn new(provider: P) -> Self { ... }
    pub fn name(mut self, name: &str) -> Self { ... }
    pub fn preamble(mut self, preamble: &str) -> Self { ... }
    pub fn context(mut self, doc: &str) -> Self { ... }
    pub fn dynamic_context(mut self, sample: usize, index: impl VectorStoreIndex) -> Self { ... }
    pub fn tool(mut self, tool: impl Tool) -> Self { ... }
    pub fn temperature(mut self, temp: f32) -> Self { ... }
    pub fn build(self) -> Agent<P> { ... }
}
```

### Phase 5: Context Management (Hybrid)

**Keep Crucible's token budget tracking**:
```rust
pub trait ContextManager: Send + Sync {
    fn trim_to_budget(&mut self, max_tokens: usize);
    fn token_estimate(&self) -> usize;
}
```

**Adopt Rig's RAG integration**:
```rust
pub struct Agent<P: CanChat> {
    pub provider: Arc<P>,
    pub preamble: Option<String>,
    pub static_context: Vec<Document>,
    pub dynamic_context: Arc<RwLock<Vec<(usize, Box<dyn VectorStoreIndex>)>>>,
    pub context_manager: Arc<Mutex<Box<dyn ContextManager>>>,
    // ...
}

impl<P: CanChat> Agent<P> {
    async fn build_request(&self, prompt: Message, history: Vec<Message>)
        -> Result<CompletionRequestBuilder<P>, CompletionError>
    {
        // 1. Fetch dynamic context via RAG
        let rag_text = prompt.extract_rag_query();
        let dynamic_docs = if let Some(query) = rag_text {
            self.fetch_rag_documents(query).await?
        } else {
            vec![]
        };

        // 2. Build request with static + dynamic context
        let mut builder = self.provider.completion_request(prompt)
            .preamble_opt(self.preamble.clone())
            .messages(history)
            .documents(self.static_context.clone())
            .documents(dynamic_docs);

        // 3. Trim context to token budget
        let budget = self.context_manager.lock().unwrap().token_estimate();
        // Adjust builder based on budget...

        Ok(builder)
    }
}
```

### Phase 6: Multi-Turn Tool Loop

**Adopt from Rig**:

```rust
pub struct PromptRequest<'a, P: CanChat, H: PromptHook<P>> {
    agent: &'a Agent<P>,
    prompt: Message,
    chat_history: Option<&'a mut Vec<Message>>,
    max_depth: usize,
    concurrency: usize,
    hook: Option<H>,
}

impl<P: CanChat, H: PromptHook<P>> PromptRequest<'_, P, H> {
    pub fn multi_turn(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn with_tool_concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = concurrency;
        self
    }

    pub fn with_hook<H2: PromptHook<P>>(self, hook: H2) -> PromptRequest<'a, P, H2> {
        PromptRequest { hook: Some(hook), .. }
    }

    pub async fn send(self) -> Result<String, PromptError> {
        let mut current_depth = 0;
        let cancel_sig = CancelSignal::new();

        loop {
            if current_depth > self.max_depth {
                return Err(PromptError::MaxDepthError { ... });
            }
            current_depth += 1;

            // Hook: before completion
            if let Some(hook) = &self.hook {
                hook.on_completion_call(&prompt, &history, cancel_sig.clone()).await;
                if cancel_sig.is_cancelled() {
                    return Err(PromptError::Cancelled);
                }
            }

            // Call completion
            let resp = self.agent.completion(prompt, history).await?.send().await?;

            // Hook: after completion
            if let Some(hook) = &self.hook {
                hook.on_completion_response(&prompt, &resp, cancel_sig.clone()).await;
            }

            // Partition tool calls vs text
            let (tool_calls, texts) = partition_response(&resp);

            if tool_calls.is_empty() {
                return Ok(merge_texts(texts));
            }

            // Execute tools concurrently
            let tool_results = stream::iter(tool_calls)
                .map(|call| async {
                    // Hook: before tool call
                    if let Some(hook) = &self.hook {
                        hook.on_tool_call(&call.name, call.id.clone(), &call.args, cancel_sig.clone()).await;
                    }

                    let result = self.agent.execute_tool(&call).await?;

                    // Hook: after tool call
                    if let Some(hook) = &self.hook {
                        hook.on_tool_result(&call.name, call.id.clone(), &call.args, &result, cancel_sig.clone()).await;
                    }

                    Ok(result)
                })
                .buffer_unordered(self.concurrency)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;

            history.push(Message::User { content: tool_results });
        }
    }
}
```

---

## Migration Path

### Step 1: Add New Traits (Non-Breaking)

Add `Prompt`, `Chat`, `Completion` traits alongside existing `AgentHandle`. Mark `AgentHandle` as deprecated.

### Step 2: Implement New Traits for InternalAgentHandle

Make `InternalAgentHandle` implement `Prompt`, `Chat`, `Completion`. Users can migrate incrementally.

### Step 3: Refactor Streaming

Replace `ChatChunk` with `StreamingChoice`. Add `StreamingResponse` with pause/cancel.

### Step 4: Add Hook System

Introduce `PromptHook` and `StreamingPromptHook`. Refactor `PromptRequest` to support hooks.

### Step 5: Add Builders

Create `CompletionRequestBuilder` and `AgentBuilder`. Deprecate direct construction.

### Step 6: Deprecate AgentHandle

Remove `AgentHandle` trait. All code now uses `Prompt`/`Chat`/`Completion`.

---

## Conclusion

**Adopt**:
- Rig's trait hierarchy (Prompt/Chat/Completion)
- Rig's streaming architecture (typed chunks, pause/cancel)
- Rig's hook system (PromptHook/StreamingPromptHook)
- Rig's builder patterns (CompletionRequestBuilder, AgentBuilder)
- Rig's multi-turn tool loop with concurrency

**Keep**:
- Crucible's capability-based providers (CanEmbed/CanChat/CanConstrainGeneration)
- Crucible's token budget tracking (ContextManager)
- Crucible's ACP abstractions (SessionManager, ToolBridge) - complement Rig's patterns

**Hybrid**:
- Context management: Rig's RAG + Crucible's token budgeting
- Error handling: Adopt Rig's hierarchy, add Crucible-specific variants

This hybrid approach gives us:
- ✅ SOLID-compliant trait design (ISP + DIP)
- ✅ Production-grade streaming with control
- ✅ Observable agent execution via hooks
- ✅ Compile-time capability checking
- ✅ Ergonomic builder APIs
- ✅ Flexible abstraction levels (simple prompt → low-level builder)
