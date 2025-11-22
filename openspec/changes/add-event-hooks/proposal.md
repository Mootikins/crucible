# Add Event & Hook Systems: Reactive Automation and API Surface

## Why

Crucible needs an **event-driven architecture** to enable:

1. **Local automation** (hooks): Trigger behaviors when events occur
2. **External integration** (webhooks): Push events to external systems
3. **API access** (REST/WebSocket): Programmatic control of memory operations
4. **Observability**: Track what happens in the system
5. **Dual-purpose architecture**: Memory infrastructure + agent platform

### The Problem: No Reactive Layer

Current Crucible is **imperative only**:
- Agents explicitly call operations
- No way to react to changes
- No automation patterns
- No external integrations
- No audit trail

### The Solution: Events as First-Class Citizens

Every state change emits an event:
```
File Modified → FileEvent.Modified
  ↓
Memory Updated → MemoryEvent.Updated
  ↓
Hook Triggered → Execute auto-link script
  ↓
Memory Linked → MemoryEvent.Linked
  ↓
Webhook Fired → External system notified
```

### Why This Creates Dual-Purpose Architecture

**Primary Use** (Memory Infrastructure):
```
Small LLM ↔ Crucible Memory ↔ Rune Tools
     ↕
 Event System (observability)
```

**Secondary Use** (Agent Platform):
```
Discord Bot → HTTP API → Crucible Memory
     ↕                        ↕
WebSocket Events ← Event System
```

Events become the **ABI boundary** between internal memory infrastructure and external autonomous agents.

### Key Insight: Hooks Like Git Hooks

Git hooks enable automation:
- `pre-commit`: Lint code before commit
- `post-receive`: Deploy after push
- `pre-push`: Run tests before pushing

Crucible hooks enable similar patterns:
- `pre-memory-create`: Validate memory format
- `post-memory-tag`: Trigger research flow
- `post-file-modified`: Auto-link related notes
- `pre-memory-delete`: Require confirmation

## What Changes

### NEW CAPABILITY: Event System

**Event Taxonomy** (7 categories):

```rust
pub enum CrucibleEvent {
    Memory(MemoryEvent),      // Memory lifecycle
    Graph(GraphEvent),        // Knowledge graph structure
    File(FileEvent),          // File system operations
    Agent(AgentEvent),        // Agent actions
    Flow(FlowEvent),          // Workflow execution
    System(SystemEvent),      // System-level changes
    Hook(HookEvent),          // Hook execution
}

// Example: Memory events
pub enum MemoryEvent {
    Created { id, content, entities, links, tags, metadata },
    Updated { id, previous_hash, new_hash, changes },
    Deleted { id, reason },
    Linked { from, to, link_type, strength },
    Tagged { id, tag },
    Accessed { id, query, relevance },
    Consolidated { original_ids, summary_id },
    Pruned { id, reason },
}

// All events have metadata
pub struct EventMetadata {
    pub id: EventId,              // UUIDv7 (time-ordered)
    pub timestamp: DateTime<Utc>, // ISO 8601
    pub version: String,          // semver
    pub source: EventSource,      // User, Agent, System, External
    pub correlation_id: Option<CorrelationId>, // For tracing
    pub actor: Option<ActorId>,   // Who triggered it
}
```

**Event Store** (Persistent Log):

```rust
pub struct EventStore {
    events: Vec<StoredEvent>,        // Append-only log
    indexes: EventIndexes,           // Fast queries
}

// Query events
pub struct EventQuery {
    pub event_types: Option<Vec<String>>,
    pub actor: Option<ActorId>,
    pub correlation_id: Option<CorrelationId>,
    pub from_time: Option<DateTime<Utc>>,
    pub to_time: Option<DateTime<Utc>>,
}

// Replay events (event sourcing)
pub async fn replay(&self, from: EventId) -> impl Stream<Item = CrucibleEvent>
```

### NEW CAPABILITY: Hook System

**Hook Definition** (Markdown + YAML):

```markdown
---
name: auto-link-related
description: Automatically link semantically similar notes
version: 1.0.0

trigger:
  events:
    - type: Memory
      event: Created
      filter:
        content:
          matches: ".{100,}"  # Only notes with 100+ chars
  timing: Post  # Run after event is committed

condition:
  # Optional: Rune expression returning bool
  script: |
    event.payload.tags.contains("#research")

action:
  type: RuneScript
  script: auto_link.rn
  inputs:
    memory_id: "{{event.payload.id}}"
    similarity_threshold: 0.7

options:
  async: true           # Don't block event
  timeout_ms: 10000     # 10 second timeout
  debounce_ms: 5000     # Min 5s between triggers
  retry:
    max_attempts: 3
    backoff: exponential
---

# Auto-Link Related Notes

This hook automatically creates wikilinks between semantically similar notes.
```

**Hook Actions** (4 types):

```rust
pub enum HookAction {
    // Run Rune script
    RuneScript {
        script: PathBuf,
        inputs: HashMap<String, Value>,
    },

    // Execute shell command
    Shell {
        command: String,
        env: Option<HashMap<String, String>>,
    },

    // Make HTTP request
    HTTP {
        method: HttpMethod,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<Value>,
    },

    // Trigger workflow
    TriggerFlow {
        flow_name: String,
        inputs: HashMap<String, Value>,
    },
}
```

**Hook Timing** (Pre vs Post):

```rust
pub enum HookTiming {
    Pre,   // Before event is committed (can block/modify)
    Post,  // After event is committed (reactive)
}
```

Pre-hooks can:
- Validate input (reject invalid memories)
- Transform data (normalize tags)
- Block operations (require confirmation)

Post-hooks can:
- React to changes (auto-link notes)
- Trigger workflows (start research flow)
- Send notifications (important tag added)

### NEW CAPABILITY: Webhook System

**Webhook Configuration:**

```rust
pub struct Webhook {
    pub id: WebhookId,
    pub name: String,
    pub url: Url,
    pub event_filters: Vec<EventFilter>,
    pub auth: Option<WebhookAuth>,
    pub options: WebhookOptions,
}

pub enum WebhookAuth {
    Bearer { token: String },
    HMAC { secret: String, algorithm: HmacAlgorithm },
    Basic { username: String, password: String },
}
```

**Webhook Request Format:**

```http
POST /webhook/endpoint HTTP/1.1
Host: example.com
Content-Type: application/json
X-Crucible-Event-Id: evt_01H8XQZ...
X-Crucible-Event-Type: Memory.Created
X-Crucible-Signature: sha256=abc123...

{
  "event": {
    "type": "Memory",
    "data": { "event": "Created", "payload": {...} },
    "metadata": {...}
  }
}
```

**Retry Logic:**

- Retry on 5xx errors
- Exponential backoff: 2s, 4s, 8s, 16s
- Max 5 attempts
- Track delivery success/failure

### NEW CAPABILITY: HTTP REST API

**Endpoints:**

```
POST   /api/v1/memory              Create memory
POST   /api/v1/memory/query        Query memories
POST   /api/v1/graph/traverse      Traverse graph
POST   /api/v1/flows/:name/trigger Trigger flow
POST   /api/v1/hooks               Register hook
POST   /api/v1/webhooks            Create webhook
GET    /api/v1/events              List events
```

**Features:**
- OpenAPI 3.0 specification
- Versioned API (semver)
- Authentication (API key, JWT, OAuth2)
- Rate limiting
- Pagination

### NEW CAPABILITY: WebSocket API

**Real-time event streaming:**

```javascript
const ws = new WebSocket('ws://localhost:8080/api/v1/events/stream');

ws.send(JSON.stringify({
  action: 'subscribe',
  filters: [
    { event_type: 'Memory.Created', filter: { tags: { contains: '#important' } } }
  ]
}));

ws.onmessage = (msg) => {
  const event = JSON.parse(msg.data);
  // Handle event
};
```

## Impact

### Affected Specs

- **shared-memory** (reference) - Events scoped to rooms/worlds
- **agent-system** (reference) - AgentEvent types
- **tool-system** (reference) - Tools trigger events
- **event-hooks** (new capability) - Core event/hook architecture

### Affected Code

**New Components:**
- `crates/crucible-events/src/types.rs` - Event types
- `crates/crucible-events/src/bus.rs` - Event bus (pub/sub)
- `crates/crucible-events/src/store.rs` - Event persistence
- `crates/crucible-hooks/src/config.rs` - Hook configuration
- `crates/crucible-hooks/src/registry.rs` - Hook registry
- `crates/crucible-hooks/src/executor.rs` - Hook execution
- `crates/crucible-webhooks/src/delivery.rs` - Webhook delivery
- `crates/crucible-api/src/server.rs` - HTTP server (Axum)
- `crates/crucible-api/src/websocket.rs` - WebSocket server
- `crates/crucible-cli/src/commands/hooks.rs` - Hook management CLI

**Modified Components:**
- All core operations emit events (memory, graph, file, agent)
- AgentRuntime subscribes to events
- CLI shows real-time event stream

### Integration Points

**With Shared Memory:**
- Events include `world_id` and `room_id`
- Room-scoped event subscriptions
- Per-world hook configurations

**With Agent System:**
- AgentEvent types for agent actions
- Agents can register hooks
- Agents can subscribe to events

**With Tool System:**
- Tools emit events when executed
- Hooks can trigger tool execution
- Tools can query event history

**With Rune/Meta-Systems:**
- Hooks execute Rune scripts
- Rune scripts can emit custom events
- Event handlers written in Rune

## Success Criteria

- [ ] All state changes emit events
- [ ] Events persisted to event store
- [ ] Can query events by type/time/actor
- [ ] Hooks defined in markdown files
- [ ] Pre-hooks can block operations
- [ ] Post-hooks run asynchronously
- [ ] Webhooks deliver events to external systems
- [ ] HTTP API supports memory operations
- [ ] WebSocket streams events in real-time
- [ ] CLI commands for hook/webhook management
- [ ] OpenAPI spec auto-generated
- [ ] Rate limiting works
- [ ] HMAC signature verification works
- [ ] Event replay functionality works
- [ ] Correlation IDs enable tracing
- [ ] Tests verify hook execution
- [ ] Documentation with examples

## Examples

### Example 1: Auto-Commit Hook

```yaml
name: auto-commit-edits
trigger:
  events:
    - type: File
      event: Modified
  timing: Post

condition:
  script: |
    let uncommitted = git::status().len();
    uncommitted >= 5

action:
  type: Shell
  command: |
    git add -A
    git commit -m "Auto-commit: {{event.payload.path}}"

options:
  debounce_ms: 60000  # Max once per minute
```

### Example 2: Discord Bot Backend

```rust
// Discord bot using Crucible via HTTP API

use crucible_api::CrucibleClient;

#[async_trait]
impl EventHandler for Bot {
    async fn message(&self, ctx: Context, msg: Message) {
        if msg.content.starts_with("!remember") {
            // Store in Crucible
            self.crucible.create_memory(CreateMemoryRequest {
                content: msg.content[10..].to_string(),
                metadata: hashmap! {
                    "source" => "discord",
                    "author" => msg.author.name.clone(),
                },
            }).await?;
        }

        if msg.content.starts_with("!recall") {
            // Query Crucible
            let memories = self.crucible.query_memory(MemoryQuery {
                semantic: Some(msg.content[8..].to_string()),
                limit: 3,
            }).await?;

            msg.reply(&ctx, format_memories(&memories)).await?;
        }
    }
}
```

### Example 3: Research Pipeline Triggered by Tag

```yaml
name: trigger-research-flow
trigger:
  events:
    - type: Memory
      event: Tagged
      filter:
        tag:
          equals: "#research-question"
  timing: Post

action:
  type: TriggerFlow
  flow_name: research_pipeline
  inputs:
    question_id: "{{event.payload.id}}"

options:
  async: true
  timeout_ms: 300000  # 5 minutes
```

## Migration Path

### Phase 1: Event System (Week 1-2)

- Define event types
- Implement event bus
- Add event store
- Create indexes
- Add query API

### Phase 2: Hook System (Week 3-4)

- Hook configuration parser
- Hook registry
- Hook executor
- Pre/post hook logic
- CLI commands

### Phase 3: Webhook System (Week 5)

- Webhook registry
- Delivery logic
- Retry mechanism
- Signature verification

### Phase 4: HTTP API (Week 6-8)

- Axum server setup
- API routes
- OpenAPI generation
- Authentication
- Rate limiting

### Phase 5: WebSocket API (Week 9)

- WebSocket server
- Subscription management
- Event filtering

### Phase 6: Integration (Week 10-12)

- Example hooks
- Example bots (Discord, Telegram)
- Python SDK
- Documentation

## Alternatives Considered

### 1. Polling Instead of Events

**Pros**: Simpler
**Cons**: Inefficient, no real-time, high latency

**Rejected**: Events enable real-time reactivity and observability.

### 2. Callbacks Instead of Hooks

**Pros**: Programmatic API
**Cons**: Requires Rust code, not user-extensible

**Rejected**: Hooks in markdown files are user-editable and agent-writable.

### 3. Message Queue (Kafka/Redis) Instead of In-Process Events

**Pros**: Distributed, scalable
**Cons**: Complex, overkill for MVP

**Deferred**: In-process event bus for MVP, message queue for Phase 2 distributed.

### 4. gRPC Instead of REST

**Pros**: Type-safe, efficient
**Cons**: Less accessible, requires codegen

**Rejected**: REST + OpenAPI is more accessible for integrations.

## Open Questions

1. Should event store be in-memory or persistent?
   - **Proposed**: Persistent (SurrealDB) for audit trail

2. Can hooks modify events before they're committed?
   - **Proposed**: Yes, pre-hooks can modify and block

3. Should webhooks batch events?
   - **Proposed**: Optional batching for high-volume scenarios

4. What's the event retention policy?
   - **Proposed**: Configurable (default: 30 days, important events: forever)

5. Can agents subscribe to other agents' events only?
   - **Proposed**: Yes, via event filters

## Security Considerations

1. **Hook Sandboxing**: Rune scripts run in sandboxed environment
2. **Webhook Authentication**: HMAC signatures prevent spoofing
3. **API Rate Limiting**: Prevent abuse
4. **Event Filtering**: Users can't see events they don't have permission for
5. **Pre-Hook Validation**: Can enforce business rules

## References

- Event Sourcing: https://martinfowler.com/eaaDev/EventSourcing.html
- Webhook Security: https://webhooks.fyi/security/
- OpenAPI Spec: https://swagger.io/specification/
- Git Hooks: https://git-scm.com/book/en/v2/Customizing-Git-Git-Hooks

## Related Work

- **add-shared-memory**: Events scoped to worlds/rooms
- **add-agent-system**: AgentEvent types
- **add-meta-systems**: Rune scripts as hook actions
