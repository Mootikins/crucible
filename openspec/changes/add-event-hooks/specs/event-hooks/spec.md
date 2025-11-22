# Event System and API Surface Specification

**Date**: 2025-11-21
**Status**: Draft
**Version**: 0.1.0

---

## Executive Summary

This document specifies Crucible's event system and API surface, which serves as the **Application Binary Interface (ABI) boundary** between:
- **Internal**: Memory infrastructure for reasoning agents
- **External**: Platform for autonomous agents and integrations

The event system enables:
1. **Hooks**: Trigger behaviors when events occur (like git hooks)
2. **Webhooks**: External systems receive event notifications
3. **Real-time streams**: WebSocket event subscriptions
4. **API integrations**: HTTP REST API for memory operations

---

## Design Principles

### 1. Events as First-Class Citizens

Events are the **source of truth** for what happens in Crucible:
- Every state change emits an event
- Events are immutable and ordered
- Events can be replayed for debugging/auditing
- Events enable reactive programming

### 2. Stable Event Schema

Events define the ABI boundary:
- Versioned schema (semver)
- Backward compatibility guarantees
- Clear deprecation policy
- JSON Schema + OpenAPI definitions

### 3. Hooks as Local Automation

Hooks are local event handlers:
- Triggered synchronously or asynchronously
- Can block operations (pre-hooks) or react (post-hooks)
- Defined in markdown/YAML files
- Can execute Rune scripts, shell commands, or HTTP calls

### 4. Webhooks as External Integration

Webhooks push events to external systems:
- HTTPS POST with event payload
- Retry logic with exponential backoff
- Signature verification (HMAC)
- Filter events by type/content

### 5. API as Programmatic Access

REST API enables external control:
- Create/query/update memories
- Trigger flows
- Subscribe to events (WebSocket)
- Manage hooks and webhooks

---

## Event Taxonomy

### Event Categories

```
CrucibleEvent
├─ MemoryEvent        (memory lifecycle)
├─ GraphEvent         (knowledge graph structure)
├─ FileEvent          (file system operations)
├─ AgentEvent         (agent actions)
├─ FlowEvent          (workflow execution)
├─ SystemEvent        (system-level changes)
└─ HookEvent          (hook execution)
```

### Event Hierarchy

```rust
/// Top-level event type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum CrucibleEvent {
    Memory(MemoryEvent),
    Graph(GraphEvent),
    File(FileEvent),
    Agent(AgentEvent),
    Flow(FlowEvent),
    System(SystemEvent),
    Hook(HookEvent),
}

/// Common metadata for all events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event ID (UUIDv7 for time-ordered IDs)
    pub id: EventId,

    /// Event timestamp (ISO 8601)
    pub timestamp: DateTime<Utc>,

    /// Event version (semver)
    pub version: String,

    /// Source of the event
    pub source: EventSource,

    /// Correlation ID (for tracing related events)
    pub correlation_id: Option<CorrelationId>,

    /// User/agent that triggered the event
    pub actor: Option<ActorId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSource {
    User { user_id: String },
    Agent { agent_id: AgentId },
    System { component: String },
    External { integration: String },
}
```

---

## Memory Events

Events related to memory (notes, messages, knowledge) lifecycle.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum MemoryEvent {
    /// New memory created
    Created {
        id: MessageId,
        content: String,
        entities: Vec<EntityId>,
        links: Vec<String>,      // wikilinks
        tags: Vec<String>,
        metadata: HashMap<String, Value>,
        file_path: Option<PathBuf>,
    },

    /// Memory updated
    Updated {
        id: MessageId,
        previous_hash: Hash,
        new_hash: Hash,
        changes: MemoryPatch,
    },

    /// Memory deleted
    Deleted {
        id: MessageId,
        reason: Option<String>,
    },

    /// Memory linked to another
    Linked {
        from: MessageId,
        to: MessageId,
        link_type: LinkType,      // Wikilink, Backlink, Semantic, etc.
        strength: Option<f32>,     // relevance score
    },

    /// Memory unlinked
    Unlinked {
        from: MessageId,
        to: MessageId,
        link_type: LinkType,
    },

    /// Memory tagged
    Tagged {
        id: MessageId,
        tag: String,
    },

    /// Memory untagged
    Untagged {
        id: MessageId,
        tag: String,
    },

    /// Memory accessed (retrieved/queried)
    Accessed {
        id: MessageId,
        query: Option<String>,
        relevance: Option<f32>,
    },

    /// Memory consolidated (summarized/archived)
    Consolidated {
        original_ids: Vec<MessageId>,
        summary_id: MessageId,
        consolidation_type: ConsolidationType,
    },

    /// Memory pruned (removed from active context)
    Pruned {
        id: MessageId,
        reason: PruneReason,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPatch {
    /// Content changes (unified diff format)
    pub content_diff: Option<String>,

    /// Added entities
    pub entities_added: Vec<EntityId>,

    /// Removed entities
    pub entities_removed: Vec<EntityId>,

    /// Metadata changes
    pub metadata_changes: HashMap<String, ValueChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsolidationType {
    Summary,      // Summarized multiple memories
    Archive,      // Archived old memories
    Merge,        // Merged duplicate/similar memories
}
```

### Example: Memory Created Event

```json
{
  "type": "Memory",
  "data": {
    "event": "Created",
    "payload": {
      "id": "msg_01H8XQZJQK7VZYFN3E8QGW9ABC",
      "content": "# Neural Networks\n\nRelated to [[Deep Learning]] and [[Machine Learning]].\n\nKey concepts:\n- Backpropagation\n- Gradient descent\n- Activation functions\n\n#ai #ml",
      "entities": ["ent_deep_learning", "ent_machine_learning"],
      "links": ["[[Deep Learning]]", "[[Machine Learning]]"],
      "tags": ["#ai", "#ml"],
      "metadata": {
        "created_at": "2025-11-21T10:30:00Z",
        "file_path": "notes/neural-networks.md"
      },
      "file_path": "/home/user/notes/neural-networks.md"
    }
  },
  "metadata": {
    "id": "evt_01H8XQZJQK7VZYFN3E8QGWXYZ",
    "timestamp": "2025-11-21T10:30:00.123Z",
    "version": "1.0.0",
    "source": {
      "User": {
        "user_id": "user_alice"
      }
    },
    "correlation_id": null,
    "actor": null
  }
}
```

---

## Graph Events

Events related to knowledge graph structure.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum GraphEvent {
    /// Node added to graph
    NodeAdded {
        node_id: NodeId,
        node_type: NodeType,    // Memory, Entity, Tag, etc.
        properties: HashMap<String, Value>,
    },

    /// Node removed from graph
    NodeRemoved {
        node_id: NodeId,
    },

    /// Edge created between nodes
    EdgeCreated {
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
        weight: Option<f32>,
        properties: HashMap<String, Value>,
    },

    /// Edge removed
    EdgeRemoved {
        from: NodeId,
        to: NodeId,
        edge_type: EdgeType,
    },

    /// Path discovered between nodes
    PathDiscovered {
        from: NodeId,
        to: NodeId,
        path: Vec<NodeId>,
        distance: usize,
        algorithm: PathAlgorithm,
    },

    /// Community detected (cluster of related nodes)
    CommunityDetected {
        community_id: CommunityId,
        nodes: Vec<NodeId>,
        cohesion: f32,          // how tightly connected
        algorithm: String,
    },

    /// Graph metrics computed
    MetricsComputed {
        metric_type: MetricType,
        results: HashMap<NodeId, f32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Memory,
    Entity,
    Tag,
    File,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    Wikilink,
    Backlink,
    Semantic { similarity: f32 },
    Temporal { time_diff: Duration },
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathAlgorithm {
    BFS,
    DFS,
    Dijkstra,
    AStar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    PageRank,
    Betweenness,
    Centrality,
    Degree,
}
```

---

## File Events

Events related to file system operations.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum FileEvent {
    /// File created
    Created {
        path: PathBuf,
        size: u64,
        hash: Hash,
    },

    /// File modified
    Modified {
        path: PathBuf,
        previous_hash: Hash,
        new_hash: Hash,
        size: u64,
    },

    /// File deleted
    Deleted {
        path: PathBuf,
        last_hash: Hash,
    },

    /// File renamed/moved
    Renamed {
        from: PathBuf,
        to: PathBuf,
    },

    /// Directory created
    DirectoryCreated {
        path: PathBuf,
    },

    /// Directory deleted
    DirectoryDeleted {
        path: PathBuf,
        file_count: usize,
    },

    /// File processed (parsed, embedded, indexed)
    Processed {
        path: PathBuf,
        process_type: ProcessType,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessType {
    Parse,
    Embed,
    Index,
    Analyze,
}
```

---

## Agent Events

Events related to agent actions.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum AgentEvent {
    /// Agent registered
    Registered {
        agent_id: AgentId,
        name: String,
        capabilities: Vec<String>,
        version: String,
    },

    /// Agent unregistered
    Unregistered {
        agent_id: AgentId,
        reason: Option<String>,
    },

    /// Agent joined a room
    JoinedRoom {
        agent_id: AgentId,
        room_id: RoomId,
    },

    /// Agent left a room
    LeftRoom {
        agent_id: AgentId,
        room_id: RoomId,
    },

    /// Agent executed an action
    ActionExecuted {
        agent_id: AgentId,
        action_name: String,
        inputs: HashMap<String, Value>,
        outputs: HashMap<String, Value>,
        duration_ms: u64,
        success: bool,
        error: Option<String>,
    },

    /// Agent sent a message
    MessageSent {
        agent_id: AgentId,
        room_id: Option<RoomId>,
        recipient: Option<AgentId>,
        message: TypedMessage,
    },

    /// Agent received a message
    MessageReceived {
        agent_id: AgentId,
        sender: AgentId,
        message: TypedMessage,
    },

    /// Agent created a tool (Rune script)
    ToolCreated {
        agent_id: AgentId,
        tool_name: String,
        tool_path: PathBuf,
        description: String,
    },

    /// Agent used a tool
    ToolUsed {
        agent_id: AgentId,
        tool_name: String,
        inputs: HashMap<String, Value>,
        outputs: HashMap<String, Value>,
        duration_ms: u64,
    },
}
```

---

## Flow Events

Events related to workflow/flow execution.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum FlowEvent {
    /// Flow started
    Started {
        flow_id: FlowId,
        flow_name: String,
        trigger: FlowTrigger,
        inputs: HashMap<String, Value>,
    },

    /// Flow step completed
    StepCompleted {
        flow_id: FlowId,
        step_name: String,
        step_index: usize,
        outputs: HashMap<String, Value>,
        duration_ms: u64,
    },

    /// Flow completed
    Completed {
        flow_id: FlowId,
        success: bool,
        outputs: HashMap<String, Value>,
        total_duration_ms: u64,
    },

    /// Flow failed
    Failed {
        flow_id: FlowId,
        step_name: Option<String>,
        error: String,
        partial_outputs: HashMap<String, Value>,
    },

    /// Flow cancelled
    Cancelled {
        flow_id: FlowId,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowTrigger {
    Manual { user_id: String },
    Event { event_id: EventId },
    Scheduled { cron: String },
    Hook { hook_id: HookId },
}
```

---

## System Events

System-level events.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum SystemEvent {
    /// System started
    Started {
        version: String,
        config: HashMap<String, Value>,
    },

    /// System shutting down
    Shutdown {
        reason: String,
    },

    /// Room created
    RoomCreated {
        room_id: RoomId,
        world_id: WorldId,
        name: String,
        creator: ActorId,
    },

    /// Room deleted
    RoomDeleted {
        room_id: RoomId,
    },

    /// World created
    WorldCreated {
        world_id: WorldId,
        name: String,
        root_path: PathBuf,
    },

    /// World deleted
    WorldDeleted {
        world_id: WorldId,
    },

    /// Error occurred
    Error {
        component: String,
        error_type: String,
        message: String,
        context: HashMap<String, Value>,
    },

    /// Configuration changed
    ConfigChanged {
        key: String,
        old_value: Option<Value>,
        new_value: Value,
    },
}
```

---

## Hook Events

Events related to hook execution.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "payload")]
pub enum HookEvent {
    /// Hook registered
    Registered {
        hook_id: HookId,
        name: String,
        event_filter: EventFilter,
        action: HookAction,
    },

    /// Hook triggered
    Triggered {
        hook_id: HookId,
        triggering_event: EventId,
    },

    /// Hook executed
    Executed {
        hook_id: HookId,
        triggering_event: EventId,
        success: bool,
        duration_ms: u64,
        output: Option<String>,
        error: Option<String>,
    },

    /// Hook failed
    Failed {
        hook_id: HookId,
        triggering_event: EventId,
        error: String,
        retry_count: u32,
    },

    /// Hook unregistered
    Unregistered {
        hook_id: HookId,
        reason: Option<String>,
    },
}
```

---

## Hook System

Hooks are local event handlers that enable automation.

### Hook Definition Format

Hooks are defined in markdown files with YAML frontmatter:

```markdown
---
name: auto-summarize-research
description: Automatically summarize research notes when 5+ notes are created
version: 1.0.0

trigger:
  events:
    - type: Memory
      event: Created
      filter:
        tags:
          contains: "#research"

condition:
  # Optional: additional conditions (Rune expression)
  script: |
    // Only trigger if 5+ research notes in last hour
    let recent = context.query_memories({
      tags: ["#research"],
      since: now() - hours(1),
    });
    recent.len() >= 5

action:
  type: RuneScript
  script: summarize_research.rn
  inputs:
    tag: "#research"

  # Alternative: shell command
  # type: Shell
  # command: ./scripts/summarize.sh {{event.payload.id}}

  # Alternative: HTTP call
  # type: HTTP
  # method: POST
  # url: https://api.example.com/summarize
  # body:
  #   event: "{{event}}"

options:
  async: true           # Run asynchronously (don't block)
  timeout_ms: 30000     # 30 second timeout
  retry:
    max_attempts: 3
    backoff: exponential
  debounce_ms: 5000     # Don't trigger within 5s of last trigger
---

# Auto-Summarize Research Notes

This hook automatically creates a summary note when 5 or more research
notes are created within an hour.

## Usage

The hook monitors for `Memory.Created` events with the `#research` tag.
When the condition is met, it runs `summarize_research.rn` which:
1. Queries all recent research notes
2. Generates a summary using an LLM
3. Creates a new summary note with wikilinks to sources

## Example Output

Creates a note like `Research Summary - 2025-11-21.md` with:
- Summary of key findings
- Links to all source notes
- Tags: #research #summary #auto-generated
```

### Hook Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    pub id: HookId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,

    pub trigger: HookTrigger,
    pub condition: Option<HookCondition>,
    pub action: HookAction,
    pub options: HookOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookTrigger {
    /// Events to listen for
    pub events: Vec<EventFilter>,

    /// Optional pre/post hook (blocking vs reactive)
    pub timing: HookTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookTiming {
    Pre,   // Before the event is committed (can block/modify)
    Post,  // After the event is committed (reactive)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Event type to match
    pub event_type: String,  // e.g., "Memory.Created"

    /// Optional field filters
    pub filter: Option<HashMap<String, FilterCondition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterCondition {
    Equals { value: Value },
    Contains { value: Value },
    StartsWith { value: String },
    Matches { regex: String },
    GreaterThan { value: f64 },
    LessThan { value: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCondition {
    /// Rune script that returns bool
    pub script: String,

    /// Script timeout
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum HookAction {
    /// Run a Rune script
    RuneScript {
        script: PathBuf,
        inputs: HashMap<String, Value>,
    },

    /// Execute shell command
    Shell {
        command: String,
        env: Option<HashMap<String, String>>,
    },

    /// Make HTTP request
    HTTP {
        method: HttpMethod,
        url: String,
        headers: Option<HashMap<String, String>>,
        body: Option<Value>,
    },

    /// Trigger a flow
    TriggerFlow {
        flow_name: String,
        inputs: HashMap<String, Value>,
    },

    /// Send to webhook
    Webhook {
        url: String,
        auth: Option<WebhookAuth>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOptions {
    /// Run asynchronously (don't block event)
    pub async_execution: bool,

    /// Timeout for hook execution
    pub timeout_ms: u64,

    /// Retry policy
    pub retry: Option<RetryPolicy>,

    /// Debounce (min time between executions)
    pub debounce_ms: Option<u64>,

    /// Rate limit (max executions per time period)
    pub rate_limit: Option<RateLimit>,
}
```

### Example Hooks

#### 1. Auto-link Related Notes

```yaml
name: auto-link-related
trigger:
  events:
    - type: Memory
      event: Created
  timing: Post

condition:
  script: |
    // Only for notes with content
    event.payload.content.len() > 100

action:
  type: RuneScript
  script: auto_link.rn
  inputs:
    memory_id: "{{event.payload.id}}"
    similarity_threshold: 0.7

options:
  async: true
  timeout_ms: 10000
```

#### 2. Commit After N Edits

```yaml
name: auto-commit-after-edits
trigger:
  events:
    - type: File
      event: Modified
  timing: Post

condition:
  script: |
    // Count uncommitted changes
    let uncommitted = git.status().len();
    uncommitted >= 5

action:
  type: Shell
  command: |
    git add -A
    git commit -m "Auto-commit: {{event.payload.path}}"

options:
  async: false
  debounce_ms: 60000  # Max once per minute
```

#### 3. Notify on Important Tag

```yaml
name: notify-important
trigger:
  events:
    - type: Memory
      event: Tagged
      filter:
        tag:
          equals: "#important"
  timing: Post

action:
  type: HTTP
  method: POST
  url: https://ntfy.sh/my-crucible-notifications
  body:
    title: "Important note tagged"
    message: "{{event.payload.id}}"
    priority: high

options:
  async: true
  retry:
    max_attempts: 3
```

#### 4. Trigger Research Flow

```yaml
name: start-research-flow
trigger:
  events:
    - type: Memory
      event: Created
      filter:
        tags:
          contains: "#research-question"
  timing: Post

action:
  type: TriggerFlow
  flow_name: answer_research_question
  inputs:
    question_id: "{{event.payload.id}}"

options:
  async: true
  timeout_ms: 300000  # 5 minutes
```

---

## Webhook System

Webhooks push events to external HTTP endpoints.

### Webhook Configuration

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub name: String,
    pub url: Url,
    pub enabled: bool,

    /// Events to forward
    pub event_filters: Vec<EventFilter>,

    /// Authentication
    pub auth: Option<WebhookAuth>,

    /// Delivery options
    pub options: WebhookOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WebhookAuth {
    /// Bearer token
    Bearer { token: String },

    /// HMAC signature
    HMAC { secret: String, algorithm: HmacAlgorithm },

    /// Basic auth
    Basic { username: String, password: String },

    /// Custom header
    Header { name: String, value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookOptions {
    /// Retry policy
    pub retry: RetryPolicy,

    /// Request timeout
    pub timeout_ms: u64,

    /// Custom headers
    pub headers: Option<HashMap<String, String>>,

    /// Batch events (send multiple events per request)
    pub batch: Option<BatchConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: BackoffStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Fixed { delay_ms: u64 },
    Linear { initial_ms: u64, increment_ms: u64 },
    Exponential { initial_ms: u64, multiplier: f64, max_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Max events per batch
    pub max_size: usize,

    /// Max time to wait before sending batch
    pub max_wait_ms: u64,
}
```

### Webhook Request Format

```http
POST /webhook/endpoint HTTP/1.1
Host: example.com
Content-Type: application/json
User-Agent: Crucible/0.1.0
X-Crucible-Event-Id: evt_01H8XQZJQK7VZYFN3E8QGWXYZ
X-Crucible-Event-Type: Memory.Created
X-Crucible-Signature: sha256=abc123...
X-Crucible-Delivery: delivery_01H8XQZ...

{
  "event": {
    "type": "Memory",
    "data": {
      "event": "Created",
      "payload": { ... }
    },
    "metadata": { ... }
  },
  "webhook": {
    "id": "webhook_01H8XQZ...",
    "name": "My Webhook"
  }
}
```

### Webhook Response Handling

```rust
impl WebhookDelivery {
    pub async fn deliver(&self, webhook: &Webhook, event: &CrucibleEvent) -> Result<()> {
        let mut attempt = 0;

        loop {
            attempt += 1;

            let response = self.send_request(webhook, event).await;

            match response {
                Ok(resp) if resp.status().is_success() => {
                    // Success
                    self.record_delivery(webhook.id, event.metadata.id, true).await;
                    return Ok(());
                }
                Ok(resp) if resp.status().is_server_error() && attempt < webhook.options.retry.max_attempts => {
                    // Retry on 5xx
                    let delay = self.calculate_backoff(attempt, &webhook.options.retry.backoff);
                    tokio::time::sleep(delay).await;
                    continue;
                }
                _ => {
                    // Failed permanently
                    self.record_delivery(webhook.id, event.metadata.id, false).await;
                    return Err(WebhookError::DeliveryFailed);
                }
            }
        }
    }
}
```

---

## HTTP REST API

External systems can interact with Crucible via HTTP API.

### API Versioning

```
/api/v1/...   (stable)
/api/v2/...   (next version)
```

Versioning strategy:
- **Major version**: Breaking changes to request/response format
- **Minor version**: Backward-compatible additions
- **Patch version**: Bug fixes

### Endpoints

#### Memory Operations

```
POST   /api/v1/memory              Create memory
GET    /api/v1/memory/:id          Get memory by ID
PUT    /api/v1/memory/:id          Update memory
DELETE /api/v1/memory/:id          Delete memory
POST   /api/v1/memory/query        Query memories
POST   /api/v1/memory/:id/link     Link memories
```

#### Graph Operations

```
GET    /api/v1/graph/nodes/:id     Get node
GET    /api/v1/graph/edges         Get edges
POST   /api/v1/graph/traverse      Traverse graph
POST   /api/v1/graph/paths         Find paths
POST   /api/v1/graph/communities   Detect communities
```

#### Agent Operations

```
POST   /api/v1/agents              Register agent
GET    /api/v1/agents/:id          Get agent info
DELETE /api/v1/agents/:id          Unregister agent
POST   /api/v1/agents/:id/actions  Execute action
```

#### Flow Operations

```
POST   /api/v1/flows/:name/trigger Trigger flow
GET    /api/v1/flows/:id           Get flow status
DELETE /api/v1/flows/:id           Cancel flow
GET    /api/v1/flows/:id/events    Get flow events
```

#### Hook Operations

```
POST   /api/v1/hooks               Register hook
GET    /api/v1/hooks               List hooks
GET    /api/v1/hooks/:id           Get hook
PUT    /api/v1/hooks/:id           Update hook
DELETE /api/v1/hooks/:id           Delete hook
POST   /api/v1/hooks/:id/enable    Enable hook
POST   /api/v1/hooks/:id/disable   Disable hook
```

#### Webhook Operations

```
POST   /api/v1/webhooks            Create webhook
GET    /api/v1/webhooks            List webhooks
GET    /api/v1/webhooks/:id        Get webhook
PUT    /api/v1/webhooks/:id        Update webhook
DELETE /api/v1/webhooks/:id        Delete webhook
GET    /api/v1/webhooks/:id/deliveries  Get delivery history
```

#### Event Operations

```
GET    /api/v1/events              List events (paginated)
GET    /api/v1/events/:id          Get event by ID
POST   /api/v1/events/query        Query events
```

### Example API Usage

#### Create Memory

```http
POST /api/v1/memory HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>

{
  "content": "# Deep Learning\n\nRelated to [[Neural Networks]].\n\n#ai #ml",
  "metadata": {
    "file_path": "notes/deep-learning.md"
  },
  "tags": ["#ai", "#ml"]
}
```

Response:

```json
{
  "id": "msg_01H8XQZJQK7VZYFN3E8QGW9ABC",
  "content": "# Deep Learning\n\nRelated to [[Neural Networks]].\n\n#ai #ml",
  "entities": ["ent_neural_networks"],
  "links": ["[[Neural Networks]]"],
  "tags": ["#ai", "#ml"],
  "metadata": {
    "file_path": "notes/deep-learning.md",
    "created_at": "2025-11-21T10:30:00Z"
  },
  "created_at": "2025-11-21T10:30:00Z",
  "updated_at": "2025-11-21T10:30:00Z"
}
```

#### Query Memories

```http
POST /api/v1/memory/query HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>

{
  "semantic": "neural network training",
  "tags": ["#ai"],
  "limit": 5,
  "min_relevance": 0.7
}
```

Response:

```json
{
  "results": [
    {
      "memory": {
        "id": "msg_...",
        "content": "...",
        ...
      },
      "relevance": 0.92,
      "match_reason": "semantic"
    },
    ...
  ],
  "total": 12,
  "page": 1,
  "per_page": 5
}
```

#### Trigger Flow

```http
POST /api/v1/flows/research_topic/trigger HTTP/1.1
Content-Type: application/json
Authorization: Bearer <token>

{
  "inputs": {
    "topic": "transformer architectures",
    "depth": "comprehensive"
  }
}
```

Response:

```json
{
  "flow_id": "flow_01H8XQZJQK7VZYFN3E8QGWXYZ",
  "status": "running",
  "started_at": "2025-11-21T10:30:00Z",
  "steps_completed": 0,
  "steps_total": 5,
  "outputs": {}
}
```

---

## WebSocket API

Real-time event streaming via WebSocket.

### Connection

```javascript
const ws = new WebSocket('ws://localhost:8080/api/v1/events/stream');

ws.onopen = () => {
  // Subscribe to events
  ws.send(JSON.stringify({
    action: 'subscribe',
    filters: [
      { event_type: 'Memory.Created', filter: { tags: { contains: '#important' } } },
      { event_type: 'Graph.PathDiscovered' }
    ]
  }));
};

ws.onmessage = (msg) => {
  const event = JSON.parse(msg.data);
  console.log('Event received:', event);
};
```

### Protocol

#### Subscribe

```json
{
  "action": "subscribe",
  "filters": [
    {
      "event_type": "Memory.Created",
      "filter": {
        "tags": { "contains": "#important" }
      }
    }
  ]
}
```

#### Unsubscribe

```json
{
  "action": "unsubscribe",
  "filters": [
    { "event_type": "Memory.Created" }
  ]
}
```

#### Event Message

```json
{
  "type": "event",
  "event": {
    "type": "Memory",
    "data": {
      "event": "Created",
      "payload": { ... }
    },
    "metadata": { ... }
  }
}
```

#### Ping/Pong (Keep-Alive)

```json
{
  "type": "ping"
}
```

```json
{
  "type": "pong"
}
```

---

## Event Store

Events are persisted for replay and auditing.

### Storage Schema

```rust
pub struct EventStore {
    /// Append-only event log
    events: Vec<StoredEvent>,

    /// Indexes for fast queries
    indexes: EventIndexes,
}

pub struct StoredEvent {
    pub id: EventId,
    pub event_type: String,
    pub event: CrucibleEvent,
    pub metadata: EventMetadata,
    pub stored_at: DateTime<Utc>,
}

pub struct EventIndexes {
    /// Index by event type
    by_type: HashMap<String, Vec<EventId>>,

    /// Index by actor
    by_actor: HashMap<ActorId, Vec<EventId>>,

    /// Index by correlation ID
    by_correlation: HashMap<CorrelationId, Vec<EventId>>,

    /// Index by timestamp (sorted)
    by_time: BTreeMap<DateTime<Utc>, Vec<EventId>>,
}
```

### Query API

```rust
impl EventStore {
    /// Query events by filter
    pub async fn query(&self, query: EventQuery) -> Result<Vec<StoredEvent>> {
        // Use indexes to efficiently find matching events
    }

    /// Replay events to rebuild state
    pub async fn replay(&self, from: EventId) -> impl Stream<Item = CrucibleEvent> {
        // Stream events from a given point
    }

    /// Get events for correlation ID (trace)
    pub async fn trace(&self, correlation_id: CorrelationId) -> Result<Vec<StoredEvent>> {
        // Get all events in a trace
    }
}

pub struct EventQuery {
    pub event_types: Option<Vec<String>>,
    pub actor: Option<ActorId>,
    pub correlation_id: Option<CorrelationId>,
    pub from_time: Option<DateTime<Utc>>,
    pub to_time: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
```

---

## Security

### Authentication

```rust
pub enum AuthMethod {
    /// API key
    ApiKey { key: String },

    /// JWT token
    JWT { token: String },

    /// OAuth2 token
    OAuth2 { token: String },
}
```

### Authorization

```rust
pub struct Permission {
    pub resource: Resource,
    pub action: Action,
}

pub enum Resource {
    Memory { id: Option<MessageId> },
    Graph,
    Agent { id: Option<AgentId> },
    Flow { name: Option<String> },
    Hook { id: Option<HookId> },
    Webhook { id: Option<WebhookId> },
    Event,
}

pub enum Action {
    Read,
    Write,
    Delete,
    Execute,
}
```

### Webhook Signature Verification

```rust
impl WebhookAuth {
    pub fn sign(&self, payload: &[u8]) -> String {
        match self {
            WebhookAuth::HMAC { secret, algorithm } => {
                let signature = hmac_sign(algorithm, secret, payload);
                format!("{}={}", algorithm, hex::encode(signature))
            }
            _ => String::new(),
        }
    }

    pub fn verify(&self, payload: &[u8], signature: &str) -> bool {
        let expected = self.sign(payload);
        constant_time_eq(signature.as_bytes(), expected.as_bytes())
    }
}
```

---

## Implementation Plan

### Phase 1: Core Event System (Week 1-2)

1. Define event schema (Rust types)
2. Implement event bus (publish/subscribe)
3. Add event store (persistence)
4. Create event indexes
5. Add event query API

**Files**:
- `crates/crucible-events/src/types.rs` - Event types
- `crates/crucible-events/src/bus.rs` - Event bus
- `crates/crucible-events/src/store.rs` - Event store
- `crates/crucible-events/src/query.rs` - Query API

### Phase 2: Hook System (Week 3-4)

1. Define hook configuration format
2. Implement hook registry
3. Add hook trigger logic
4. Create built-in hook actions (Rune, Shell, HTTP)
5. Add hook management CLI

**Files**:
- `crates/crucible-hooks/src/config.rs` - Hook configuration
- `crates/crucible-hooks/src/registry.rs` - Hook registry
- `crates/crucible-hooks/src/executor.rs` - Hook execution
- `crates/crucible-cli/src/commands/hooks.rs` - CLI commands

### Phase 3: Webhook System (Week 5)

1. Implement webhook registry
2. Add webhook delivery logic
3. Create retry mechanism
4. Add signature verification
5. Implement delivery tracking

**Files**:
- `crates/crucible-webhooks/src/registry.rs` - Webhook registry
- `crates/crucible-webhooks/src/delivery.rs` - Delivery logic
- `crates/crucible-webhooks/src/auth.rs` - Authentication

### Phase 4: HTTP API (Week 6-8)

1. Set up Axum server
2. Implement API routes
3. Add OpenAPI spec generation
4. Create authentication middleware
5. Add rate limiting

**Files**:
- `crates/crucible-api/src/server.rs` - HTTP server
- `crates/crucible-api/src/routes/` - API routes
- `crates/crucible-api/src/auth.rs` - Auth middleware
- `crates/crucible-api/src/openapi.rs` - OpenAPI spec

### Phase 5: WebSocket API (Week 9)

1. Implement WebSocket server
2. Add subscription management
3. Create event filtering
4. Add keep-alive mechanism

**Files**:
- `crates/crucible-api/src/websocket.rs` - WebSocket server
- `crates/crucible-api/src/subscription.rs` - Subscription management

### Phase 6: Integration & Examples (Week 10-12)

1. Create example hooks
2. Build Discord bot example
3. Build Telegram bot example
4. Create Python SDK
5. Write documentation

**Files**:
- `examples/hooks/` - Example hooks
- `examples/bots/discord/` - Discord bot
- `examples/bots/telegram/` - Telegram bot
- `sdk/python/` - Python SDK

---

## Examples

### Example 1: Auto-Tag ML Papers

Hook that automatically tags papers with machine learning topics:

```yaml
name: auto-tag-ml-papers
trigger:
  events:
    - type: Memory
      event: Created
      filter:
        content:
          matches: ".*\\.(pdf|PDF)$"

action:
  type: RuneScript
  script: classify_paper.rn
  inputs:
    memory_id: "{{event.payload.id}}"

options:
  async: true
  timeout_ms: 30000
```

`classify_paper.rn`:

```rune
pub async fn main(memory_id) {
    let memory = crucible::get_memory(memory_id);
    let content = crucible::extract_text(memory.file_path);

    let topics = llm::classify(content, [
        "deep-learning",
        "reinforcement-learning",
        "nlp",
        "computer-vision"
    ]);

    for topic in topics {
        crucible::tag_memory(memory_id, `#${topic}`);
    }
}
```

### Example 2: Discord Research Assistant

Discord bot that uses Crucible as knowledge backend:

```rust
// examples/bots/discord/src/main.rs

use crucible_api::CrucibleClient;
use serenity::all::*;

struct ResearchBot {
    crucible: CrucibleClient,
}

#[async_trait]
impl EventHandler for ResearchBot {
    async fn message(&self, ctx: Context, msg: Message) {
        // !research <topic> - Research a topic
        if msg.content.starts_with("!research") {
            let topic = msg.content[10..].trim();

            // Trigger research flow in Crucible
            let flow = self.crucible
                .trigger_flow("research_topic", hashmap! {
                    "topic" => topic,
                    "channel_id" => msg.channel_id.to_string(),
                })
                .await
                .unwrap();

            msg.reply(&ctx, format!("🔍 Researching {}...", topic)).await.unwrap();

            // Subscribe to flow completion
            let result = self.crucible.wait_for_flow(flow.id).await.unwrap();

            msg.reply(&ctx, format!("📚 Research complete!\n\n{}", result.summary)).await.unwrap();
        }

        // !recall <query> - Search knowledge base
        if msg.content.starts_with("!recall") {
            let query = msg.content[8..].trim();

            let memories = self.crucible
                .query_memory(MemoryQuery {
                    semantic: Some(query.to_string()),
                    limit: 3,
                    ..Default::default()
                })
                .await
                .unwrap();

            let response = memories.iter()
                .map(|m| format!("**{}**\n{}", m.title(), m.snippet()))
                .collect::<Vec<_>>()
                .join("\n\n");

            msg.reply(&ctx, response).await.unwrap();
        }
    }
}
```

### Example 3: Automated Research Pipeline

Flow triggered when a research question is tagged:

```yaml
# flows/research_pipeline.yml

name: research_pipeline
description: Automated research pipeline for questions

trigger:
  events:
    - type: Memory
      event: Tagged
      filter:
        tag:
          equals: "#research-question"

steps:
  - name: extract_question
    agent: QuestionExtractionAgent
    inputs:
      memory_id: "{{trigger.event.payload.id}}"
    outputs:
      question: "{{result.question}}"

  - name: search_existing
    agent: MemoryRetrievalAgent
    inputs:
      query: "{{steps.extract_question.outputs.question}}"
      limit: 10
    outputs:
      existing_knowledge: "{{result.memories}}"

  - name: identify_gaps
    agent: AnalysisAgent
    inputs:
      question: "{{steps.extract_question.outputs.question}}"
      knowledge: "{{steps.search_existing.outputs.existing_knowledge}}"
    outputs:
      gaps: "{{result.gaps}}"

  - name: search_external
    agent: WebSearchAgent
    condition: "{{steps.identify_gaps.outputs.gaps.len() > 0}}"
    inputs:
      queries: "{{steps.identify_gaps.outputs.gaps}}"
    outputs:
      external_sources: "{{result.sources}}"

  - name: synthesize_answer
    agent: SynthesisAgent
    inputs:
      question: "{{steps.extract_question.outputs.question}}"
      internal: "{{steps.search_existing.outputs.existing_knowledge}}"
      external: "{{steps.search_external.outputs.external_sources}}"
    outputs:
      answer: "{{result.answer}}"
      sources: "{{result.sources}}"

  - name: create_answer_note
    agent: MemoryWritingAgent
    inputs:
      content: |
        # Answer: {{steps.extract_question.outputs.question}}

        {{steps.synthesize_answer.outputs.answer}}

        ## Sources
        {{#each steps.synthesize_answer.outputs.sources}}
        - {{this}}
        {{/each}}
      tags:
        - "#research-answer"
        - "#auto-generated"
      links:
        - "{{trigger.event.payload.id}}"
```

---

## Conclusion

The event system and API surface provide:

1. **Hooks**: Local automation triggered by events
2. **Webhooks**: External integration via HTTP callbacks
3. **REST API**: Programmatic access to Crucible
4. **WebSocket**: Real-time event streaming
5. **Event Store**: Audit trail and replay capability

This creates a stable ABI boundary that enables:
- **Primary use**: Memory infrastructure for reasoning agents
- **Secondary use**: Platform for autonomous agents (ElizaOS-style)
- **Ecosystem**: Third-party integrations and tools

The event-driven architecture allows Crucible to maintain its core identity while supporting diverse use cases through a clean, versioned interface.

---

## References

- Event Sourcing: https://martinfowler.com/eaaDev/EventSourcing.html
- Webhook Best Practices: https://webhooks.fyi/
- OpenAPI Specification: https://swagger.io/specification/
- WebSocket Protocol: https://datatracker.ietf.org/doc/html/rfc6455

---

**Document Version**: 0.1.0
**Last Updated**: 2025-11-21
**Status**: Draft - Ready for Review
