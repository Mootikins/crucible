# Plugin API Sketches (MVP)

MVP-level API designs for Crucible's plugin/extension system, based on patterns from Neovim, Emacs, Obsidian, VSCode, and Bevy.

> **Note**: Crucible uses Lua (via mlua) as its primary scripting language, with Fennel as an optional layer.
> See [[Meta/Analysis/Scripting Language Philosophy]] for the rationale.

---

## 1. Basic Hooks on Events

The simplest extension point: subscribe to events, optionally modify or cancel them.

### Rust Host Types

```rust
//! Core event hook types for the plugin system

use serde::{Deserialize, Serialize};

/// Event timing determines modification capability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventTiming {
    Before,  // Can cancel/modify
    After,   // Notification only
}

/// Priority for handler ordering (lower = earlier)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority(pub i16);

impl Priority {
    pub const FIRST: Self = Self(-100);
    pub const EARLY: Self = Self(-50);
    pub const NORMAL: Self = Self(0);
    pub const LATE: Self = Self(50);
    pub const LAST: Self = Self(100);
}

/// Event subscription configuration
#[derive(Debug, Clone)]
pub struct HookConfig {
    pub event: String,
    pub priority: Priority,
    pub pattern: Option<String>,  // Glob pattern for filtering
    pub once: bool,               // Unsubscribe after first call
}

/// Result from a hook handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    #[serde(default)]
    pub cancel: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
}

/// Core events in the system
#[derive(Debug, Clone)]
pub enum CoreEvent {
    // Note lifecycle
    NoteBeforeSave { path: String, content: String },
    NoteSaved { path: String },
    NoteBeforeDelete { path: String },
    NoteDeleted { path: String },
    NoteParsed { path: String, blocks: Vec<Block> },

    // Tool lifecycle
    ToolBefore { name: String, args: serde_json::Value },
    ToolAfter { name: String, result: serde_json::Value },
    ToolError { name: String, error: String },

    // Agent lifecycle
    AgentTaskStarted { agent_id: String, task_id: String },
    AgentTaskCompleted { agent_id: String, result: serde_json::Value },

    // Custom events
    Custom { name: String, payload: serde_json::Value },
}

/// Plugin context provided to handlers
pub trait PluginContext: Send + Sync {
    /// Subscribe to an event
    fn on(&mut self, event: &str, handler: HandlerFn, config: HookConfig) -> SubscriptionId;

    /// Unsubscribe from an event
    fn off(&mut self, id: SubscriptionId);

    /// Emit a custom event
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Get current timestamp
    fn now(&self) -> i64;

    /// Log a message
    fn log(&self, level: &str, message: &str);
}
```

---

## 2. State Machine-Based Flows

For multi-step workflows with defined states and transitions.

### Rust Host Types

```rust
//! State machine flow types

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// A state machine definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDefinition {
    pub id: String,
    pub initial: String,
    pub states: HashMap<String, StateDefinition>,
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Definition of a single state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDefinition {
    /// Transitions: action -> target_state
    #[serde(default)]
    pub on: HashMap<String, String>,

    /// State type: "normal" (default), "final", "parallel"
    #[serde(default)]
    pub r#type: StateType,

    /// Handler called on state entry
    #[serde(default)]
    pub enter: Option<String>,

    /// Handler called on state exit
    #[serde(default)]
    pub exit: Option<String>,

    /// Guards for transitions: action -> guard_fn
    #[serde(default)]
    pub guards: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateType {
    #[default]
    Normal,
    Final,
    Parallel,
}

/// A running flow instance
#[derive(Debug, Clone)]
pub struct FlowInstance {
    pub id: FlowInstanceId,
    pub definition_id: String,
    pub current_state: String,
    pub context: serde_json::Value,
    pub history: Vec<Transition>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A recorded transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub action: String,
    pub timestamp: i64,
    pub data: Option<serde_json::Value>,
}

/// Result of attempting a transition
#[derive(Debug, Clone)]
pub enum TransitionResult {
    Success { new_state: String },
    GuardFailed { guard: String, reason: String },
    InvalidAction { action: String, current_state: String },
    InvalidState { state: String },
}

/// Flow context provided to handlers
pub trait FlowContext: Send + Sync {
    /// Get the current state
    fn current_state(&self) -> &str;

    /// Get/set context data
    fn context(&self) -> &serde_json::Value;
    fn set_context(&mut self, key: &str, value: serde_json::Value);

    /// Get the subject (e.g., note path) this flow is about
    fn subject(&self) -> &str;

    /// Get the user who triggered the current action
    fn current_user(&self) -> &str;

    /// Timestamp helpers
    fn now(&self) -> i64;

    /// Emit events
    fn emit(&self, event: &str, payload: serde_json::Value);
    fn log(&self, level: &str, message: &str);
}

/// Flow manager API
pub trait FlowManager: Send + Sync {
    /// Register a flow definition
    fn register(&mut self, definition: FlowDefinition) -> Result<(), FlowError>;

    /// Start a new flow instance
    fn start(&mut self, definition_id: &str, initial_context: serde_json::Value)
        -> Result<FlowInstanceId, FlowError>;

    /// Send an action to a flow instance
    fn send(&mut self, instance_id: FlowInstanceId, action: &str, data: Option<serde_json::Value>)
        -> Result<TransitionResult, FlowError>;

    /// Get flow instance state
    fn get(&self, instance_id: FlowInstanceId) -> Option<&FlowInstance>;

    /// Query flows by subject or state
    fn query(&self, filter: FlowFilter) -> Vec<&FlowInstance>;
}
```

---

## 3. Context/Concurrency Control for Generation

Control how LLM generation happens: context assembly, concurrent vs sequential, streaming.

### Rust Host Types

```rust
//! Generation control types

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// A generation pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    pub id: String,
    pub context: ContextConfig,
    pub strategy: GenerationStrategy,
    #[serde(default)]
    pub streaming: StreamingConfig,
}

/// Configuration for context assembly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextConfig {
    /// Static system prompt
    #[serde(default)]
    pub system: Option<String>,

    /// Dynamic context sources
    #[serde(default)]
    pub sources: Vec<ContextSource>,

    /// Maximum tokens for assembled context
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Priority map for truncation
    #[serde(default)]
    pub priorities: HashMap<String, i32>,
}

fn default_max_tokens() -> usize { 8000 }

/// A source of context data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextSource {
    SemanticSearch { query: String, limit: usize },
    RecentNotes { folder: Option<String>, limit: usize },
    LinkedNotes { from: String, depth: usize },
    StaticText { content: String },
    Custom { handler: String, config: serde_json::Value },
}

/// Generation strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GenerationStrategy {
    /// Single generation
    Single {
        prompt: String,
        #[serde(default)]
        model: Option<String>,
    },

    /// Sequential steps
    Sequential {
        steps: Vec<GenerationStep>,
    },

    /// Parallel generations, merged
    Parallel {
        branches: Vec<GenerationStep>,
        merge: MergeStrategy,
    },

    /// Chain with dependencies
    Chain {
        steps: Vec<ChainStep>,
    },
}

/// A single generation step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationStep {
    pub id: String,
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
}

/// A step in a chain (can reference previous results)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStep {
    pub id: String,
    pub prompt: String,  // Can include ${previous_step.result}
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub parallel_on: Option<String>,  // Split and parallelize on this field
    #[serde(default)]
    pub depends_on: Vec<String>,  // Explicit dependencies
}

/// How to merge parallel results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    Concatenate { separator: String },
    PickBest { criteria: String },
    Summarize { prompt: String },
}

/// Streaming configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub chunk_handler: Option<String>,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
}

fn default_buffer_size() -> usize { 100 }

/// Assembled context ready for generation
#[derive(Debug, Clone)]
pub struct AssembledContext {
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub total_tokens: usize,
    pub sources_used: Vec<String>,
    pub truncated: bool,
}

/// Result of a pipeline run
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub pipeline_id: String,
    pub steps: HashMap<String, StepResult>,
    pub final_result: String,
    pub total_tokens_used: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub id: String,
    pub result: String,
    pub tokens_used: usize,
    pub duration_ms: u64,
}

/// Streaming chunk
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub step_id: String,
    pub content: String,
    pub is_final: bool,
}

/// Generation context for handlers
#[async_trait]
pub trait GenerationContext: Send + Sync {
    /// Get a variable from the pipeline context
    fn get(&self, key: &str) -> Option<serde_json::Value>;

    /// Set a variable
    fn set(&mut self, key: &str, value: serde_json::Value);

    /// Get current step ID
    fn current_step(&self) -> &str;

    /// Emit an event
    fn emit(&self, event: &str, payload: serde_json::Value);

    /// Count tokens in text
    fn count_tokens(&self, text: &str) -> usize;
}

/// Generation manager API
#[async_trait]
pub trait GenerationManager: Send + Sync {
    /// Register a pipeline definition
    fn register(&mut self, definition: PipelineDefinition) -> Result<(), GenerationError>;

    /// Run a pipeline
    async fn run(
        &self,
        pipeline_id: &str,
        variables: serde_json::Value
    ) -> Result<PipelineResult, GenerationError>;

    /// Run with streaming
    async fn run_streaming(
        &self,
        pipeline_id: &str,
        variables: serde_json::Value,
    ) -> Result<mpsc::Receiver<StreamChunk>, GenerationError>;

    /// Simple completion (no pipeline)
    async fn complete(
        &self,
        prompt: &str,
        options: CompletionOptions,
    ) -> Result<String, GenerationError>;

    /// Assemble context without generating
    async fn assemble_context(
        &self,
        config: &ContextConfig,
        variables: serde_json::Value,
    ) -> Result<AssembledContext, GenerationError>;
}

#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    pub model: Option<String>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub include_context: bool,
    pub context_sources: Vec<String>,
    pub stream: bool,
}
```

---

## 4. Lua Implementation

Lua (via mlua with Luau mode) offers gradual typing with union types, generics, and excellent Rust embedding. These sketches show the complete plugin API.

### 4.1 Basic Hooks on Events (Lua)

```lua
--!strict

-- Type definitions for the event system

export type Priority = number
export type SubscriptionId = string

export type HookConfig = {
    event: string,
    priority: Priority?,
    pattern: string?,
    once: boolean?,
}

-- Tagged union for hook results
export type HookResult =
    | { type: "continue" }
    | { type: "continue_modified", payload: any }
    | { type: "cancel", reason: string? }

-- Core event types as tagged union
export type CoreEvent =
    | { type: "note:before_save", path: string, content: string }
    | { type: "note:saved", path: string }
    | { type: "note:before_delete", path: string }
    | { type: "note:deleted", path: string }
    | { type: "tool:before", name: string, args: {[string]: any} }
    | { type: "tool:after", name: string, result: any }
    | { type: "tool:error", name: string, error: string }
    | { type: "custom", name: string, payload: any }

-- Handler function signature
export type EventHandler = (event: CoreEvent, ctx: PluginContext) -> HookResult

-- Plugin context API
export type PluginContext = {
    on: (self: PluginContext, event: string, handler: EventHandler, config: HookConfig?) -> SubscriptionId,
    off: (self: PluginContext, id: SubscriptionId) -> (),
    emit: (self: PluginContext, event: string, payload: any) -> (),
    now: (self: PluginContext) -> number,
    log: (self: PluginContext, level: "debug" | "info" | "warn" | "error", message: string) -> (),
}

-- Plugin module
local Plugin = {}

--- Plugin initialization - called once on load
function Plugin.init(ctx: PluginContext): ()
    -- Subscribe to events with priority
    ctx:on("note:before_save", Plugin.onBeforeSave, { priority = -50 })
    ctx:on("note:saved", Plugin.onSaved, { priority = 0 })
    ctx:on("tool:after", Plugin.onToolResult, { pattern = "search_*" })
end

--- Before-save hook: can modify or cancel
function Plugin.onBeforeSave(event: CoreEvent, ctx: PluginContext): HookResult
    if event.type ~= "note:before_save" then
        return { type = "continue" }
    end

    -- Validation: cancel if invalid
    if #event.content == 0 then
        return { type = "cancel", reason = "Empty note not allowed" }
    end

    -- Modification: add timestamp to content
    local modified_content = event.content .. "\n<!-- modified: " .. tostring(ctx:now()) .. " -->"

    return {
        type = "continue_modified",
        payload = { path = event.path, content = modified_content }
    }
end

--- After-save hook: notification only
function Plugin.onSaved(event: CoreEvent, ctx: PluginContext): HookResult
    if event.type == "note:saved" then
        ctx:log("info", "Saved: " .. event.path)
    end
    return { type = "continue" }
end

--- Pattern-matched hook
function Plugin.onToolResult(event: CoreEvent, ctx: PluginContext): HookResult
    if event.type == "tool:after" then
        local results = event.result.results or {}
        ctx:emit("custom:search_completed", { count = #results })
    end
    return { type = "continue" }
end

return Plugin
```

### 4.2 State Machine-Based Flows (Lua)

```lua
--!strict

-- Type definitions for state machine flows

export type StateType = "normal" | "final" | "parallel"

export type StateDefinition = {
    on: {[string]: string}?,           -- action -> target_state
    type: StateType?,
    enter: string?,                     -- handler function name
    exit: string?,                      -- handler function name
    guards: {[string]: string}?,        -- action -> guard function name
}

export type FlowDefinition = {
    id: string,
    initial: string,
    states: {[string]: StateDefinition},
    context: {[string]: any}?,
}

export type Transition = {
    from: string,
    to: string,
    action: string,
    timestamp: number,
    data: any?,
}

export type TransitionResult =
    | { type: "success", newState: string }
    | { type: "guard_failed", guard: string, reason: string }
    | { type: "invalid_action", action: string, currentState: string }
    | { type: "invalid_state", state: string }

export type FlowInstance = {
    id: string,
    definitionId: string,
    currentState: string,
    context: {[string]: any},
    history: {Transition},
    createdAt: number,
    updatedAt: number,
}

-- Flow context for handlers
export type FlowContext = {
    currentState: (self: FlowContext) -> string,
    context: (self: FlowContext) -> {[string]: any},
    setContext: (self: FlowContext, key: string, value: any) -> (),
    subject: (self: FlowContext) -> string,
    currentUser: (self: FlowContext) -> string,
    now: (self: FlowContext) -> number,
    emit: (self: FlowContext, event: string, payload: any) -> (),
    log: (self: FlowContext, level: string, message: string) -> (),
}

-- Guard function signature
export type GuardFn = (ctx: FlowContext, event: {[string]: any}) -> boolean

-- State handler signature
export type StateHandler = (ctx: FlowContext, event: {[string]: any}) -> ()

-- Flow manager API
export type FlowManager = {
    register: (self: FlowManager, definition: FlowDefinition) -> (),
    start: (self: FlowManager, definitionId: string, initialContext: {[string]: any}) -> string,
    send: (self: FlowManager, instanceId: string, action: string, data: any?) -> TransitionResult,
    get: (self: FlowManager, instanceId: string) -> FlowInstance?,
}

-- Plugin module
local ReviewWorkflow = {}

-- Guard: check if user can approve
function ReviewWorkflow.canApprove(ctx: FlowContext, event: {[string]: any}): boolean
    local flowCtx = ctx:context()
    -- Can't approve your own submission
    return flowCtx.submitter ~= ctx:currentUser()
end

-- State entry handlers
function ReviewWorkflow.onEnterDraft(ctx: FlowContext, event: {[string]: any}): ()
    ctx:setContext("comments", {})
    ctx:log("info", "Entered draft state")
end

function ReviewWorkflow.onEnterPending(ctx: FlowContext, event: {[string]: any}): ()
    ctx:setContext("submitted_at", ctx:now())
    ctx:emit("notification:review_requested", {
        note = ctx:subject(),
        submitter = ctx:currentUser(),
    })
end

function ReviewWorkflow.onApproved(ctx: FlowContext, event: {[string]: any}): ()
    ctx:setContext("approved_at", ctx:now())
    ctx:setContext("approver", event.user)

    ctx:emit("workflow:completed", {
        workflow = "review",
        result = "approved",
        note = ctx:subject(),
    })
end

--- Define the workflow
function ReviewWorkflow.define(): FlowDefinition
    return {
        id = "review_workflow",
        initial = "draft",

        states = {
            draft = {
                on = {
                    submit = "pending_review",
                    discard = "discarded",
                },
                enter = "onEnterDraft",
            },
            pending_review = {
                on = {
                    approve = "approved",
                    reject = "draft",
                    request_changes = "needs_changes",
                },
                enter = "onEnterPending",
                guards = {
                    approve = "canApprove",
                },
            },
            needs_changes = {
                on = {
                    resubmit = "pending_review",
                    discard = "discarded",
                },
            },
            approved = {
                type = "final",
                enter = "onApproved",
            },
            discarded = {
                type = "final",
            },
        },

        context = {
            reviewer = nil,
            comments = {},
            history = {},
        },
    }
end

return ReviewWorkflow
```

### 4.3 Generation Control (Lua)

```lua
--!strict

-- Type definitions for generation control

export type ContextSourceType = "semantic_search" | "recent_notes" | "linked_notes" | "static_text" | "custom"

export type ContextSource =
    | { type: "semantic_search", query: string, limit: number }
    | { type: "recent_notes", folder: string?, limit: number }
    | { type: "linked_notes", from: string, depth: number }
    | { type: "static_text", content: string }
    | { type: "custom", handler: string, config: {[string]: any} }

export type ContextConfig = {
    system: string?,
    sources: {ContextSource},
    maxTokens: number?,
    priorities: {[string]: number}?,
}

export type GenerationStep = {
    id: string,
    prompt: string,
    model: string?,
    maxTokens: number?,
}

export type ChainStep = {
    id: string,
    prompt: string,
    model: string?,
    parallelOn: string?,
    dependsOn: {string}?,
}

export type MergeStrategy =
    | { type: "concatenate", separator: string }
    | { type: "pick_best", criteria: string }
    | { type: "summarize", prompt: string }

export type GenerationStrategy =
    | { type: "single", prompt: string, model: string? }
    | { type: "sequential", steps: {GenerationStep} }
    | { type: "parallel", branches: {GenerationStep}, merge: MergeStrategy }
    | { type: "chain", steps: {ChainStep} }

export type StreamingConfig = {
    enabled: boolean,
    chunkHandler: string?,
    bufferSize: number?,
}

export type PipelineDefinition = {
    id: string,
    context: ContextConfig,
    strategy: GenerationStrategy,
    streaming: StreamingConfig?,
}

export type StepResult = {
    id: string,
    result: string,
    tokensUsed: number,
    durationMs: number,
}

export type PipelineResult = {
    pipelineId: string,
    steps: {[string]: StepResult},
    finalResult: string,
    totalTokensUsed: number,
    durationMs: number,
}

export type StreamChunk = {
    stepId: string,
    content: string,
    isFinal: boolean,
}

-- Generation context for handlers
export type GenerationContext = {
    get: (self: GenerationContext, key: string) -> any?,
    set: (self: GenerationContext, key: string, value: any) -> (),
    currentStep: (self: GenerationContext) -> string,
    emit: (self: GenerationContext, event: string, payload: any) -> (),
    countTokens: (self: GenerationContext, text: string) -> number,
}

-- Completion options
export type CompletionOptions = {
    model: string?,
    maxTokens: number?,
    temperature: number?,
    includeContext: boolean?,
    contextSources: {string}?,
    stream: boolean?,
}

-- Generation manager API
export type GenerationManager = {
    register: (self: GenerationManager, definition: PipelineDefinition) -> (),
    run: (self: GenerationManager, pipelineId: string, variables: {[string]: any}) -> PipelineResult,
    complete: (self: GenerationManager, prompt: string, options: CompletionOptions?) -> string,
}

-- Plugin module
local ResearchPipeline = {}

--- Handle streaming chunks
function ResearchPipeline.onChunk(chunk: string, ctx: GenerationContext): ()
    local inCodeBlock = ctx:get("in_code_block") or false

    if string.find(chunk, "```") then
        ctx:set("in_code_block", not inCodeBlock)
    end

    ctx:emit("generation:chunk", {
        step = ctx:currentStep(),
        content = chunk,
    })
end

--- Define the research pipeline
function ResearchPipeline.define(): PipelineDefinition
    return {
        id = "research_pipeline",

        context = {
            system = "You are a research assistant...",

            sources = {
                { type = "semantic_search", query = "${user_query}", limit = 5 },
                { type = "recent_notes", folder = "Research", limit = 3 },
                { type = "linked_notes", from = "${current_note}", depth = 1 },
            },

            maxTokens = 8000,

            priorities = {
                system = 100,
                user_message = 90,
                semantic_search = 70,
                recent_notes = 50,
                linked_notes = 30,
            },
        },

        strategy = {
            type = "chain",
            steps = {
                {
                    id = "outline",
                    prompt = "Create an outline for: ${user_query}",
                    model = "fast",
                },
                {
                    id = "expand",
                    prompt = "Expand this outline:\n${outline.result}",
                    model = "capable",
                    parallelOn = "outline.sections",
                },
                {
                    id = "synthesize",
                    prompt = "Synthesize these sections:\n${expand.results}",
                    model = "capable",
                },
            },
        },

        streaming = {
            enabled = true,
            chunkHandler = "onChunk",
            bufferSize = 100,
        },
    }
end

--- Simpler API for basic generation
function ResearchPipeline.simpleGenerate(
    prompt: string,
    options: CompletionOptions?,
    manager: GenerationManager
): string
    local opts: CompletionOptions = options or {}

    return manager:complete(prompt, {
        model = opts.model or "default",
        maxTokens = opts.maxTokens or 1000,
        temperature = opts.temperature or 0.7,
        includeContext = if opts.includeContext ~= nil then opts.includeContext else true,
        contextSources = opts.contextSources or {"semantic_search"},
        stream = opts.stream or false,
    })
end

return ResearchPipeline
```

### 4.4 Rust Host for Lua (mlua)

```rust
//! Lua plugin host using mlua
//!
//! Demonstrates exposing the plugin APIs to Lua scripts

use mlua::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin context exposed to Lua
struct LuaPluginContext {
    handlers: HashMap<String, Vec<LuaRegistryKey>>,
}

impl LuaPluginContext {
    fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl UserData for LuaPluginContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Subscribe to events
        methods.add_method_mut("on", |lua, this,
            (event, handler, config): (String, LuaFunction, Option<LuaTable>)
        | -> LuaResult<String> {
            let key = lua.create_registry_value(handler)?;
            let id = format!("sub_{}", this.handlers.len());

            this.handlers
                .entry(event)
                .or_default()
                .push(key);

            Ok(id)
        });

        // Unsubscribe
        methods.add_method_mut("off", |_lua, this, id: String| -> LuaResult<()> {
            // Implementation would remove by ID
            Ok(())
        });

        // Emit custom event
        methods.add_method("emit", |_lua, _this,
            (event, payload): (String, LuaValue)
        | -> LuaResult<()> {
            log::debug!("Event emitted: {} with {:?}", event, payload);
            Ok(())
        });

        // Get timestamp
        methods.add_method("now", |_lua, _this, ()| -> LuaResult<i64> {
            Ok(chrono::Utc::now().timestamp_millis())
        });

        // Log message
        methods.add_method("log", |_lua, _this,
            (level, message): (String, String)
        | -> LuaResult<()> {
            match level.as_str() {
                "debug" => log::debug!("[lua] {}", message),
                "info" => log::info!("[lua] {}", message),
                "warn" => log::warn!("[lua] {}", message),
                "error" => log::error!("[lua] {}", message),
                _ => log::info!("[lua] {}", message),
            }
            Ok(())
        });
    }
}

/// Flow context for state machine handlers
struct LuaFlowContext {
    current_state: String,
    context: HashMap<String, serde_json::Value>,
    subject: String,
    current_user: String,
}

impl UserData for LuaFlowContext {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("currentState", |_lua, this, ()| -> LuaResult<String> {
            Ok(this.current_state.clone())
        });

        methods.add_method("context", |lua, this, ()| -> LuaResult<LuaValue> {
            lua.to_value(&this.context)
        });

        methods.add_method_mut("setContext", |_lua, this,
            (key, value): (String, LuaValue)
        | -> LuaResult<()> {
            // Convert LuaValue to serde_json::Value
            let json_value = serde_json::to_value(&value).unwrap_or(serde_json::Value::Null);
            this.context.insert(key, json_value);
            Ok(())
        });

        methods.add_method("subject", |_lua, this, ()| -> LuaResult<String> {
            Ok(this.subject.clone())
        });

        methods.add_method("currentUser", |_lua, this, ()| -> LuaResult<String> {
            Ok(this.current_user.clone())
        });

        methods.add_method("now", |_lua, _this, ()| -> LuaResult<i64> {
            Ok(chrono::Utc::now().timestamp_millis())
        });

        methods.add_method("emit", |_lua, _this,
            (event, payload): (String, LuaValue)
        | -> LuaResult<()> {
            log::debug!("Flow event: {} with {:?}", event, payload);
            Ok(())
        });

        methods.add_method("log", |_lua, _this,
            (level, message): (String, String)
        | -> LuaResult<()> {
            log::info!("[flow:{}] {}", level, message);
            Ok(())
        });
    }
}

/// Load and initialize a Lua plugin
pub fn load_lua_plugin(script_path: &str) -> LuaResult<()> {
    let lua = Lua::new();

    // Enable sandbox mode for untrusted plugins
    lua.sandbox(true)?;

    // Create and expose plugin context
    let ctx = LuaPluginContext::new();
    lua.globals().set("PluginContext", ctx)?;

    // Load and execute the plugin
    let script = std::fs::read_to_string(script_path)?;
    let plugin_module: LuaTable = lua.load(&script).eval()?;

    // Call init if present
    if let Ok(init_fn) = plugin_module.get::<LuaFunction>("init") {
        let ctx: LuaAnyUserData = lua.globals().get("PluginContext")?;
        init_fn.call::<()>(ctx)?;
    }

    Ok(())
}

/// Execute plugin handlers for an event
pub fn dispatch_event(
    lua: &Lua,
    event_type: &str,
    payload: serde_json::Value,
) -> LuaResult<Option<serde_json::Value>> {
    let ctx: LuaAnyUserData = lua.globals().get("PluginContext")?;
    let ctx_ref = ctx.borrow::<LuaPluginContext>()?;

    let event_table = lua.create_table()?;
    event_table.set("type", event_type)?;

    // Convert payload to Lua
    let lua_payload = lua.to_value(&payload)?;
    for (k, v) in lua_payload.as_table().unwrap().pairs::<String, LuaValue>() {
        let (key, val) = (k?, v?);
        event_table.set(key, val)?;
    }

    if let Some(handlers) = ctx_ref.handlers.get(event_type) {
        for handler_key in handlers {
            let handler: LuaFunction = lua.registry_value(handler_key)?;
            let result: LuaTable = handler.call((event_table.clone(), ctx.clone()))?;

            // Check result type
            let result_type: String = result.get("type")?;
            match result_type.as_str() {
                "cancel" => {
                    let reason: Option<String> = result.get("reason").ok();
                    log::info!("Event cancelled: {:?}", reason);
                    return Ok(None);
                }
                "continue_modified" => {
                    let modified: LuaValue = result.get("payload")?;
                    return Ok(Some(serde_json::to_value(&modified)?));
                }
                "continue" => continue,
                _ => continue,
            }
        }
    }

    Ok(Some(payload))
}
```

---

## Summary: Three Plugin Patterns

| Pattern | Use Case | Key Features |
|---------|----------|--------------|
| **Event Hooks** | React to system events | Priority ordering, before/after, cancel/modify |
| **State Machines** | Multi-step workflows | Defined states, guards, history tracking |
| **Generation Control** | LLM orchestration | Context assembly, parallel/chain execution |

All three share:
- Plugin lifecycle management (init/cleanup)
- Event emission for cross-plugin communication
- Async-friendly design
- Type-safe Rust host with flexible Lua scripting

### Lua Features

| Feature | Lua (via mlua) |
|---------|----------------|
| **Type System** | Gradual (opt-in strict with Luau) |
| **Union Types** | Full support with tagged unions |
| **Generics** | Yes with inference |
| **Async** | Via mlua + Rust executor |
| **Sandboxing** | VM-level + memory/CPU limits |
| **Learning Curve** | Low (familiar syntax) |
| **Embedding** | mlua (excellent) |
| **IDE Support** | LSP + type checker |
| **LLM Writability** | Excellent (massive training data) |
