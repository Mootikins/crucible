# Add Shared Memory & Communication: Worlds and Rooms

## Why

Crucible's A2A system currently provides basic message-passing between agents, but lacks **memory context isolation** and **collaborative cognition patterns** needed for multi-agent reasoning systems.

### The Problem: Flat Memory Space

Current architecture:
- Agents share a single, flat memory space
- No isolation between different projects/contexts
- No room for focused reasoning sessions
- Difficult to prevent memory contamination
- Hard to implement multi-tenant scenarios

### The Solution: Hierarchical Memory Contexts

Inspired by ElizaOS's Worlds/Rooms pattern (reinterpreted for memory infrastructure):

**Worlds** = Separate memory spaces (workspaces, projects)
- Work vs Personal vs Research
- Different privacy/sharing rules
- Isolated knowledge graphs
- Separate file system roots

**Rooms** = Reasoning sessions within a World
- Project-specific contexts
- Task-focused memory assembly
- Temporary working memory
- Agent collaboration spaces

### Why This Matters for Memory Infrastructure

Unlike ElizaOS (which uses Rooms for chat channels), Crucible uses them for **collaborative cognition**:

```
World: "ML Research Project"
  ├─ Room: "Literature Review"
  │   └─ MemoryRetrievalAgent + SynthesisAgent
  │       → Assemble papers on transformers
  ├─ Room: "Experiment Design"
  │   └─ ReasoningAgent + PlanningAgent
  │       → Design architecture experiments
  └─ Room: "Paper Writing"
      └─ SynthesisAgent + CitationAgent
          → Write results section
```

Each room has its own:
- Working memory (recent messages)
- Context window (assembled memories)
- Participant agents
- Purpose/goal

This enables:
1. **Context isolation**: Rooms don't cross-contaminate
2. **Focused reasoning**: Agents work on specific tasks
3. **Memory scoping**: Room-level context assembly and pruning
4. **Multi-tenancy**: Different users/projects in different worlds

## What Changes

### NEW CAPABILITY: Shared Memory Architecture

**Core Types:**

```rust
/// A World is a separate memory space with isolated knowledge graph
pub struct World {
    pub id: WorldId,
    pub name: String,
    pub root_path: PathBuf,        // e.g., ~/notes/work/
    pub agents: HashSet<AgentId>,
    pub rooms: Vec<RoomId>,
    pub created_at: SystemTime,
}

/// A Room is a reasoning session within a World
pub struct Room {
    pub id: RoomId,
    pub world_id: WorldId,
    pub name: String,
    pub participants: HashSet<AgentId>,
    pub working_memory: VecDeque<MessageId>,  // Recent context
    pub context_window: ContextWindow,        // Assembled memories
    pub purpose: String,                      // "Research transformers"
    pub created_at: SystemTime,
}

/// Enhanced MessageEnvelope with room/world context
pub struct MessageEnvelope {
    pub sender: AgentId,
    pub recipient: Option<AgentId>,  // None = broadcast to room
    pub room_id: RoomId,
    pub world_id: WorldId,
    pub message: TypedMessage,
    pub message_id: MessageId,
    pub timestamp: SystemTime,
}
```

**Memory Operations:**

```rust
/// Memory-specific A2A protocol
pub enum MemoryOperation {
    // Retrieval
    RecallByWikilink { link: String, depth: usize },
    RecallBySemantic { query: String, limit: usize },
    RecallByEntity { entity: EntityId },

    // Assembly
    AssembleContext { memories: Vec<MessageId>, max_tokens: usize },
    SummarizeMemories { memories: Vec<MessageId> },

    // Writing
    CreateMemory { content: String, metadata: Metadata },
    LinkMemories { from: MessageId, to: MessageId },

    // Consolidation
    MergeMemories { memories: Vec<MessageId> },
    PruneMemories { criteria: PruneCriteria },
}

/// Memory-focused protocol (not generic coordination)
pub enum MemoryProtocol {
    MemoryQuery { query: Query, requester: AgentId },
    MemoryResponse { memories: Vec<Memory>, confidence: f32 },
    MemoryProposal { content: String, proposed_links: Vec<Link> },
    MemoryReview { proposal_id: ProposalId, feedback: Feedback },
    MemoryTask { task_type: TaskType, target: Vec<MessageId> },
    MemoryTaskResult { task_id: TaskId, result: TaskResult },
}
```

**Room-Scoped Routing:**

- Messages sent without recipient → broadcast to room participants
- Messages with recipient → direct to specific agent in room
- Agents can subscribe to multiple rooms
- Room-level context assembly (only memories relevant to room's purpose)

**World Registry & Room Manager:**

```rust
pub struct WorldRegistry {
    worlds: HashMap<WorldId, World>,
    by_path: HashMap<PathBuf, WorldId>,
}

pub struct RoomManager {
    rooms: HashMap<RoomId, Room>,
    by_world: HashMap<WorldId, Vec<RoomId>>,
}
```

### Changes to Existing Systems

**A2A Protocol** (`crates/crucible-a2a/`):
- Add `world_id` and `room_id` to `MessageEnvelope`
- Update `LocalAgentBus` for room-scoped broadcasts
- Add `WorldRegistry` and `RoomManager`
- Implement room join/leave mechanics

**Context Management** (`crates/crucible-a2a/src/context/`):
- Add per-room `ContextWindow`
- Implement room-scoped entity tracking
- Add room-level pruning strategies

**CLI** (`crates/crucible-cli/`):
- Add `world create/delete/list` commands
- Add `room create/delete/list/join/leave` commands
- Add `message send --room <room>` for room-scoped messaging
- Add room/world visualization

## Impact

### Affected Specs

- **agent-system** (reference) - Agents operate within rooms
- **acp-integration** (reference) - ACP queries scoped to worlds
- **tool-system** (reference) - Tools access room context
- **shared-memory** (new capability) - Core memory architecture

### Affected Code

**New Components:**
- `crates/crucible-a2a/src/context/workspace.rs` - World/Room management
- `crates/crucible-a2a/src/protocol/memory_ops.rs` - Memory operations
- `crates/crucible-cli/src/commands/world.rs` - World CLI
- `crates/crucible-cli/src/commands/room.rs` - Room CLI

**Modified Components:**
- `crates/crucible-a2a/src/protocol/messages.rs` - Update MessageEnvelope
- `crates/crucible-a2a/src/transport/local.rs` - Room-aware routing
- `crates/crucible-a2a/src/bus/message_bus.rs` - Room context assembly
- `crates/crucible-core/src/agent/types.rs` - Add room membership

### Migration Path

**Phase 1: Core Types** (Week 1)
- Add World, Room, WorldId, RoomId types
- Implement WorldRegistry and RoomManager
- Update MessageEnvelope schema

**Phase 2: Transport** (Week 2)
- Modify LocalAgentBus for room routing
- Add room join/leave operations
- Implement room broadcasts

**Phase 3: Context** (Week 3)
- Per-room ContextWindow
- Room-scoped entity tracking
- Room-level pruning

**Phase 4: CLI** (Week 4)
- World/room management commands
- Room-scoped messaging
- Visualization

**Backward Compatibility:**
- Default world created automatically
- Default room per world
- Agents without world/room assignment → default world/room
- Existing message envelopes → migrated with default IDs

## Success Criteria

- [ ] Can create separate worlds for work/personal/research
- [ ] Can create rooms within worlds for focused tasks
- [ ] Agents can join/leave rooms
- [ ] Room-scoped message broadcasts work
- [ ] Per-room context windows isolate memory
- [ ] CLI supports world/room management
- [ ] Tests verify context isolation between rooms
- [ ] Documentation explains worlds/rooms mental model
- [ ] Migration from flat memory space works seamlessly

## Alternatives Considered

### 1. Keep Flat Memory Space

**Pros**: Simpler, no migration needed
**Cons**: No multi-tenancy, no context isolation, memory contamination

**Rejected**: Crucible aims to be infrastructure for complex multi-agent systems. Context isolation is essential.

### 2. Namespaces Instead of Worlds/Rooms

**Pros**: Simpler mental model (like Kubernetes namespaces)
**Cons**: Single level of hierarchy, no reasoning session concept

**Rejected**: Two-level hierarchy (World → Rooms) better matches user mental models (projects → tasks).

### 3. Copy ElizaOS Exactly

**Pros**: Proven pattern
**Cons**: ElizaOS Rooms are chat channels, not reasoning sessions

**Rejected**: Crucible reinterprets the pattern for memory infrastructure, not social bots.

## Open Questions

1. Should worlds have separate file system roots, or logical separation only?
   - **Proposed**: Separate roots for true isolation

2. Can rooms span multiple worlds?
   - **Proposed**: No, rooms belong to exactly one world

3. What happens when an agent leaves a room mid-task?
   - **Proposed**: Task fails gracefully, other agents notified

4. How deep can room hierarchy go?
   - **Proposed**: Flat (no sub-rooms), use multiple rooms instead

5. Should room context persist after all agents leave?
   - **Proposed**: Yes, rooms are persistent until explicitly deleted

## References

- ElizaOS Worlds/Rooms: https://github.com/elizaOS/eliza
- MemGPT Memory Hierarchy: "MemGPT: Towards LLMs as Operating Systems" (2023)
- Crucible A2A Implementation: `crates/crucible-a2a/`
- Crucible Context Management: `crates/crucible-a2a/src/context/`

## Related Work

- **add-agent-system**: Agents that operate within rooms
- **add-event-hooks**: Events scoped to rooms/worlds
- **add-meta-systems**: Plugins that access room context
