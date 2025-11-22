# Tasks: Shared Memory & Communication

## Phase 1: Core Types (Week 1)

- [ ] Define `World`, `Room`, `WorldId`, `RoomId` types in `crates/crucible-a2a/src/context/workspace.rs`
- [ ] Implement `WorldRegistry` with CRUD operations
- [ ] Implement `RoomManager` with CRUD operations
- [ ] Add `world_id` and `room_id` fields to `MessageEnvelope`
- [ ] Update `MessageEnvelope` serialization/deserialization
- [ ] Add migration path for existing message envelopes (default world/room)
- [ ] Write unit tests for World and Room types
- [ ] Write tests for WorldRegistry operations
- [ ] Write tests for RoomManager operations

## Phase 2: Transport Integration (Week 2)

- [ ] Modify `LocalAgentBus` to support room-scoped routing in `crates/crucible-a2a/src/transport/local.rs`
- [ ] Add `join_room(agent_id, room_id)` method to LocalAgentBus
- [ ] Add `leave_room(agent_id, room_id)` method to LocalAgentBus
- [ ] Implement room-scoped broadcast (send to all participants in room)
- [ ] Update `send()` to check room membership before delivery
- [ ] Add room participant tracking to transport layer
- [ ] Write integration tests for room-scoped messaging
- [ ] Write tests for join/leave mechanics
- [ ] Write tests for broadcast functionality

## Phase 3: Memory Operations Protocol (Week 2)

- [ ] Define `MemoryOperation` enum in `crates/crucible-a2a/src/protocol/memory_ops.rs`
- [ ] Define `MemoryProtocol` enum for agent-to-agent memory coordination
- [ ] Implement `MemoryOperation` handlers
- [ ] Add `RecallByWikilink`, `RecallBySemantic`, `RecallByEntity` operations
- [ ] Add `AssembleContext`, `SummarizeMemories` operations
- [ ] Add `CreateMemory`, `LinkMemories` operations
- [ ] Add `MergeMemories`, `PruneMemories` operations
- [ ] Write tests for each memory operation
- [ ] Write tests for memory protocol message flow

## Phase 4: Context Windows (Week 3)

- [ ] Add per-room `ContextWindow` to `Room` struct
- [ ] Implement room-scoped entity tracking in `crates/crucible-a2a/src/context/store.rs`
- [ ] Add room-level pruning strategies
- [ ] Implement context assembly scoped to room's purpose
- [ ] Add methods to query memories by room
- [ ] Add methods to get room-specific entity index
- [ ] Write tests for room-scoped context windows
- [ ] Write tests for room-level pruning
- [ ] Benchmark context assembly performance

## Phase 5: CLI Integration (Week 4)

- [ ] Create `crates/crucible-cli/src/commands/world.rs`
- [ ] Implement `world create <name> --path <path>` command
- [ ] Implement `world delete <id>` command
- [ ] Implement `world list` command
- [ ] Implement `world info <id>` command
- [ ] Create `crates/crucible-cli/src/commands/room.rs`
- [ ] Implement `room create <name> --world <id> --purpose <purpose>` command
- [ ] Implement `room delete <id>` command
- [ ] Implement `room list [--world <id>]` command
- [ ] Implement `room join <room-id> --agent <agent-id>` command
- [ ] Implement `room leave <room-id> --agent <agent-id>` command
- [ ] Update `message send` command to support `--room <room-id>` flag
- [ ] Add room/world visualization (ASCII tree or similar)
- [ ] Write CLI integration tests
- [ ] Update CLI documentation

## Phase 6: Agent Integration (Week 5)

- [ ] Update `AgentDefinition` to include `worlds` and `rooms` fields
- [ ] Add methods to register agent with world
- [ ] Add methods to join/leave rooms
- [ ] Implement agent discovery by room
- [ ] Implement agent discovery by world
- [ ] Update agent registry to track world/room membership
- [ ] Write tests for agent room membership
- [ ] Write tests for agent discovery

## Phase 7: Documentation & Examples (Week 6)

- [ ] Write user guide for worlds and rooms
- [ ] Write developer guide for memory operations
- [ ] Create example: Multi-world setup (work/personal/research)
- [ ] Create example: Research room with multiple agents
- [ ] Create example: Memory query scoped to room
- [ ] Create example: Room-level context assembly
- [ ] Add architecture diagrams
- [ ] Update README with worlds/rooms section

## Testing Checklist

- [ ] Unit tests for all new types
- [ ] Integration tests for room-scoped messaging
- [ ] Integration tests for world isolation
- [ ] Integration tests for memory operations
- [ ] CLI command tests
- [ ] Performance benchmarks for context assembly
- [ ] Migration tests (flat → hierarchical)
- [ ] Backward compatibility tests

## Documentation Checklist

- [ ] API documentation (rustdoc)
- [ ] User guide
- [ ] Developer guide
- [ ] Architecture decision record (ADR)
- [ ] Migration guide
- [ ] Examples

## Success Metrics

- [ ] Can create 100+ worlds without performance degradation
- [ ] Can create 1000+ rooms without performance degradation
- [ ] Room-scoped broadcasts deliver in <10ms
- [ ] Context assembly for room completes in <100ms
- [ ] CLI commands respond in <100ms
- [ ] All tests pass
- [ ] Code coverage >80%
