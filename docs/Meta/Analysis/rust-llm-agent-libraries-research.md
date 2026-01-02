---
title: "Rust LLM Agent Libraries Research"
date: 2025-12-24
tags: [research, llm, agents, rust]
---

# Rust LLM Agent Libraries Research

Comprehensive research into Rust libraries for LLM and AI agent development, focusing on agent abstractions, memory patterns, tool execution, context injection, and architectural approaches.

## Executive Summary

The Rust LLM ecosystem has matured significantly with several production-ready frameworks offering different trade-offs:

- **Rig**: Best unified interface, strong RAG support, MCP integration
- **Swiftide**: Best for RAG pipelines, streaming data, fast indexing
- **AutoAgents**: Best actor-based multi-agent orchestration
- **Graph-Flow**: Best for workflow orchestration with conditional routing
- **Kalosm**: Best for local-first, controlled generation
- **LangChain-Rust**: Best Python LangChain compatibility

### Key Recommendation for Crucible

**Primary**: Use **Rig** as the foundation for agent abstractions and provider unification
**Secondary**: Study **Graph-Flow** for workflow patterns and **Swiftide** for RAG pipeline design
**Avoid**: Building custom provider abstractions (Rig already does this well)

---

## Comparison Table

| Library | Stars | Maturity | Agent Pattern | Memory/State | Context Injection | Tool System | Unique Features |
|---------|-------|----------|---------------|--------------|-------------------|-------------|-----------------|
| **Rig** | 3.2k+ | Production | Agent struct with builder | Multi-turn chat state | Static + Dynamic (RAG) | Static + Dynamic tools | MCP support, 20+ providers, unified interface |
| **Swiftide** | 500+ | Beta | Macro-based tools | Pipeline state | RAG query pipelines | `#[tool]` macro | Streaming indexing, RAGAS evaluation, tree-sitter |
| **AutoAgents** | 300+ | Alpha | Derive macro agents | Sliding window, pluggable | Task-based | `#[tool]` macro | Ractor actors, WASM runtime, type-safe pub/sub |
| **Graph-Flow** | 200+ | Alpha | Task trait nodes | Context object | Graph context state | Rig integration | Conditional routing, FanOut parallelism |
| **Kalosm** | 2k+ | Beta | Model interfaces | N/A | Direct prompt | N/A | Local-first, constrained generation, pure Rust |
| **LangChain-Rust** | 1.5k+ | Beta | AgentExecutor | Message-based | Message history | Built-in tools | Python LangChain patterns, multiple chains |
| **llm-chain** | 1.3k+ | Maintenance | Chain executor | Executor state | Sequential chains | Bash/Python/Web | Prompt templates, chain composition |
| **AgentAI** | 100+ | Alpha | Agent with toolbox | Planned | N/A | `#[toolbox]` macro | MCP client support, structured responses |
| **mistral.rs** | 4k+ | Production | N/A (inference) | N/A | N/A | N/A | Fastest inference, FlashAttn V3, tensor parallel |

---

## Detailed Library Analysis

### 1. Rig (0xPlaygrounds)

**Repository**: [github.com/0xPlaygrounds/rig](https://github.com/0xPlaygrounds/rig)
**Docs**: [rig.rs](https://rig.rs/) | [docs.rig.rs](https://docs.rig.rs/)
**Crate**: `rig-core`

#### Core Abstractions

**Agent Structure**:
```rust
let agent = client.agent("gpt-4")
    .preamble("System prompt")
    .context("static context")
    .dynamic_context(3, vector_index)  // RAG: top 3 results
    .tool(calculator)
    .dynamic_tools(2, tool_index, toolset)
    .multi_turn(5)
    .build();
```

**Provider Unification**: 20+ providers (OpenAI, Anthropic, Cohere, Gemini, Ollama, Groq, Perplexity, etc.) with unified interface via traits:
- `CompletionModel` - Text generation
- `EmbeddingModel` - Vector embeddings
- `AudioGenerationModel` - Text-to-speech

**Vector Store Abstraction**: 10+ vector stores (MongoDB, Qdrant, LanceDB, Neo4j, SurrealDB, etc.) via `VectorStoreIndex` trait

#### Context Management

**Static Context**: Documents always appended to requests via `AgentBuilder::context()`

**Dynamic Context (RAG)**: Retrieves top N documents from vector store based on query similarity:
```rust
.dynamic_context(3, index)  // Fetch 3 most relevant docs
```

Documents are "automatically appended to provider request before it is sent"

#### Conversation State

**Multi-Turn State**: `.multi_turn(n)` sets max turns to prevent infinite tool loops
**Chat History**: `.chat(previous_messages)` enables conversation continuity
**Prompt Hooks**: Custom observability and behavioral modifications

#### Tool Execution

**Tool Types**:
- Static tools: Always available
- Dynamic tools: Retrieved from vector store based on relevance

**Tool Resolution**: Automatic tool selection and execution with error handling

#### Memory Patterns

**External State Machine**: Separate [rig-agent-state-machine-example](https://github.com/0xPlaygrounds/rig-agent-state-machine-example) provides:
- Flexible state management for agents
- Built-in chat history tracking
- State change notifications
- Rig-compatible with any provider

#### Unique Features

- **MCP (Model Context Protocol)**: Native integration for tools
- **WASM Support**: Browser-based deployments
- **GenAI Semantic Convention**: OpenTelemetry observability
- **Modular Companion Crates**: Separate packages for integrations
- **Multi-modal**: Transcription, audio gen, image gen

#### Strengths for Crucible

- ✅ Already has SurrealDB vector store integration
- ✅ Unified provider interface matches Crucible's trait-based approach
- ✅ Dynamic context RAG pattern aligns with Crucible's needs
- ✅ MCP support matches Crucible's tool architecture
- ✅ Minimal boilerplate, ergonomic API

#### Weaknesses

- ⚠️ State management requires external implementation
- ⚠️ Memory patterns not built-in (but extensible)
- ⚠️ Workflow orchestration not included (use Graph-Flow)

---

### 2. Swiftide (bosun.ai)

**Repository**: [github.com/bosun-ai/swiftide](https://github.com/floneum/floneum)
**Docs**: [swiftide.rs](https://swiftide.rs/)
**Crate**: `swiftide`, `swiftide-agents`

#### Core Abstractions

**RAG Pipeline**: "Fast, streaming indexing, query, and agentic LLM applications"

**Agent Pattern**: Macro-based tool definition
```rust
#[swiftide_macros::tool(description = "Search code")]
async fn search_code(context: &AgentContext, query: String) -> Result<ToolOutput, ToolError> {
    context.exec_cmd("rg", &[&query]).await
}
```

**Pipelines**: Transform → Enrich → Persist with lazy async execution

#### Context Management

**RAG-First Design**:
- Load data from multiple sources (files, RSS, websites)
- Transform with tree-sitter for code understanding
- Chunk and augment with metadata
- Index to vector stores (Redis, Qdrant, LanceDB)

**Query Pipelines**: Agents execute queries through existing indexing infrastructure

#### Tool System

**Declarative Tools**: `#[tool]` macro with structured params and descriptions
**Context Access**: Tools receive `AgentContext` for command execution
**Structured Output**: `Result<ToolOutput, ToolError>` for consistency

#### Lifecycle Hooks

"Hook in on important parts of the agent lifecycle" via `AgentContext` trait:
- Pre/post tool execution
- Query pipeline stages
- Error handling

#### Unique Features

- **Streaming Indexing**: Parallel, lazy pipelines for fast data ingestion
- **Tree-sitter Integration**: Code-aware chunking and augmentation
- **RAGAS Evaluation**: Built-in quality metrics for RAG systems
- **Fluvio Integration**: Real-time data streaming
- **LanceDB Support**: Serverless vector storage

#### Strengths for Crucible

- ✅ Tree-sitter integration matches Crucible's code-aware parsing
- ✅ Streaming pipeline design fits Crucible's async architecture
- ✅ RAG evaluation framework useful for quality metrics
- ✅ Code chunking patterns applicable to wikilink extraction

#### Weaknesses

- ⚠️ Agent system less mature than Rig
- ⚠️ Focused on RAG, not general agents
- ⚠️ Breaking changes expected (heavy development)

---

### 3. AutoAgents (liquidos-ai)

**Repository**: [github.com/liquidos-ai/AutoAgents](https://github.com/liquidos-ai/AutoAgents)
**Crate**: `autoagents`

#### Core Abstractions

**Agent Definition**: Derive macro with declarative syntax
```rust
#[agent(name = "math_agent", tools = [Addition], output = MathAgentOutput)]
pub struct MathAgent {}
```

**Executor Pattern**: Swappable executors (ReAct, Basic) with streaming support

**Actor Model**: Built on Ractor for concurrent agent orchestration

#### Memory Systems

**Sliding Window Memory**: Configurable context window
```rust
let memory = Box::new(SlidingWindowMemory::new(10));
```

**Pluggable Backend**: Interface allows custom memory implementations

#### Tool Execution

**Tool Runtime**: Sandboxed execution via `ToolRuntime` trait
```rust
#[tool(name = "Addition", description = "...", input = AdditionArgs)]
```

**WASM Runtime**: Browser-deployable tools with isolation

#### Multi-Agent Coordination

**Type-Safe Pub/Sub**: Ractor-based message passing
**Agent Orchestration**: Distributed agent networks
**State Isolation**: Actors provide independent state contexts

#### Conversation State

**Task Abstraction**: State flows through Task → Memory → Agent
**Reasoning Traces**: `ReActAgentOutput` captures thought process
**Stateful Threading**: Memory maintains conversation context

#### Unique Features

- **Ractor Actors**: Production-grade actor system
- **Structured Outputs**: Type-safe JSON schema validation
- **Modular Architecture**: Swappable memory/executors
- **WASM Tools**: Browser-based agent deployment

#### Strengths for Crucible

- ✅ Actor-based concurrency matches Crucible's async design
- ✅ Structured outputs align with typed responses
- ✅ Modular architecture enables custom components

#### Weaknesses

- ⚠️ Alpha maturity, API changes likely
- ⚠️ Smaller ecosystem than Rig
- ⚠️ Ractor adds complexity vs simpler patterns

---

### 4. Graph-Flow (rs-graph-llm)

**Repository**: [github.com/a-agmon/rs-graph-llm](https://github.com/a-agmon/rs-graph-llm)

#### Core Abstractions

**Graph Execution Engine**: Tasks as nodes, edges as flow control

**Task Trait**: Async nodes with shared context
```rust
impl Task for MyTask {
    async fn run(&self, context: Arc<Mutex<Context>>) -> Result<NextAction> {
        // ... task logic
    }
}
```

**NextAction Enum**: Control flow decisions (Continue, StepBack, End)

#### State Management

**Context Object**: Central state repository
- Typed data: `context.set()` / `context.get()`
- Thread-safe: `Arc<Mutex<Context>>`
- Chat history: Automatic serialization
- Session persistence: In-memory or PostgreSQL

#### Rig Integration

**Seamless LLM Tasks**: Rig agents as graph nodes
**Message Management**: Built-in methods for chat history
**Provider Abstraction**: Leverage Rig's unified interface

#### Workflow Patterns

**Conditional Routing**: Runtime branching based on context values
**FanOut Tasks**: Parallel execution with result aggregation
**Step-Wise vs Continuous**: Flexible execution control

#### Unique Features

- **LangGraph-Inspired**: Python LangGraph patterns in Rust
- **Type-Safe Graphs**: Compile-time workflow validation
- **Session Management**: Pluggable storage backends
- **Conditional Edges**: Dynamic routing logic

#### Strengths for Crucible

- ✅ Workflow orchestration aligns with Crucible's agent system
- ✅ Rig integration provides best of both worlds
- ✅ Session persistence matches Crucible's storage needs
- ✅ Conditional routing useful for complex agent workflows

#### Weaknesses

- ⚠️ Early stage (200 stars)
- ⚠️ Smaller community/examples
- ⚠️ Graph complexity may be overkill for simple agents

---

### 5. Kalosm (floneum)

**Repository**: [github.com/floneum/floneum](https://github.com/floneum/floneum)
**Docs**: [docs.rs/kalosm](https://docs.rs/kalosm)
**Crate**: `kalosm`, `kalosm-language-model`

#### Core Abstractions

**Model Interfaces**: Direct interaction with local models
```rust
let model = Llama::new().await?;
model.chat().with_system_prompt("You are a pirate").prompt("Hello").await?;
```

**Modality Modules**:
- `kalosm::language` - Text generation, embeddings, RAG
- `kalosm::audio` - Whisper transcription, voice detection
- `kalosm::vision` - Image generation, segmentation

#### Controlled Generation

**Structured Output**: Compile-time constraints via derive macros
```rust
#[derive(Parse, Schema)]
struct Character {
    #[regex("[A-Z][a-z]+")]
    name: String,

    #[range(18..100)]
    age: u8,
}
```

**Parser-Guided Sampling**: "Structure-aware acceleration makes generation faster than uncontrolled text"

**Constraint Enforcement**: Invalid outputs prevented at generation time, not post-processing

#### Local-First Design

**Pure Rust**: Candle ML framework, no Python dependencies
**Quantized Models**: Performance on par with llama.cpp
**Hardware Support**: CPU, CUDA, Metal via feature flags

#### Unique Features

- **Fastest Structured Generation**: Parser-guided sampling acceleration
- **Tree-sitter Integration**: Code understanding and augmentation
- **Multi-modal Unified**: Text, audio, vision in one interface
- **WGPU Backend (Fusor)**: Future web/AMD support

#### Strengths for Crucible

- ✅ Local-first matches Crucible's Ollama focus
- ✅ Structured generation useful for typed tool outputs
- ✅ Tree-sitter integration aligns with code parsing
- ✅ Pure Rust eliminates Python dependencies

#### Weaknesses

- ⚠️ No agent abstractions (just model interfaces)
- ⚠️ No memory/conversation patterns
- ⚠️ No tool system (manual implementation needed)

---

### 6. LangChain-Rust (Abraxas-365)

**Repository**: [github.com/Abraxas-365/langchain-rust](https://github.com/Abraxas-365/langchain-rust)
**Crate**: `langchain-rust`

#### Core Abstractions

**Agent Executor**: Tool-based decision-making
```rust
let agent = OpenAiToolAgentBuilder::new()
    .tools(vec![search_tool, calculator])
    .build();
```

**Chain Patterns**: Composable prompt processing
- LLMChain: Basic prompt → LLM → output
- Conversational: Maintain dialogue context
- Sequential: Multi-step chains
- Retrieval: Vector store integration
- Domain-specific: Q&A, SQL

#### Memory Systems

**Message-Based Memory**: Explicit conversation tracking
```rust
Message::new_system_message("context")
Message::new_human_message("query")
Message::new_ai_message("response")
```

**History Placeholders**: `fmt_placeholder!("history")` injects prior exchanges

**SimpleMemory**: Basic conversation buffer implementation

#### Tool Architecture

**Built-in Tools**:
- Search: Serpapi, DuckDuckGo
- Computational: Wolfram Alpha
- Execution: Command-line
- Multimodal: Text-to-speech

**Consistent Interfaces**: Pluggable tool expansion

#### Provider Support

**Multiple LLMs**: OpenAI, Azure OpenAI, Anthropic, Ollama
**Vector Stores**: Qdrant, PostgreSQL, SurrealDB, SQLite, OpenSearch

#### Unique Features

- **Python LangChain Patterns**: Familiar API for Python users
- **SurrealDB Integration**: Matches Crucible's storage
- **Comprehensive Chains**: Pre-built patterns for common tasks
- **Active Maintenance**: Regular releases (v4.6.0 latest)

#### Strengths for Crucible

- ✅ SurrealDB vector store integration
- ✅ Chain patterns useful for multi-step workflows
- ✅ Message-based memory easy to understand
- ✅ Good Python LangChain migration path

#### Weaknesses

- ⚠️ Heavy serde_json dependency
- ⚠️ Less ergonomic than Rig's builder pattern
- ⚠️ Memory doesn't include tool call history (open issue)

---

### 7. llm-chain (sobelio)

**Repository**: [github.com/sobelio/llm-chain](https://github.com/sobelio/llm-chain)
**Crate**: `llm-chain`

#### Core Abstractions

**Chain Executor**: Multi-step prompt orchestration
```rust
let exec = executor!()?;
let res = prompt!(
    "system instruction",
    "user task"
).run(parameters()!, &exec).await?;
```

**Macro-Based API**: Streamlined composition via procedural macros

#### State Management

**Executor Context**: Runtime state threading through chains
**Parameter Passing**: State flows via `.run()` calls
**Template Reuse**: Stateless prompts, stateful execution

#### Tool Integration

**External Capabilities**: Bash, Python, web search
**Chain Steps**: Tools expand per-step capabilities beyond LLM

#### Memory Patterns

**Vector Store Integration**: "Long-term memory and subject matter knowledge"
**Persistent Context**: External storage addresses context limits

#### Unique Features

- **Prompt Templates**: Reusable, parameterized prompts
- **Cloud + Local**: Supports both hosted and local LLMs
- **Chain Composition**: Multi-step task decomposition

#### Strengths for Crucible

- ✅ Chain patterns useful for workflows
- ✅ Vector store focus aligns with RAG needs
- ✅ Template system applicable to prompts

#### Weaknesses

- ⚠️ Less active development
- ⚠️ Older API design vs newer libraries
- ⚠️ Smaller ecosystem than Rig

---

### 8. AgentAI

**Repository**: Not found (docs.rs only)
**Docs**: [docs.rs/agentai](https://docs.rs/agentai)
**Crate**: `agentai`

#### Core Abstractions

**Agent System**: System prompt + model selection
```rust
let agent = Agent::new("system prompt");
agent.invoke("gpt-4", "query").await?;
```

**ToolBox Trait**: Custom tool creation interface

#### MCP Integration

**Experimental MCP Support**: Agent tools via Model Context Protocol
**Eliminates Custom Tools**: Leverage existing MCP servers

#### Unique Features

- **Multi-Model**: OpenAI, Anthropic, Gemini, Ollama, OpenAI-compatible
- **Structured Responses**: Type-safe output format
- **Toolbox Macro**: `#[toolbox]` simplifies tool definition
- **GenAI Backend**: Leverages `genai` library for providers

#### Development Status

**Heavy Development**: Planned additions:
- Agent memory management
- Streaming output
- Configurable behavioral parameters

#### Strengths for Crucible

- ✅ MCP support matches Crucible's tool architecture
- ✅ Multi-model flexibility useful
- ✅ Lightweight, simple API

#### Weaknesses

- ⚠️ Very early stage
- ⚠️ Missing memory/state features
- ⚠️ Limited documentation/examples

---

### 9. mistral.rs (EricLBuehler)

**Repository**: [github.com/EricLBuehler/mistral.rs](https://github.com/EricLBuehler/mistral.rs)
**Focus**: LLM inference engine (not agent framework)

#### Core Capabilities

**Blazing Fast Inference**: FlashAttention V3, PagedAttention, tensor parallelism
**Multi-Modal**: Text, vision, audio, image generation, embeddings
**Hardware Support**: CUDA, Metal, CPU (MKL, Accelerate, ARM/AVX)

#### Performance Optimizations

**In-Place Quantization (ISQ)**: Reduced memory footprint
**Automatic Device Mapping**: Multi-GPU/CPU distribution
**Competitive Speed**: On par with llama.cpp and MLX

#### API Options

- Rust multithreaded/async
- Python bindings
- OpenAI-compatible HTTP server

#### Unique Features

- **Tensor Parallelism**: NCCL support for distributed inference
- **FlashAttention V3**: Latest attention optimization
- **30x Faster ISQ**: On Metal (v0.5.0)
- **Native Tool Calling**: Llama 3.x, Mistral models

#### Strengths for Crucible

- ✅ Fastest local inference for Crucible's Ollama alternative
- ✅ Multi-modal support future-proofs capabilities
- ✅ OpenAI-compatible API easy integration
- ✅ Pure Rust, production-ready

#### Weaknesses

- ⚠️ Not an agent framework (inference only)
- ⚠️ No memory/context management
- ⚠️ No tool orchestration (just execution)

---

## Key Patterns Identified

### 1. Context Injection Patterns

#### Static Context (Rig, Swiftide)
Always-available documents appended to every request. Simple, predictable, but wastes tokens.

```rust
agent.context("Always include this information")
```

#### Dynamic Context / RAG (Rig, Swiftide, LangChain-Rust)
Query-time retrieval from vector stores. Efficient token usage, relevant context.

```rust
agent.dynamic_context(3, vector_index)  // Top 3 relevant docs
```

#### Graph Context (Graph-Flow)
Shared state object across workflow nodes. Flexible, type-safe, session-persistent.

```rust
context.set("key", value);
let value: Type = context.get("key")?;
```

#### Message History (LangChain-Rust, Rig)
Explicit conversation tracking via message sequences. Standard pattern, compatible with APIs.

```rust
agent.chat(previous_messages).prompt("next query")
```

### 2. Memory Patterns

#### Sliding Window (AutoAgents)
Configurable context window size. Simple, predictable memory usage.

```rust
SlidingWindowMemory::new(10)  // Last 10 messages
```

#### Vector Store Memory (Rig, llm-chain, Swiftide)
Long-term semantic memory via embeddings. Scales beyond context limits.

```rust
vector_store.index(embedding_model)
```

#### External State Machine (Rig)
Separate state management crate. Flexible, supports custom state logic.

#### Session Persistence (Graph-Flow)
Pluggable backends (in-memory, PostgreSQL). Production-ready, scalable.

### 3. Tool Execution Patterns

#### Static Tools (Rig)
Always available, presented in every interaction.

```rust
agent.tool(calculator)
```

#### Dynamic Tools (Rig)
Retrieved from vector store based on query relevance.

```rust
agent.dynamic_tools(2, tool_index, toolset)
```

#### Macro-Based Tools (Swiftide, AutoAgents, AgentAI)
Declarative tool definition via derive/attribute macros.

```rust
#[tool(name = "Search", description = "...")]
async fn search(query: String) -> Result<ToolOutput> { ... }
```

#### MCP Tools (Rig, AgentAI)
Model Context Protocol integration. Leverage existing tool servers.

#### WASM Tools (AutoAgents)
Browser-deployable, sandboxed execution.

### 4. Agent Orchestration Patterns

#### Single Agent (Rig, Kalosm, AgentAI)
One agent handles entire interaction. Simple, direct.

#### Multi-Agent Actor (AutoAgents)
Ractor-based concurrent agents with message passing. Distributed, scalable.

#### Multi-Agent Graph (Graph-Flow)
Task nodes with conditional routing. Complex workflows, type-safe.

#### Agent Chains (LangChain-Rust, llm-chain)
Sequential agent/chain composition. Multi-step decomposition.

---

## Recommendations for Crucible

### Primary Recommendation: Adopt Rig Core

**Rationale**:
1. **Unified Provider Interface**: Rig already abstracts 20+ LLM providers with traits (`CompletionModel`, `EmbeddingModel`) matching Crucible's existing trait-based design
2. **SurrealDB Integration**: Native vector store support via companion crate
3. **MCP Support**: Aligns with Crucible's Model Context Protocol tools
4. **Dynamic RAG**: `dynamic_context()` pattern perfect for Crucible's knowledge graph
5. **Production-Ready**: 3.2k+ stars, active development, WASM support

**Integration Path**:
```rust
// Crucible agent wraps Rig agent
use rig::{Agent, providers::openai};
use crucible_surrealdb::RigVectorStore;

let provider = crucible_llm::create_provider(config).await?;
let vector_store = RigVectorStore::new(crucible_db);
let index = vector_store.index(embedding_model);

let agent = provider
    .agent("gpt-4")
    .preamble(agent_card.system_prompt)
    .dynamic_context(5, index)  // Top 5 wikilinks/blocks
    .tool(crucible_search_tool)
    .tool(crucible_graph_tool)
    .multi_turn(10)
    .build();
```

**Action Items**:
- [ ] Add `rig-core` as dependency
- [ ] Create `RigVectorStore` adapter for Crucible's SurrealDB
- [ ] Implement Crucible tools as Rig tools
- [ ] Wrap Rig agents in Crucible's agent system
- [ ] Leverage Rig's MCP integration for tool discovery

### Secondary: Study Graph-Flow for Workflows

**Rationale**:
1. **Session Management**: Pluggable persistence backends match Crucible's needs
2. **Conditional Routing**: Useful for complex agent workflows (triage, specialist routing)
3. **Rig Integration**: Uses Rig under the hood, composable approach
4. **Type-Safe Graphs**: Compile-time workflow validation

**Use Cases in Crucible**:
- Workflow definitions (multi-step agent processes)
- Agent orchestration (triage → specialist → synthesis)
- Session persistence (resume long-running workflows)

**Action Items**:
- [ ] Study Graph-Flow's `Context` pattern for session state
- [ ] Evaluate conditional routing for workflow system
- [ ] Consider FanOut pattern for parallel agent queries

### Tertiary: Learn from Swiftide's RAG Pipeline

**Rationale**:
1. **Streaming Indexing**: Fast, parallel data ingestion
2. **Tree-sitter Integration**: Code-aware chunking applicable to wikilinks
3. **RAGAS Evaluation**: Quality metrics for RAG performance

**Patterns to Adopt**:
- Lazy async pipelines for incremental indexing
- Chunking strategies for block-level embeddings
- Evaluation framework for semantic search quality

**Action Items**:
- [ ] Study Swiftide's chunking algorithm
- [ ] Evaluate RAGAS integration for quality metrics
- [ ] Review streaming pipeline design for incremental updates

### Avoid: Custom Provider Abstractions

**Don't Build**:
- Custom LLM provider traits (Rig already has this)
- Vector store interfaces (Rig's `VectorStoreIndex` is sufficient)
- Tool execution runtime (Rig + MCP covers this)

**Instead**:
- Contribute SurrealDB improvements to Rig ecosystem
- Build Crucible-specific tools on Rig's foundation
- Focus on knowledge graph uniqueness, not LLM plumbing

---

## Architecture Synthesis for Crucible

### Recommended Stack

```
┌─────────────────────────────────────────┐
│   Crucible Agent Layer                  │
│   - Agent cards (persona, tools)        │
│   - Workflow orchestration              │
│   - Session management                  │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│   Rig Agent Core                        │
│   - Agent builder pattern               │
│   - Static + Dynamic context            │
│   - Tool execution                      │
│   - Multi-turn state                    │
└─────────────────────────────────────────┘
                  ↓
┌─────────────────┬───────────────────────┐
│  Rig Providers  │  Rig Vector Stores    │
│  (20+ LLMs)     │  (SurrealDB adapter)  │
└─────────────────┴───────────────────────┘
                  ↓
┌─────────────────────────────────────────┐
│   Crucible Core Infrastructure          │
│   - Parser (wikilinks, blocks)          │
│   - SurrealDB (EAV graph, Merkle)       │
│   - MCP Tools                           │
└─────────────────────────────────────────┘
```

### Agent Context Injection Strategy

**For Crucible Agents**:

1. **Static Context**: Agent card preamble, vault metadata
2. **Dynamic Context (RAG)**: Top N relevant blocks/notes via semantic search
3. **Tool Context**: Graph traversal, search results, file contents
4. **Conversation Context**: Message history via Rig's chat state

**Example**:
```rust
// Retrieval: User asks about [[Rust]]
let query_embedding = embed("What is Rust?").await?;
let relevant_blocks = search_blocks(query_embedding, limit: 5).await?;

// Injection: Rig automatically appends
agent
    .preamble("You are a knowledge assistant for a personal vault")
    .context(vault_metadata)  // Static: vault structure
    .dynamic_context(5, block_index)  // Dynamic: top 5 blocks
    .tool(graph_search_tool)  // Tools: graph traversal
    .chat(previous_messages)  // History: conversation
    .prompt("What is Rust?")
    .await?;
```

### Memory Architecture

**Short-Term (Conversation)**:
- Rig's `.chat()` method with message history
- Sliding window if needed (trim old messages)

**Long-Term (Knowledge)**:
- Vector store of all blocks/notes
- Dynamic retrieval via Rig's RAG

**Session State (Workflows)**:
- Graph-Flow's Context pattern
- PostgreSQL persistence via SurrealDB

**External State (Agent Cards)**:
- Agent configuration as data
- Loaded per-agent from vault

### Tool System Design

**Crucible-Specific Tools** (implement as Rig tools):
- `search_vault` - Keyword/semantic search
- `traverse_graph` - Follow wikilinks (depth-limited BFS)
- `get_note` - Fetch full note content
- `list_tags` - Enumerate tags
- `recent_notes` - Temporal queries
- `block_context` - Get surrounding context for block

**MCP Tools** (via Rig's MCP integration):
- External tools from MCP servers
- Dynamic tool discovery
- Standard protocol, no custom impl

**Tool Retrieval**:
- Dynamic tools stored in vector store
- Descriptions embedded for relevance matching
- Rig auto-selects based on query

---

## Code Examples

### Example 1: Basic Crucible Agent with Rig

```rust
use rig::{Agent, providers::openai};
use crucible_core::agents::AgentCard;
use crucible_surrealdb::RigVectorStore;

pub struct CrucibleAgent {
    card: AgentCard,
    rig_agent: Agent,
}

impl CrucibleAgent {
    pub async fn new(
        card: AgentCard,
        llm_provider: impl Provider,
        vault_index: RigVectorStore,
    ) -> Result<Self> {
        let rig_agent = llm_provider
            .agent(&card.model)
            .preamble(&card.system_prompt)
            .dynamic_context(card.context_limit.unwrap_or(5), vault_index)
            .tools(card.tools.iter().map(|t| crucible_tool_to_rig(t)))
            .multi_turn(card.max_turns.unwrap_or(10))
            .build();

        Ok(Self { card, rig_agent })
    }

    pub async fn chat(&self, message: &str, history: Vec<Message>) -> Result<String> {
        self.rig_agent
            .chat(history)
            .prompt(message)
            .await
    }
}
```

### Example 2: Dynamic Context from Crucible Graph

```rust
use rig::vector_store::{VectorStoreIndex, VectorSearchRequest};
use crucible_core::parser::{BlockHash, ParsedNote};

pub struct CrucibleBlockIndex {
    storage: Arc<SurrealStorage>,
    embedding_provider: Arc<dyn CanEmbed>,
}

#[async_trait]
impl VectorStoreIndex for CrucibleBlockIndex {
    async fn search(&self, query: VectorSearchRequest) -> Result<Vec<Document>> {
        // Embed query
        let query_embedding = self.embedding_provider.embed(&query.query).await?;

        // Semantic search in SurrealDB
        let blocks = self.storage
            .search_blocks_by_embedding(query_embedding, query.limit)
            .await?;

        // Convert to Rig documents
        let documents = blocks.into_iter()
            .map(|block| Document {
                content: block.content,
                metadata: serde_json::json!({
                    "block_hash": block.hash,
                    "note_path": block.note_path,
                    "tags": block.tags,
                }),
            })
            .collect();

        Ok(documents)
    }
}
```

### Example 3: Crucible Tools as Rig Tools

```rust
use rig::tool::{Tool, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct SearchVaultArgs {
    query: String,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct SearchVaultOutput {
    results: Vec<SearchResult>,
}

pub struct SearchVaultTool {
    storage: Arc<SurrealStorage>,
}

#[async_trait]
impl Tool for SearchVaultTool {
    fn name(&self) -> String {
        "search_vault".to_string()
    }

    fn description(&self) -> String {
        "Search the knowledge vault for notes and blocks matching a query".to_string()
    }

    async fn execute(&self, args_json: &str) -> ToolResult {
        let args: SearchVaultArgs = serde_json::from_str(args_json)?;

        let results = self.storage
            .search_content(&args.query, args.limit.unwrap_or(10))
            .await?;

        let output = SearchVaultOutput { results };
        Ok(serde_json::to_string(&output)?)
    }
}
```

### Example 4: Workflow with Graph-Flow + Rig

```rust
use graph_flow::{Graph, Task, Context, NextAction};
use rig::Agent;

struct TriageTask {
    agent: Agent,
}

#[async_trait]
impl Task for TriageTask {
    async fn run(&self, context: Arc<Mutex<Context>>) -> Result<NextAction> {
        let mut ctx = context.lock().await;
        let user_query = ctx.get::<String>("user_query")?;

        let response = self.agent
            .prompt(&format!("Classify this query: {}", user_query))
            .await?;

        ctx.set("category", response.category);

        // Route based on classification
        match response.category.as_str() {
            "code" => Ok(NextAction::Continue("code_specialist")),
            "general" => Ok(NextAction::Continue("general_assistant")),
            _ => Ok(NextAction::End),
        }
    }
}

struct WorkflowBuilder {
    graph: Graph,
}

impl WorkflowBuilder {
    pub fn build_knowledge_workflow(agents: HashMap<String, Agent>) -> Graph {
        let mut graph = Graph::new();

        graph.add_task("triage", TriageTask { agent: agents["triage"] });
        graph.add_task("code_specialist", CodeTask { agent: agents["code"] });
        graph.add_task("general_assistant", GeneralTask { agent: agents["general"] });

        graph.add_edge("triage", "code_specialist");
        graph.add_edge("triage", "general_assistant");

        graph
    }
}
```

---

## Competitive Analysis

### Why Use Libraries vs Custom Build?

**Advantages of Using Rig**:
- ✅ 20+ providers supported (avoid reimplementing OAuth, APIs, etc.)
- ✅ Vector store abstraction (10+ backends, tested at scale)
- ✅ Active community (3.2k stars, frequent updates)
- ✅ MCP integration (standardized tool protocol)
- ✅ Production-ready (observability, error handling, WASM)

**When Custom Makes Sense**:
- ⚠️ Crucible-specific context injection (wikilink graph, block-level)
- ⚠️ Agent card system (vault-based configuration)
- ⚠️ Workflow orchestration (multi-agent, long-running)
- ⚠️ SurrealDB optimizations (EAV schema, Merkle trees)

**Hybrid Approach (Recommended)**:
- Use Rig for: Provider abstraction, RAG, tools, chat state
- Build custom: Context injection logic, agent cards, workflows, SurrealDB adapter

### Rig vs Building from Scratch

| Aspect | Build from Scratch | Use Rig | Winner |
|--------|-------------------|---------|--------|
| Provider support | 1-2 (OpenAI, Ollama) | 20+ | **Rig** |
| Time to production | 6-12 months | 1-2 months | **Rig** |
| Maintenance burden | High (API changes) | Low (library updates) | **Rig** |
| Customization | Full control | Trait-based extension | Tie |
| Vector stores | SurrealDB only | 10+ backends | **Rig** |
| Community support | None | Active (3.2k stars) | **Rig** |
| Crucible-specific | Perfect fit | Requires adapter | **Custom** |
| MCP integration | Manual impl | Built-in | **Rig** |

**Verdict**: Use Rig as foundation, build Crucible-specific features on top.

---

## Migration Path

### Phase 1: Rig Integration (Week 1-2)

1. Add `rig-core` dependency
2. Create `RigVectorStore` adapter for SurrealDB
3. Implement 1-2 core tools (search, graph traversal)
4. Build basic agent wrapper

### Phase 2: Agent Cards (Week 3-4)

1. Map agent card YAML to Rig agent builder
2. Load tools dynamically from card configuration
3. Implement context limit, max turns from card
4. Test with existing agent card examples

### Phase 3: RAG Integration (Week 5-6)

1. Integrate block-level embeddings with Rig index
2. Implement dynamic context retrieval
3. Tune relevance scoring and limits
4. Benchmark search quality (consider RAGAS)

### Phase 4: Workflows (Week 7-8)

1. Evaluate Graph-Flow vs custom workflow system
2. Implement session persistence
3. Build multi-agent orchestration
4. Add conditional routing for complex workflows

### Phase 5: Production Hardening (Week 9-12)

1. Add observability (Rig's GenAI semantic conventions)
2. Error handling and fallbacks
3. Rate limiting and cost tracking
4. Performance optimization

---

## Open Questions

1. **Embedding Model**: Use Rig's embedding providers or Crucible's existing `CanEmbed`?
   - **Recommendation**: Map Crucible's `CanEmbed` to Rig's `EmbeddingModel` via adapter

2. **Tool Discovery**: Static config or dynamic MCP discovery?
   - **Recommendation**: Hybrid - static tools in agent cards, optional MCP for extensions

3. **State Management**: Rig chat state vs Graph-Flow context vs custom?
   - **Recommendation**: Start with Rig's chat state, add Graph-Flow if workflows need it

4. **Vector Store**: Use Rig's SurrealDB integration or build custom?
   - **Recommendation**: Build custom adapter to leverage Crucible's EAV schema + Merkle

5. **Multi-Agent**: AutoAgents' Ractor vs Graph-Flow's graphs vs simple orchestrator?
   - **Recommendation**: Start simple (sequential), add Graph-Flow if conditional routing needed

---

## References

### Documentation Links

- [Rig Documentation](https://docs.rig.rs/)
- [Swiftide Documentation](https://swiftide.rs/)
- [AutoAgents GitHub](https://github.com/liquidos-ai/AutoAgents)
- [Graph-Flow GitHub](https://github.com/a-agmon/rs-graph-llm)
- [Kalosm Documentation](https://docs.rs/kalosm)
- [LangChain-Rust GitHub](https://github.com/Abraxas-365/langchain-rust)
- [llm-chain GitHub](https://github.com/sobelio/llm-chain)
- [AgentAI Documentation](https://docs.rs/agentai)
- [mistral.rs GitHub](https://github.com/EricLBuehler/mistral.rs)

### Related Articles

- [Build a RAG System with Rig in Under 100 Lines](https://docs.rig.rs/guides/rag/rag_system)
- [RAG can be Rigged (SurrealDB + Rig)](https://surrealdb.com/blog/rag-can-be-rigged)
- [AI Agents: Building AI Primitives with Rust](https://www.shuttle.dev/blog/2024/04/30/building-ai-agents-rust)
- [Pragmatic Rust Guidelines: Agents & LLMs](https://microsoft.github.io/rust-guidelines/agents/index.html)
- [Rust Ecosystem for AI & LLMs](https://hackmd.io/@Hamze/Hy5LiRV1gg)
- [Awesome Rust LLM](https://github.com/jondot/awesome-rust-llm)

### Community Resources

- [Rig Discord](https://discord.gg/rig) (via rig.rs)
- [Rust LLM Topic on GitHub](https://github.com/topics/rust-llm)
- [r/rust discussions on LLMs](https://www.reddit.com/r/rust/)

---

## Conclusion

The Rust LLM ecosystem has matured to the point where **building custom provider abstractions is unnecessary**. **Rig** provides a production-ready foundation with exactly the features Crucible needs:

1. **Unified provider interface** (20+ LLMs)
2. **Dynamic RAG** (vector store integration)
3. **Tool system** (static + dynamic + MCP)
4. **Chat state** (multi-turn conversations)
5. **SurrealDB support** (via companion crate)

Crucible should focus on its **unique value proposition**:
- Wikilink-based knowledge graphs
- Block-level semantic search
- Agent cards as data
- Workflow orchestration
- Markdown-first UX

By adopting Rig as the LLM foundation, Crucible can ship agent features **months faster** while maintaining full control over the knowledge graph logic that differentiates it from generic RAG systems.

**Next Steps**:
1. Prototype `RigVectorStore` adapter for SurrealDB
2. Implement 2-3 core tools (search, graph, content)
3. Build agent card → Rig agent mapper
4. Test with real vault and benchmark performance
5. Evaluate Graph-Flow for workflow orchestration

---

*Research conducted: 2025-12-24*
*Libraries evaluated: 9 major frameworks*
*Recommendation: Adopt Rig as foundation, build Crucible-specific features on top*
