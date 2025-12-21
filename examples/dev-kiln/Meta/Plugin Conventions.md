# Plugin System Architecture Best Practices for Crucible

A synthesis of design patterns from Neovim, Emacs, Obsidian, VSCode, Bevy, and modern event-driven systems.

---

## Executive Summary

After analyzing successful plugin architectures across major applications, several core principles emerge that are particularly relevant for Crucible's event-driven knowledge management system:

1. **Lifecycle-aware registration with automatic cleanup** (Obsidian, Bevy)
2. **Pre/Post event hooks with priority ordering** (Neovim, Emacs)
3. **Capability-based host functions for sandboxed plugins** (Extism, WASM)
4. **Declarative contribution points** (VSCode)
5. **Reactive observers alongside scheduled systems** (Bevy ECS)

---

## 1. Event System Foundations

### 1.1 Event Types and Naming Conventions

**From Neovim:**
Events follow a consistent naming pattern that indicates timing and scope:
- `{Resource}{Pre|Post}{Action}` — e.g., `BufWritePre`, `BufWritePost`
- Pre events allow cancellation/modification; Post events are notifications only
- Pattern matching for event filtering (glob patterns, regex)

**From Emacs:**
- Normal hooks: Functions called with no arguments, names end in `-hook`
- Abnormal hooks: Functions receive arguments or return values, names end in `-functions`
- Single-function hooks: Variable holds one function, name ends in `-function`

**Crucible Application:**
```rust
pub enum EventTiming {
    Before,  // Can cancel/modify
    After,   // Notification only
}

pub enum CrucibleEvent {
    // Content events
    ContentCreating { id: ContentId, timing: Before },
    ContentCreated { id: ContentId },  // After is implicit
    ContentModifying { id: ContentId, timing: Before },
    ContentModified { id: ContentId, changes: ChangeSet },
    
    // Knowledge graph events
    LinkCreating { source: ContentId, target: ContentId },
    LinkCreated { source: ContentId, target: ContentId },
    
    // Agent events
    AgentTaskStarting { agent_id: AgentId, task: Task },
    AgentTaskCompleted { agent_id: AgentId, result: TaskResult },
    
    // Semantic events
    EmbeddingComputed { id: ContentId, vector: Embedding },
    SimilarityQueryCompleted { query_id: QueryId, results: Vec<Match> },
}
```

### 1.2 Event Priority and Ordering

**From Emacs `add-hook`:**
```elisp
;; depth parameter controls ordering:
;; -100 to 100, where negative runs earlier
(add-hook 'hook-name #'function :depth -50)
```

**From Neovim:**
- Event groups (`augroup`) for organizational control
- `clear = true` option prevents duplicate registrations
- Autocommands run in registration order by default

**Crucible Design:**
```rust
pub struct EventPriority(i16);

impl EventPriority {
    pub const FIRST: Self = Self(-100);
    pub const EARLY: Self = Self(-50);
    pub const NORMAL: Self = Self(0);
    pub const LATE: Self = Self(50);
    pub const LAST: Self = Self(100);
}

pub struct EventSubscription {
    event_type: TypeId,
    handler: Box<dyn EventHandler>,
    priority: EventPriority,
    plugin_id: PluginId,
    group: Option<EventGroup>,
}
```

---

## 2. Plugin Lifecycle Management

### 2.1 Automatic Resource Cleanup

**From Obsidian:**
The Component base class provides automatic cleanup through registration methods:
```typescript
// Events automatically detach when plugin unloads
this.registerEvent(app.on('event-name', callback));

// DOM events cleaned up automatically
this.registerDomEvent(element, 'click', callback);

// Intervals cleared automatically
this.registerInterval(setInterval(callback, 1000));
```

**From Bevy:**
Plugins implement a clear lifecycle with build-time registration:
```rust
impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_system)
           .add_systems(Update, update_system)
           .add_event::<MyEvent>()
           .insert_resource(MyResource::default());
    }
    
    fn cleanup(&self, app: &mut App) {
        // Optional explicit cleanup
    }
}
```

**Crucible Design:**
```rust
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> PluginMetadata;
    
    fn on_load(&mut self, ctx: &mut PluginContext) -> Result<()>;
    fn on_unload(&mut self, ctx: &mut PluginContext) -> Result<()>;
    
    // Optional lifecycle hooks
    fn on_enable(&mut self, _ctx: &mut PluginContext) -> Result<()> { Ok(()) }
    fn on_disable(&mut self, _ctx: &mut PluginContext) -> Result<()> { Ok(()) }
}

pub struct PluginContext {
    registrations: Vec<Registration>,
}

impl PluginContext {
    // All registrations tracked for automatic cleanup
    pub fn register_event<E: Event>(&mut self, 
        handler: impl EventHandler<E>,
        priority: EventPriority
    ) -> RegistrationHandle {
        let reg = self.event_bus.subscribe(handler, priority);
        self.registrations.push(reg.into());
        reg.handle()
    }
    
    pub fn register_command(&mut self, cmd: Command) -> RegistrationHandle {
        // Commands removed on plugin unload
    }
    
    pub fn register_view(&mut self, view: ViewDescriptor) -> RegistrationHandle {
        // Views destroyed on plugin unload
    }
}

impl Drop for PluginContext {
    fn drop(&mut self) {
        // Automatic cleanup of all registrations
        for reg in self.registrations.drain(..) {
            reg.unregister();
        }
    }
}
```

### 2.2 Lazy Activation

**From VSCode:**
Plugins declare activation events and only load when needed:
```json
{
  "activationEvents": [
    "onLanguage:python",
    "onCommand:extension.sayHello",
    "workspaceContains:.editorconfig"
  ]
}
```

**Crucible Application:**
```rust
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: Version,
    
    // Lazy activation
    pub activation_events: Vec<ActivationEvent>,
}

pub enum ActivationEvent {
    OnStartup,
    OnEvent(EventPattern),
    OnCommand(CommandId),
    OnContentType(ContentTypePattern),
    OnAgentRequest(AgentCapability),
}
```

---

## 3. Communication Patterns

### 3.1 Event Bus Architecture

**From the Event Bus Pattern:**
```
┌──────────────┐     ┌───────────────┐     ┌──────────────┐
│   Plugin A   │────▶│   Event Bus   │────▶│   Plugin B   │
│  (Producer)  │     │               │     │  (Consumer)  │
└──────────────┘     │  ┌─────────┐  │     └──────────────┘
                     │  │ Router  │  │
┌──────────────┐     │  └─────────┘  │     ┌──────────────┐
│   Plugin C   │◀───▶│               │◀───▶│   Core       │
│  (Both)      │     └───────────────┘     │   System     │
└──────────────┘                           └──────────────┘
```

**Core Components:**
- **Event Producers**: Emit events without knowledge of consumers
- **Event Consumers**: Subscribe to specific event types
- **Event Router**: Matches events to subscribers by type and pattern
- **Dead Letter Queue**: Handles undelivered/failed events

**Crucible Implementation:**
```rust
pub struct EventBus {
    subscribers: HashMap<TypeId, Vec<EventSubscription>>,
    interceptors: Vec<Box<dyn EventInterceptor>>,
    dead_letters: VecDeque<DeadLetter>,
}

impl EventBus {
    pub async fn emit<E: Event>(&self, event: E) -> EmitResult {
        let type_id = TypeId::of::<E>();
        
        // Run interceptors (logging, metrics, etc.)
        let event = self.run_interceptors(event)?;
        
        // Get subscribers sorted by priority
        let subs = self.subscribers
            .get(&type_id)
            .map(|s| s.iter().sorted_by_key(|s| s.priority))
            .unwrap_or_default();
        
        for sub in subs {
            match sub.handler.handle(&event).await {
                Ok(EventControl::Continue) => continue,
                Ok(EventControl::Stop) => break,
                Err(e) => self.handle_error(e, &event, sub)?,
            }
        }
        
        Ok(())
    }
}

pub enum EventControl {
    Continue,
    Stop,  // Halt propagation (only for Before events)
}
```

### 3.2 Observers vs Scheduled Systems

**From Bevy:**
Two complementary event patterns:
1. **MessageReader/MessageWriter**: Scheduled systems process events during update cycle
2. **Observers**: Reactive triggers that fire immediately

```rust
// Scheduled approach - runs every frame in Update
fn process_events(mut reader: EventReader<MyEvent>) {
    for event in reader.read() {
        // Handle event
    }
}

// Observer approach - fires immediately when triggered
world.add_observer(|trigger: Trigger<MyEvent>| {
    // Immediate reaction
});
```

**Crucible Hybrid Approach:**
```rust
// Synchronous observers for immediate reactions
pub trait Observer<E: Event>: Send + Sync {
    fn observe(&self, event: &E, ctx: &ObserverContext) -> Result<()>;
}

// Async handlers for background processing
pub trait AsyncHandler<E: Event>: Send + Sync {
    async fn handle(&self, event: E, ctx: &HandlerContext) -> Result<()>;
}

// Register both types
ctx.observe::<ContentModified>(|event, ctx| {
    // Sync: Update in-memory indices immediately
    ctx.update_content_index(event.id)?;
    Ok(())
});

ctx.handle_async::<ContentModified>(|event, ctx| async move {
    // Async: Recompute embeddings in background
    ctx.recompute_embeddings(event.id).await?;
    Ok(())
});
```

---

## 4. Advice/Interception System

### 4.1 Function Wrapping

**From Emacs Advice System:**
```elisp
;; :before - run before original
;; :after - run after original  
;; :around - wrap original, control invocation
;; :override - replace original entirely
;; :filter-args - modify arguments
;; :filter-return - modify return value

(advice-add 'display-buffer :around #'my-tracing-function)
```

**Crucible Command Interception:**
```rust
pub enum AdviceType {
    Before,       // Run before, can cancel
    After,        // Run after, receive result
    Around,       // Full control, receives continuation
    FilterArgs,   // Modify arguments
    FilterResult, // Modify return value
}

pub struct CommandAdvice {
    advice_type: AdviceType,
    priority: i16,
    handler: Box<dyn Fn(&mut CommandContext) -> Result<AdviceControl>>,
}

pub enum AdviceControl {
    Continue,                    // Proceed normally
    Skip,                        // Skip original (Before only)
    ReplaceArgs(Args),          // Use different args
    ReplaceResult(Result),      // Use different result
}

// Usage
ctx.advise_command::<SaveContent>(AdviceType::Before, |cmd_ctx| {
    // Validate content before save
    if !validate(cmd_ctx.args())? {
        return Ok(AdviceControl::Skip);
    }
    Ok(AdviceControl::Continue)
});
```

---

## 5. Contribution Points (Declarative Extension)

### 5.1 Static Declarations

**From VSCode:**
Plugins declare what they contribute via manifest:
```json
{
  "contributes": {
    "commands": [...],
    "keybindings": [...],
    "views": [...],
    "languages": [...],
    "configuration": [...]
  }
}
```

**Crucible Contribution System:**
```rust
pub struct PluginContributions {
    // Commands available in command palette
    pub commands: Vec<CommandContribution>,
    
    // Content types this plugin can handle
    pub content_types: Vec<ContentTypeContribution>,
    
    // Views/panels
    pub views: Vec<ViewContribution>,
    
    // Agent capabilities
    pub agent_capabilities: Vec<AgentCapability>,
    
    // Configuration schema
    pub configuration: Option<ConfigurationSchema>,
    
    // Semantic extractors
    pub extractors: Vec<ExtractorContribution>,
}

#[derive(Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
    pub when: Option<WhenClause>,  // Conditional availability
    pub keybinding: Option<Keybinding>,
}
```

---

## 6. Sandboxing and Capabilities

### 6.1 WASM Plugin Model

**From Extism:**
- Host functions grant specific capabilities to plugins
- Plugins run in isolated WASM sandbox
- Bytes-in/bytes-out interface with serialization

```rust
// Host defines capabilities
pub trait HostCapabilities {
    // Storage access
    fn kv_read(&self, key: &str) -> Option<Vec<u8>>;
    fn kv_write(&self, key: &str, value: &[u8]) -> Result<()>;
    
    // Content access (filtered by permissions)
    fn read_content(&self, id: ContentId) -> Option<Content>;
    fn write_content(&self, id: ContentId, content: Content) -> Result<()>;
    
    // HTTP (if allowed)
    fn http_request(&self, req: HttpRequest) -> Result<HttpResponse>;
}

// Plugin declares required capabilities
pub struct PluginManifest {
    pub required_capabilities: Vec<Capability>,
    pub optional_capabilities: Vec<Capability>,
}

pub enum Capability {
    ReadContent { patterns: Vec<Pattern> },
    WriteContent { patterns: Vec<Pattern> },
    KvStorage { namespace: String },
    HttpAccess { allowed_hosts: Vec<String> },
    AgentExecution { models: Vec<ModelId> },
}
```

### 6.2 Capability Granting

```rust
pub struct PluginSandbox {
    plugin: WasmPlugin,
    granted_capabilities: HashSet<Capability>,
    resource_limits: ResourceLimits,
}

impl PluginSandbox {
    pub fn call<I: Serialize, O: DeserializeOwned>(
        &self,
        function: &str,
        input: I,
    ) -> Result<O> {
        // Validate capability requirements
        // Execute in sandbox with limits
        // Deserialize result
    }
}

pub struct ResourceLimits {
    pub max_memory_bytes: usize,
    pub max_execution_time: Duration,
    pub max_storage_bytes: usize,
}
```

---

## 7. Recommended Architecture for Crucible

### 7.1 Core Event Categories

```rust
// Core content lifecycle
ContentEvents {
    Creating, Created, Reading, Read, 
    Modifying, Modified, Deleting, Deleted,
    Moving, Moved, Linking, Linked,
}

// Knowledge graph operations
GraphEvents {
    NodeAdding, NodeAdded,
    EdgeCreating, EdgeCreated,
    TraversalStarting, TraversalCompleted,
    IndexRebuildStarting, IndexRebuildCompleted,
}

// Semantic/AI operations
SemanticEvents {
    EmbeddingRequested, EmbeddingComputed,
    SimilarityQueryStarting, SimilarityQueryCompleted,
    ClusteringRequested, ClusteringCompleted,
}

// Agent/Orchestration
AgentEvents {
    TaskReceived, TaskStarting, TaskCompleted, TaskFailed,
    ToolInvoked, ToolCompleted,
    ContextUpdated, ConversationCompleted,
}

// CRDT/Sync operations
SyncEvents {
    ChangeReceived, ChangeApplied, ConflictDetected,
    MergeStarting, MergeCompleted,
    PeerConnected, PeerDisconnected,
}

// UI/Interaction (if applicable)
UIEvents {
    ViewOpening, ViewOpened, ViewClosing, ViewClosed,
    SelectionChanged, NavigationRequested,
}
```

### 7.2 Plugin Registration Flow

```
┌─────────────────────────────────────────────────────────────┐
│                      Plugin Manager                          │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  1. Discover     ─▶  Scan plugin directories                │
│                      Parse manifests                         │
│                      Validate dependencies                   │
│                                                              │
│  2. Activate     ─▶  Check activation events                │
│                      Load plugin (lazy or eager)            │
│                      Call on_load()                          │
│                                                              │
│  3. Register     ─▶  Process contributions                  │
│                      Subscribe to events                     │
│                      Register commands                       │
│                      Track all registrations                 │
│                                                              │
│  4. Run          ─▶  Route events to subscribers            │
│                      Execute commands                        │
│                      Handle errors gracefully               │
│                                                              │
│  5. Unload       ─▶  Call on_unload()                       │
│                      Auto-cleanup registrations              │
│                      Release resources                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 7.3 Complete Plugin Example

```rust
pub struct SemanticSearchPlugin {
    config: SemanticConfig,
    index: Option<VectorIndex>,
}

impl Plugin for SemanticSearchPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "crucible.semantic-search".into(),
            name: "Semantic Search".into(),
            version: Version::new(1, 0, 0),
            activation_events: vec![
                ActivationEvent::OnStartup,
            ],
            contributions: PluginContributions {
                commands: vec![
                    CommandContribution {
                        id: "semantic.search".into(),
                        title: "Semantic Search".into(),
                        keybinding: Some("Ctrl+Shift+F".parse()?),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            required_capabilities: vec![
                Capability::ReadContent { patterns: vec!["*".into()] },
            ],
        }
    }
    
    fn on_load(&mut self, ctx: &mut PluginContext) -> Result<()> {
        // Initialize index
        self.index = Some(VectorIndex::new(&self.config)?);
        
        // Register event handlers
        ctx.register_event::<ContentCreated>(
            |event, ctx| self.on_content_created(event, ctx),
            EventPriority::NORMAL,
        );
        
        ctx.register_event::<ContentModified>(
            |event, ctx| self.on_content_modified(event, ctx),
            EventPriority::NORMAL,
        );
        
        // Register async background tasks
        ctx.register_async_handler::<ContentCreated>(
            |event, ctx| async move {
                self.compute_embeddings(event.id).await
            },
        );
        
        // Register command handler
        ctx.register_command_handler("semantic.search", |args, ctx| {
            self.handle_search_command(args, ctx)
        });
        
        Ok(())
    }
    
    fn on_unload(&mut self, _ctx: &mut PluginContext) -> Result<()> {
        // Explicit cleanup if needed (auto-cleanup handles registrations)
        if let Some(index) = self.index.take() {
            index.flush()?;
        }
        Ok(())
    }
}
```

---

## 8. Key Takeaways

### Do:
- **Use lifecycle-aware registration** — All resources tied to plugin lifetime
- **Implement Pre/Post event pairs** — Allow cancellation and notification
- **Support priority ordering** — Predictable, controllable execution order
- **Provide automatic cleanup** — No orphaned handlers or leaked resources
- **Use declarative contributions** — Static manifest plus runtime registration
- **Design for eventual consistency** — Especially with CRDT-based storage
- **Sandbox untrusted plugins** — Capability-based access control

### Don't:
- **Don't use advice when hooks exist** — Prefer explicit extension points
- **Don't require synchronous handling** — Support async for expensive operations
- **Don't couple plugins directly** — All communication through event bus
- **Don't activate eagerly** — Lazy activation improves startup time
- **Don't ignore error handling** — Dead letter queue for failed events

### Crucible-Specific Considerations:

1. **CRDT Integration**: Events should carry CRDT operation metadata for proper merge semantics

2. **Content-Addressable Storage**: Event payloads can reference content by hash, enabling efficient delta sync

3. **Multi-Agent Orchestration**: Events from different agents need proper causality tracking

4. **Semantic Search**: Embedding computation is async; design for eventual index consistency

5. **Tree-based Agent Structure**: Error containment aligns with subtree plugin isolation

---

## References

- Neovim API Documentation: https://neovim.io/doc/user/api.html
- Emacs Hooks Reference: https://www.gnu.org/software/emacs/manual/html_node/elisp/Hooks.html
- Obsidian Plugin API: https://github.com/obsidianmd/obsidian-api
- VSCode Extension API: https://code.visualstudio.com/api
- Bevy ECS: https://docs.rs/bevy_ecs/latest/bevy_ecs/
- Extism: https://extism.org/docs
- Event Sourcing Pattern: https://microservices.io/patterns/data/event-sourcing.html
