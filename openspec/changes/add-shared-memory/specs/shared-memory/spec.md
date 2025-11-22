# Agent Memory Architecture: Crucible's Role in Post-Plateau AI Systems

**Date**: 2025-11-21
**Purpose**: Analyze Crucible as memory infrastructure for reasoning-focused AI agents

---

## Executive Summary

Crucible is not a personal knowledge management tool with AI features. It is **persistent memory infrastructure for reasoning-focused AI agents** operating in a post-plateau AI landscape.

**Core Thesis**: As LLMs plateau in scale-driven improvement, future AI systems will use:
- **Small, efficient reasoning models** (focused on core reasoning, not knowledge)
- **External memory systems** (structured knowledge graphs like Crucible)
- **Tool-use capabilities** (self-extension via plugins/skills)

Crucible provides the memory and self-improvement infrastructure for this architecture.

---

## The Problem: LLMs Lack Persistent Memory

### Human Memory System
```
Working Memory (7±2 items)
    ↓
Short-term Memory (minutes to hours)
    ↓
Long-term Memory (lifetime)
    ↑
Consolidation (sleep, rehearsal)
```

### LLM "Memory" System (Current)
```
Context Window (2k-200k tokens)
    ↓
[NOTHING]
    ↓
[NO PERSISTENT STORAGE]
```

**The Gap**: LLMs have working memory (context) but no long-term memory. Each conversation starts fresh or requires expensive context loading.

---

## The Crucible Solution: Markdown as Memory Medium

### Architecture Analogy

| Human Brain | Computer | Crucible + LLM |
|-------------|----------|----------------|
| Neocortex (reasoning) | CPU | Small reasoning LLM |
| Hippocampus (memory formation) | RAM | Context window |
| Long-term memory (storage) | Hard drive | Crucible markdown graphs |
| Motor cortex (tool use) | Peripherals | Plugins/MCP/Rune |

### Why Markdown?

1. **Version Control**: Git tracks memory evolution
   - See when memories formed
   - Understand how knowledge changed
   - Rollback corrupted memories

2. **Human-Readable**: Humans can audit agent memories
   - Catch hallucinations
   - Correct misconceptions
   - Understand agent reasoning

3. **Structured**: Frontmatter + wikilinks + headings
   - Typed metadata (tags, dates, entities)
   - Explicit relationships ([[wikilinks]])
   - Hierarchical organization (headings)

4. **Contributable**: Both humans and agents write
   - Agents can write their own memories
   - Humans can add/correct information
   - Collaborative knowledge building

### Why Not Databases?

| Aspect | Traditional DB | Markdown + Crucible |
|--------|---------------|---------------------|
| **Human-readable** | ❌ Binary blobs | ✅ Plain text |
| **Version control** | ⚠️ Possible but complex | ✅ Native git support |
| **Editor support** | ❌ Specialized tools | ✅ Any text editor |
| **Searchable** | ✅ SQL queries | ✅ Semantic + graph |
| **Backups** | ⚠️ Dump/restore | ✅ Git push |
| **Merging** | ❌ Conflict hell | ✅ Standard git merge |

---

## Core Concepts: Memory Architecture

### 1. Wikilinks = Memory Associations

```markdown
# Machine Learning

Related to [[Neural Networks]] and [[Statistics]].
Used in [[Project Alpha]] and [[Research Paper Draft]].
See also [[Deep Learning]] and [[Bayesian Methods]].
```

**Key Insight**: Wikilinks are HOW memories connect, not just references.

- **Bidirectional**: `[[A]]` in note B creates B→A and A←B links
- **Emergent**: Graph structure forms naturally from writing
- **Retrievable**: Follow links to recall related memories

This mirrors human **associative memory**: thinking about "machine learning" activates related concepts.

### 2. Block-Level Embeddings = Precise Recall

Traditional RAG:
```
Query: "How do I train a neural network?"
Retrieves: Entire document "Machine Learning Basics.md"
Problem: 50 paragraphs, only 2 relevant
```

Crucible:
```
Query: "How do I train a neural network?"
Retrieves: Specific paragraphs:
  - "Machine Learning Basics.md#Training Process" (paragraph 3)
  - "Neural Networks.md#Backpropagation" (paragraph 7)
Result: Precise context, minimal token waste
```

**Key Insight**: Human memory recalls specific moments/facts, not entire episodes.

### 3. Entity Indexing = Structured Recall

Crucible extracts entities:
- `#tags` → Topics/categories
- `@mentions` → People/agents
- `file paths` → Code/documents
- `CamelCase` → Technical terms

These enable **faceted search**:
- "All memories tagged #machine-learning mentioning @sarah"
- "All memories referencing src/model.py"
- "All memories about NeuralNetworks created last month"

**Key Insight**: Memory has structure beyond just semantic similarity.

### 4. A2A = Collaborative Cognition

Crucible's A2A is NOT about autonomous agents coordinating.

It's about **specialized reasoning modules sharing memory**:

```rust
// Example: Answer a complex question

QuestionAgent
  ↓ "What did we learn about training neural networks?"
  ↓
MemoryRetrievalAgent
  ↓ Searches: [[Neural Networks]], #training, @sarah
  ↓ Finds 5 relevant note blocks
  ↓
ContextAssemblyAgent
  ↓ Assembles blocks + wikilink context
  ↓ Creates coherent narrative
  ↓
ReasoningAgent
  ↓ Synthesizes answer from memories
  ↓ Identifies gaps
  ↓
SynthesisAgent
  ↓ Writes answer as new memory
  ↓ Creates wikilinks to sources
  ↓
[New memory persists in Crucible]
```

Each agent is a **cognitive function**, not a personality:
- Retrieval (recall)
- Assembly (working memory)
- Reasoning (inference)
- Synthesis (memory consolidation)

### 5. Plugins/Rune = Self-Improvement

**The Vision**: Agents write tools for themselves

```rust
// Agent discovers it needs a tool

ResearchAgent: "I keep needing to extract tables from PDFs"
  ↓
ToolDiscoveryAgent: "No existing tool for this"
  ↓
ToolWritingAgent:
  ↓ Writes Rune script: extract_pdf_tables.rn
  ↓ Tests on sample PDFs
  ↓ Saves to plugin directory
  ↓
[New capability persists]
  ↓
ResearchAgent: *uses new tool*
```

**Key Insight**: This is like humans learning skills and remembering them.

Compare to Claude Skills:
- User writes markdown files defining skills
- Claude loads and executes them
- Skills extend Claude's capabilities

Crucible enables **agents** to write their own skills:
- Agent identifies need
- Agent writes Rune plugin
- Agent uses plugin
- Other agents discover and use it

This is **self-improvement through tool creation**.

---

## Design Principles: Memory-First Architecture

### 1. Plaintext-First = Memory Transparency

**Principle**: Agent memories should be inspectable by humans.

**Why**: Catch hallucinations, correct misconceptions, understand reasoning.

**How**:
- Markdown notes as memory records
- Git history as memory timeline
- Human-readable structure

**Anti-pattern**: Binary embeddings as sole memory store.

### 2. Local-First = Memory Privacy

**Principle**: Memories should stay on your machine.

**Why**: Privacy, speed, offline capability.

**How**:
- SurrealDB embedded (no server)
- Local vector embeddings
- Git for sync (user-controlled)

**Anti-pattern**: Cloud-hosted vector databases.

### 3. Entity-First = Structured Memory

**Principle**: Memories have structure beyond semantics.

**Why**: Enable faceted search, graph traversal, temporal queries.

**How**:
- Extract #tags, @mentions, file paths
- Bidirectional entity index
- Temporal metadata (created, modified, accessed)

**Anti-pattern**: Pure vector similarity search.

### 4. Block-First = Precise Recall

**Principle**: Retrieve specific insights, not entire documents.

**Why**: Minimize token waste, focus on relevant context.

**How**:
- Embed paragraphs/sections individually
- Maintain document structure
- Return precise blocks + surrounding context

**Anti-pattern**: Document-level embeddings.

### 5. Graph-First = Associative Memory

**Principle**: Memories connect through relationships.

**Why**: Mirror human associative recall.

**How**:
- Wikilinks as explicit connections
- Backlinks as implicit connections
- Graph traversal for discovery

**Anti-pattern**: Isolated documents with no relationships.

---

## Relevant Systems to Study

### 1. MemGPT (Most Relevant)

**Paper**: "MemGPT: Towards LLMs as Operating Systems" (2023)

**Key Ideas**:
- Virtual context management
- Memory hierarchy: working memory (context) ↔ long-term storage (disk)
- Memory swapping (move data between context and storage)
- Self-directed memory management (agent decides what to remember)

**Architecture**:
```
┌─────────────────────────────┐
│  LLM Context (Working Mem)  │  ← 4k tokens
└─────────────────────────────┘
        ↕ swap operations
┌─────────────────────────────┐
│  Long-term Storage          │  ← Unlimited
│  - Conversation history     │
│  - Facts database           │
│  - Archival memory          │
└─────────────────────────────┘
```

**Crucible Parallel**:
- Context window = Working memory
- Crucible markdown = Long-term storage
- A2A agents = Memory management operations

**What to Adopt**:
- Memory swapping strategies
- Summarization for old memories
- Importance scoring for retention

### 2. Voyager (Minecraft Agent)

**Paper**: "Voyager: An Open-Ended Embodied Agent with Large Language Models" (2023)

**Key Ideas**:
- Self-writes code skills
- Stores skills in library
- Composes skills for complex tasks
- Iterative improvement

**Architecture**:
```
GPT-4
  ↓ observes environment
  ↓ generates JavaScript skill
  ↓ executes in Minecraft
  ↓ stores if successful
  ↓
[Skill Library grows over time]
```

**Example Skill Evolution**:
1. `mine_block()` - basic mining
2. `mine_ore()` - specialized for ores
3. `collect_iron()` - full sequence: mine, smelt, collect
4. `craft_iron_tools()` - compose mining + smelting + crafting

**Crucible Parallel**:
- Rune scripts = Skills
- Plugin directory = Skill library
- Agents compose Rune scripts = Skill composition

**What to Adopt**:
- Skill naming conventions
- Success criteria for skill retention
- Skill documentation patterns
- Compositional skill architecture

### 3. Obsidian (Markdown Knowledge Graphs)

**Key Ideas**:
- Wikilinks as first-class citizens
- Graph view visualization
- Daily notes pattern
- Plugin ecosystem

**Features Relevant to Crucible**:
- `[[wikilink]]` syntax and resolution
- Backlink computation
- Graph algorithms (connected components, paths)
- Frontmatter metadata
- Search operators (tag:, file:, path:)

**What to Adopt**:
- Wikilink resolution strategies
- Backlink UI/API design
- Graph query patterns
- Metadata extraction from frontmatter

### 4. Neo4j (Knowledge Graphs)

**Key Ideas**:
- Index-free adjacency (fast graph traversal)
- Cypher query language
- Graph algorithms (PageRank, community detection)
- Property graphs (nodes + relationships + properties)

**Example Query**:
```cypher
// Find notes related to "Machine Learning" within 2 hops
MATCH (start:Note {title: "Machine Learning"})-[:LINKS_TO*1..2]-(related:Note)
WHERE related.tags CONTAINS "important"
RETURN related.title, related.created_at
ORDER BY related.created_at DESC
LIMIT 10
```

**What to Adopt**:
- Graph traversal patterns
- Path finding algorithms
- Community detection for topic clustering
- Centrality measures for note importance

### 5. RAG Systems (Retrieval-Augmented Generation)

**Key Papers**:
- "Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks" (2020)
- "Dense Passage Retrieval for Open-Domain Question Answering" (2020)
- "Self-RAG: Learning to Retrieve, Generate, and Critique through Self-Reflection" (2023)

**Patterns**:

**Basic RAG**:
```
Query → Embed → Vector Search → Retrieve Top-K → LLM Generate
```

**HyDE (Hypothetical Document Embeddings)**:
```
Query → LLM generates hypothetical answer →
Embed hypothetical answer → Vector search → Retrieve actual docs →
LLM generates real answer
```

**Self-RAG**:
```
Query → Retrieve → Generate → Critique (is this good?) →
If bad: Retrieve more → Generate again → Repeat
```

**What to Adopt**:
- HyDE for better retrieval
- Self-critique for answer quality
- Iterative refinement
- Multi-hop reasoning

---

## Crucible's A2A Patterns (Revised)

With the memory-focused framing, here's what A2A should actually be:

### Pattern 1: Memory Contexts (Worlds/Rooms)

**From ElizaOS**: Worlds and Rooms concept
**Reinterpreted for Crucible**:

- **World** = Separate memory space
  - Personal vs Work vs Research
  - Prevents memory contamination
  - Different privacy/sharing rules

- **Room** = Reasoning session
  - Project-specific context
  - Task-focused memory assembly
  - Temporary working memory

**Implementation**:
```rust
pub struct MemoryWorld {
    pub id: WorldId,
    pub name: String,
    pub agents: HashSet<AgentId>,
    pub rooms: Vec<RoomId>,
    pub root_path: PathBuf, // e.g., ~/notes/work/
}

pub struct ReasoningRoom {
    pub id: RoomId,
    pub world_id: WorldId,
    pub name: String,
    pub participants: HashSet<AgentId>,
    pub working_memory: VecDeque<MessageId>, // Recent messages
    pub context: RoomContext, // Assembled memories
    pub purpose: String, // "Research neural networks", "Debug issue #42"
}
```

**Use Case**:
```
World: "ML Research Project"
  ├─ Room: "Literature Review"
  │   └─ MemoryRetrievalAgent + SynthesisAgent
  ├─ Room: "Experiment Design"
  │   └─ ReasoningAgent + PlanningAgent
  └─ Room: "Paper Writing"
      └─ SynthesisAgent + CitationAgent
```

### Pattern 2: Memory Operations as Actions

NOT generic "actions" like ElizaOS.

But **memory-specific operations**:

```rust
pub enum MemoryOperation {
    // Retrieval
    RecallByWikilink { link: String, depth: usize },
    RecallBySemantic { query: String, limit: usize },
    RecallByEntity { entity: EntityId, relation: RelationType },
    RecallByTime { start: Time, end: Time },

    // Assembly
    AssembleContext { memories: Vec<MessageId>, max_tokens: usize },
    SummarizeMemories { memories: Vec<MessageId> },
    ExtractEntities { text: String },

    // Writing
    CreateMemory { content: String, metadata: Metadata },
    UpdateMemory { id: MessageId, changes: Patch },
    LinkMemories { from: MessageId, to: MessageId, relation: String },

    // Consolidation
    MergeMemories { memories: Vec<MessageId> },
    ArchiveMemories { memories: Vec<MessageId> },
    PruneMemories { criteria: PruneCriteria },
}
```

**Example Flow**:
```rust
// Agent answering: "What did we learn about neural networks?"

// 1. Retrieval operation
let memories = memory_ops.execute(MemoryOperation::RecallByWikilink {
    link: "[[Neural Networks]]",
    depth: 2,
}).await?;

// 2. Assembly operation
let context = memory_ops.execute(MemoryOperation::AssembleContext {
    memories,
    max_tokens: 4000,
}).await?;

// 3. Reasoning (LLM processes context)
let answer = reasoning_agent.process(context).await?;

// 4. Writing operation
memory_ops.execute(MemoryOperation::CreateMemory {
    content: answer,
    metadata: Metadata {
        tags: vec!["#neural-networks", "#qa"],
        links: vec!["[[Neural Networks]]"],
        created_by: agent_id,
    },
}).await?;
```

### Pattern 3: Memory Lifecycle Events

**Purpose**: Observe memory evolution

```rust
pub enum MemoryEvent {
    // Creation
    MemoryCreated { id: MessageId, agent_id: AgentId, entities: Vec<EntityId> },

    // Access
    MemoryRecalled { id: MessageId, query: String, relevance: f32 },
    MemoryAssembled { ids: Vec<MessageId>, purpose: String },

    // Modification
    MemoryUpdated { id: MessageId, changes: Patch },
    LinkCreated { from: MessageId, to: MessageId },

    // Consolidation
    MemorySummarized { original: Vec<MessageId>, summary: MessageId },
    MemoryArchived { id: MessageId, reason: String },
    MemoryPruned { id: MessageId, reason: PruneReason },

    // Meta
    EntityExtracted { memory_id: MessageId, entity: EntityId },
    ConnectionDiscovered { from: MessageId, to: MessageId, via: Path },
}
```

**Use Cases**:
- Trigger memory consolidation (summarize old memories)
- Identify frequently accessed memories (importance scoring)
- Track entity emergence (new concepts learned)
- Visualize knowledge graph evolution

### Pattern 4: Multi-Agent Memory Protocols

Agents coordinate through **memory protocols**, not generic messages:

```rust
pub enum MemoryProtocol {
    // Request/Response
    MemoryQuery {
        query: Query,
        requester: AgentId,
    },
    MemoryResponse {
        memories: Vec<Memory>,
        confidence: f32,
    },

    // Collaboration
    MemoryProposal {
        content: String,
        proposed_links: Vec<Link>,
        proposer: AgentId,
    },
    MemoryReview {
        proposal_id: ProposalId,
        feedback: Feedback,
        reviewer: AgentId,
    },

    // Delegation
    MemoryTask {
        task_type: TaskType, // "Summarize", "Extract", "Link"
        target: Vec<MessageId>,
        assignee: AgentId,
    },
    MemoryTaskResult {
        task_id: TaskId,
        result: TaskResult,
    },
}
```

**Example: Collaborative Research**

```rust
// Scenario: Two agents collaborating on research

ResearchAgent --MemoryQuery--> MemoryRetrievalAgent
  "Find all notes about neural network architectures"

MemoryRetrievalAgent --MemoryResponse--> ResearchAgent
  [10 relevant memories]

ResearchAgent --MemoryProposal--> SynthesisAgent
  "I think we should create a summary note linking these"

SynthesisAgent --MemoryReview--> ResearchAgent
  "Looks good, but also link to [[Transformers]] note"

ResearchAgent --MemoryTask--> SynthesisAgent
  "Create the summary note with suggested links"

SynthesisAgent --MemoryTaskResult--> ResearchAgent
  "Created: [[Neural Network Architectures Overview.md]]"
```

### Pattern 5: Self-Extension via Rune

**Inspiration**: Voyager's skill library

**Crucible Implementation**:

```rust
// Agent discovers need for new capability

// 1. Need identification
ResearchAgent: "I keep needing to extract citations from PDFs"

// 2. Check existing tools
ToolRegistryAgent.query("extract citations PDF") → None found

// 3. Generate tool specification
ToolDesignAgent.design({
    name: "extract_pdf_citations",
    inputs: ["pdf_path"],
    outputs: ["citations: Vec<Citation>"],
    description: "Extract citations from academic PDFs",
})

// 4. Generate Rune implementation
ToolWritingAgent.generate_rune(spec) → extract_pdf_citations.rn

// 5. Test tool
ToolTestingAgent.test(tool, test_cases) → Success rate: 95%

// 6. Register tool
ToolRegistryAgent.register(tool)

// 7. Document in memory
DocumentationAgent.create_memory({
    title: "extract_pdf_citations Tool",
    content: "Extracts citations from PDFs using regex + ML...",
    tags: ["#tool", "#pdf", "#citations"],
    links: ["[[Research Workflow]]", "[[PDF Tools]]"],
})

// 8. Future agents discover and use it
OtherAgent.search_tools("PDF citations") → Finds extract_pdf_citations
```

**Key Insight**: Tools become **memories** that agents discover and use.

---

## Implementation Priorities for Crucible

Given the memory-focused architecture, here are the actual priorities:

### Priority 1: Block-Level Memory Operations

**Current**: File-level processing
**Needed**: Block-level (paragraph/heading) operations

**Tasks**:
1. Parse markdown into blocks (headings, paragraphs, lists)
2. Generate embeddings per block (not per file)
3. Store block metadata (file, line range, heading context)
4. Retrieve specific blocks (not whole files)
5. Assemble blocks into coherent context

**Files**:
- `crates/crucible-core/src/parser/block.rs` - Block parser
- `crates/crucible-core/src/storage/block_store.rs` - Block storage
- `crates/crucible-embeddings/src/block_embeddings.rs` - Block embeddings

### Priority 2: Memory-Specific A2A Protocol

**Current**: Generic TypedMessage enum
**Needed**: Memory operation protocol

**Tasks**:
1. Define `MemoryOperation` enum (see above)
2. Define `MemoryProtocol` enum (see above)
3. Implement memory operation handlers
4. Add memory event system
5. Create memory-focused agent types

**Files**:
- `crates/crucible-a2a/src/protocol/memory_ops.rs` - Memory operations
- `crates/crucible-a2a/src/agents/memory_agents.rs` - Memory-focused agents

### Priority 3: Wikilink Graph Operations

**Current**: Wikilink parsing
**Needed**: Graph traversal and analysis

**Tasks**:
1. Build wikilink graph data structure
2. Implement graph traversal (DFS, BFS)
3. Add backlink computation
4. Implement path finding
5. Add graph algorithms (centrality, clustering)

**Files**:
- `crates/crucible-core/src/graph/wikilink_graph.rs` - Graph structure
- `crates/crucible-core/src/graph/traversal.rs` - Traversal algorithms
- `crates/crucible-core/src/graph/analysis.rs` - Graph analysis

### Priority 4: Memory Consolidation

**Current**: No memory management
**Needed**: Summarization, archival, pruning

**Tasks**:
1. Implement memory importance scoring
2. Add memory summarization (LLM-based)
3. Create archival strategies (time-based, access-based)
4. Implement memory pruning (token limits)
5. Add memory lifecycle management

**Files**:
- `crates/crucible-a2a/src/context/consolidation.rs` - Consolidation
- `crates/crucible-a2a/src/context/scoring.rs` - Importance scoring

### Priority 5: Self-Extension Framework

**Current**: Rune sandboxing only
**Needed**: Tool generation and registration

**Tasks**:
1. Define tool specification schema
2. Implement tool registry
3. Add tool discovery (search by capability)
4. Create tool composition patterns
5. Add tool testing framework

**Files**:
- `crates/crucible-rune/src/tools/registry.rs` - Tool registry
- `crates/crucible-rune/src/tools/discovery.rs` - Tool discovery
- `crates/crucible-rune/src/tools/composition.rs` - Tool composition

---

## Comparison: ElizaOS vs Crucible (Revised)

| Dimension | ElizaOS | Crucible |
|-----------|---------|----------|
| **Primary Purpose** | Autonomous social/crypto bots | Memory infrastructure for reasoning agents |
| **Core Abstraction** | Characters + Rooms | Memories + Wikilinks |
| **Memory Model** | Ephemeral messages + vector DB | Persistent markdown + knowledge graph |
| **Agent Role** | Independent actors with personalities | Cognitive functions sharing memory |
| **Tool Use** | Predefined actions (reply, follow, mute) | Self-generated Rune scripts |
| **Coordination** | Implicit via shared rooms | Explicit via memory protocols |
| **Knowledge** | Stored in LLM weights + vector DB | Stored in markdown + embeddings |
| **Self-Improvement** | Plugin installation | Tool generation and learning |
| **Target User** | Web3 developers, bot creators | AI researchers, reasoning systems |

**Conclusion**: These are fundamentally different systems solving orthogonal problems.

---

## Why This Matters: Post-Plateau AI

### The Scaling Hypothesis is Breaking Down

**Evidence**:
- GPT-4 → GPT-4.5 → GPT-5: Diminishing returns
- Compute costs growing exponentially
- Data exhaustion (running out of quality training data)
- Emergent capabilities plateauing

**Consensus**: Scaling alone won't get us to AGI

### The New Paradigm: Small Models + External Systems

**Architecture**:
```
Small Reasoning Model (7B-30B params)
    ↕
Memory System (Crucible)
    ↕
Tool System (MCP + Rune)
    ↕
Coordination System (A2A)
```

**Why This Works**:
1. **Reasoning is general**: Small models can reason well
2. **Knowledge is specific**: Store externally, retrieve as needed
3. **Tools are extensible**: Generate new capabilities dynamically
4. **Coordination is compositional**: Combine specialized agents

**Examples**:
- **Voyager**: 7B model + skill library → Open-ended Minecraft play
- **MemGPT**: 7B model + memory system → Long-term conversations
- **ReAct**: 7B model + tools → Complex problem solving

**Crucible's Role**: Provide the memory and tool infrastructure.

---

## Concrete Next Steps

### 1. Study MemGPT Architecture

**Why**: Most relevant to Crucible's memory focus

**Tasks**:
- Read paper: "MemGPT: Towards LLMs as Operating Systems"
- Analyze memory swapping strategies
- Implement similar context management
- Adapt for markdown-based storage

### 2. Study Voyager Skill System

**Why**: Self-extension via tool generation

**Tasks**:
- Read paper: "Voyager: An Open-Ended Embodied Agent"
- Analyze skill storage and composition
- Design similar Rune-based system
- Implement tool registry and discovery

### 3. Implement Block-Level Memory

**Why**: Foundation for all other features

**Tasks**:
- Parse markdown into blocks
- Generate block embeddings
- Store block metadata
- Implement block retrieval API

### 4. Design Memory Operation Protocol

**Why**: A2A needs to be memory-focused

**Tasks**:
- Define MemoryOperation enum
- Define MemoryProtocol enum
- Implement operation handlers
- Add memory events

### 5. Build Wikilink Graph

**Why**: Associative memory requires graph traversal

**Tasks**:
- Build graph data structure
- Implement traversal algorithms
- Add backlink computation
- Create graph query API

---

## Conclusion

Crucible is not "Rust ElizaOS." It's not even in the same category.

**ElizaOS**: Autonomous bot platform for social/crypto agents
**Crucible**: Memory infrastructure for reasoning-focused AI systems

Crucible's vision:
1. **Markdown as memory medium** (persistent, version-controlled, human-readable)
2. **Wikilinks as associative memory** (explicit concept connections)
3. **Block-level retrieval** (precise context assembly)
4. **Memory-focused A2A** (collaborative cognition over shared memory)
5. **Self-extension via Rune** (tool generation and learning)

This positions Crucible as infrastructure for **post-plateau AI systems** that use:
- Small, efficient reasoning models
- External memory systems
- Tool-use capabilities
- Multi-agent coordination

The relevant systems to study are:
- **MemGPT** (memory hierarchy)
- **Voyager** (skill generation)
- **RAG systems** (retrieval patterns)
- **Knowledge graphs** (Neo4j, Obsidian)

NOT:
- Social media bots
- Character-driven agents
- Platform integrations
- Crypto/Web3 systems

---

## References

### Papers
- "MemGPT: Towards LLMs as Operating Systems" (2023)
- "Voyager: An Open-Ended Embodied Agent with Large Language Models" (2023)
- "Retrieval-Augmented Generation for Knowledge-Intensive NLP Tasks" (2020)
- "Self-RAG: Learning to Retrieve, Generate, and Critique through Self-Reflection" (2023)
- "ReAct: Synergizing Reasoning and Acting in Language Models" (2022)

### Systems
- MemGPT: https://github.com/cpacker/MemGPT
- Voyager: https://github.com/MineDojo/Voyager
- Obsidian: https://obsidian.md
- Neo4j: https://neo4j.com

### Crucible Documentation
- Philosophy: `docs/PHILOSOPHY.md`
- Architecture: `docs/ARCHITECTURE.md`
- A2A Source: `crates/crucible-a2a/`
- Core Source: `crates/crucible-core/`

---

**Document Version**: 1.0
**Last Updated**: 2025-11-21
**Author**: Claude Code Analysis Agent
