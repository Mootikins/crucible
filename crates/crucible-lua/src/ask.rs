//! Ask module for Lua scripts
//!
//! Provides batched user questions with choices for Lua and Fennel scripts,
//! along with response types for handling user answers.
//!
//! ## Types
//!
//! - [`LuaAskQuestion`] - A single question with choices
//! - [`LuaAskBatch`] - A batch of questions to ask together
//! - [`LuaQuestionAnswer`] - A single question's answer
//! - [`LuaAskBatchResponse`] - Response to a batch of questions
//!
//! ## Lua Usage
//!
//! ```lua
//! local ask = require("ask")
//!
//! -- Create a question with choices
//! local q = ask.question("Library", "Which library?")
//!     :choice("Tokio")
//!     :choice("async-std")
//!
//! -- Create batch and add questions
//! local batch = ask.batch()
//!     :question(q)
//!     :question(ask.question("DB", "Which database?")
//!         :choices({"Postgres", "SQLite", "MongoDB"}))
//!
//! -- Questions can also enable multi-select
//! local features = ask.question("Features", "Select features")
//!     :choices({"Auth", "Logging", "Caching"})
//!     :multi_select()
//!
//! -- Create answers for testing/mocking
//! local answer = ask.answer({0, 2})       -- selected indices
//! local other = ask.answer_other("Custom") -- free-text input
//!
//! -- Log notifications
//! ask.notify("Processing complete!")
//! ```
//!
//! ## Working with Responses
//!
//! ```lua
//! -- Example: processing a batch response
//! local function process_response(response)
//!     if response:is_cancelled() then
//!         print("User cancelled")
//!         return
//!     end
//!
//!     for i, answer in ipairs(response:answers()) do
//!         if answer:has_other() then
//!             print("Custom: " .. answer:other())
//!         else
//!             print("Selected: " .. answer:first_selected())
//!         end
//!     end
//! end
//! ```
//!
//! ## Fennel Usage
//!
//! ```fennel
//! (local ask (require :ask))
//!
//! ;; Create a question
//! (local q (-> (ask.question "Library" "Which library?")
//!              (: :choice "Tokio")
//!              (: :choice "async-std")))
//!
//! ;; Create batch
//! (local batch (-> (ask.batch)
//!                  (: :question q)))
//!
//! ;; Create an answer
//! (local answer (ask.answer [0 1]))
//! ```

use crate::error::LuaError;
use crucible_core::interaction::{AskBatch, AskBatchResponse, AskQuestion, QuestionAnswer};
use crucible_core::traits::completion_backend::{BackendCompletionRequest, CompletionBackend};
use crucible_core::traits::context_ops::ContextMessage;
use crucible_core::uuid;
use mlua::{FromLua, Lua, MetaMethod, Result as LuaResult, Table, UserData, UserDataMethods, Value};

/// Lua wrapper for AskQuestion with chainable methods
#[derive(Debug, Clone)]
pub struct LuaAskQuestion {
    pub inner: AskQuestion,
}

impl LuaAskQuestion {
    /// Create a new question with header and question text
    pub fn new(header: String, question: String) -> Self {
        Self {
            inner: AskQuestion::new(header, question),
        }
    }
}

impl FromLua for LuaAskQuestion {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaAskQuestion>().map(|q| q.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaAskQuestion".to_string(),
                message: Some("expected AskQuestion userdata".to_string()),
            }),
        }
    }
}

impl UserData for LuaAskQuestion {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Add a single choice (chainable)
        methods.add_method("choice", |_, this, label: String| {
            let mut new = this.clone();
            new.inner.choices.push(label);
            Ok(new)
        });

        // Add multiple choices at once (chainable)
        methods.add_method("choices", |_, this, labels: Vec<String>| {
            let mut new = this.clone();
            new.inner.choices.extend(labels);
            Ok(new)
        });

        // Enable multi-select mode (chainable)
        // Can be called as .multi_select() or .multi_select(true/false)
        methods.add_method("multi_select", |_, this, enabled: Option<bool>| {
            let mut new = this.clone();
            new.inner.multi_select = enabled.unwrap_or(true);
            Ok(new)
        });

        // Accessors for reading fields
        methods.add_method("header", |_, this, ()| Ok(this.inner.header.clone()));

        methods.add_method("question_text", |_, this, ()| {
            Ok(this.inner.question.clone())
        });

        methods.add_method("get_choices", |_, this, ()| Ok(this.inner.choices.clone()));

        methods.add_method("is_multi_select", |_, this, ()| Ok(this.inner.multi_select));

        // String representation for debugging
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "AskQuestion {{ header: \"{}\", question: \"{}\", choices: {}, multi_select: {} }}",
                this.inner.header,
                this.inner.question,
                this.inner.choices.len(),
                this.inner.multi_select
            ))
        });
    }
}

/// Lua wrapper for AskBatch with chainable methods
#[derive(Debug, Clone, Default)]
pub struct LuaAskBatch {
    pub inner: AskBatch,
}

impl LuaAskBatch {
    /// Create a new empty batch
    pub fn new() -> Self {
        Self {
            inner: AskBatch::new(),
        }
    }
}

impl FromLua for LuaAskBatch {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaAskBatch>().map(|b| b.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaAskBatch".to_string(),
                message: Some("expected AskBatch userdata".to_string()),
            }),
        }
    }
}

impl UserData for LuaAskBatch {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Add a question to the batch (chainable)
        methods.add_method("question", |_, this, q: LuaAskQuestion| {
            let mut new = this.clone();
            new.inner.questions.push(q.inner);
            Ok(new)
        });

        // Accessors
        methods.add_method("id", |_, this, ()| Ok(this.inner.id.to_string()));

        methods.add_method("question_count", |_, this, ()| {
            Ok(this.inner.questions.len())
        });

        // String representation for debugging
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "AskBatch {{ id: {}, questions: {} }}",
                this.inner.id,
                this.inner.questions.len()
            ))
        });
    }
}

/// Lua wrapper for QuestionAnswer - a single question's answer
#[derive(Debug, Clone)]
pub struct LuaQuestionAnswer {
    pub inner: QuestionAnswer,
}

impl LuaQuestionAnswer {
    /// Create a new answer with selected indices
    pub fn new(selected: Vec<usize>) -> Self {
        Self {
            inner: QuestionAnswer {
                selected,
                other: None,
            },
        }
    }

    /// Create an answer with free-text "other" input
    pub fn with_other(text: String) -> Self {
        Self {
            inner: QuestionAnswer {
                selected: Vec::new(),
                other: Some(text),
            },
        }
    }
}

impl FromLua for LuaQuestionAnswer {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaQuestionAnswer>().map(|a| a.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaQuestionAnswer".to_string(),
                message: Some("expected QuestionAnswer userdata".to_string()),
            }),
        }
    }
}

impl UserData for LuaQuestionAnswer {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Get selected indices as a table
        methods.add_method("selected", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, idx) in this.inner.selected.iter().enumerate() {
                table.set(i + 1, *idx)?;
            }
            Ok(table)
        });

        // Get the "other" free-text value (if any)
        methods.add_method("other", |_, this, ()| Ok(this.inner.other.clone()));

        // Check if "other" was used
        methods.add_method("has_other", |_, this, ()| Ok(this.inner.other.is_some()));

        // Get the first selected index (convenience for single-select questions)
        methods.add_method("first_selected", |_, this, ()| {
            Ok(this.inner.selected.first().copied())
        });

        // Check if any choice was selected
        methods.add_method("has_selection", |_, this, ()| {
            Ok(!this.inner.selected.is_empty())
        });

        // Get count of selected items
        methods.add_method("selection_count", |_, this, ()| {
            Ok(this.inner.selected.len())
        });

        // String representation for debugging
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            if let Some(other) = &this.inner.other {
                Ok(format!("QuestionAnswer {{ other: \"{}\" }}", other))
            } else {
                Ok(format!(
                    "QuestionAnswer {{ selected: {:?} }}",
                    this.inner.selected
                ))
            }
        });
    }
}

/// Lua wrapper for AskBatchResponse - response to a batch of questions
#[derive(Debug, Clone)]
pub struct LuaAskBatchResponse {
    pub inner: AskBatchResponse,
}

impl LuaAskBatchResponse {
    /// Create a new response for a request ID
    pub fn new(id: uuid::Uuid) -> Self {
        Self {
            inner: AskBatchResponse::new(id),
        }
    }

    /// Create a cancelled response
    pub fn cancelled(id: uuid::Uuid) -> Self {
        Self {
            inner: AskBatchResponse::cancelled(id),
        }
    }
}

impl FromLua for LuaAskBatchResponse {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaAskBatchResponse>().map(|r| r.clone()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaAskBatchResponse".to_string(),
                message: Some("expected AskBatchResponse userdata".to_string()),
            }),
        }
    }
}

impl UserData for LuaAskBatchResponse {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Get the request ID this responds to
        methods.add_method("id", |_, this, ()| Ok(this.inner.id.to_string()));

        // Check if the user cancelled the interaction
        methods.add_method("is_cancelled", |_, this, ()| Ok(this.inner.cancelled));

        // Get the number of answers
        methods.add_method("answer_count", |_, this, ()| Ok(this.inner.answers.len()));

        // Get a specific answer by index (1-based for Lua convention)
        methods.add_method("answer", |_, this, index: usize| {
            // Convert from 1-based Lua index to 0-based
            let idx = index.checked_sub(1).ok_or_else(|| {
                mlua::Error::runtime("Answer index must be >= 1")
            })?;
            this.inner
                .answers
                .get(idx)
                .cloned()
                .map(|a| LuaQuestionAnswer { inner: a })
                .ok_or_else(|| mlua::Error::runtime("Answer index out of bounds"))
        });

        // Get all answers as a table
        methods.add_method("answers", |lua, this, ()| {
            let table = lua.create_table()?;
            for (i, answer) in this.inner.answers.iter().enumerate() {
                table.set(i + 1, LuaQuestionAnswer { inner: answer.clone() })?;
            }
            Ok(table)
        });

        // Check if there are any answers
        methods.add_method("has_answers", |_, this, ()| {
            Ok(!this.inner.answers.is_empty())
        });

        // String representation for debugging
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "AskBatchResponse {{ id: {}, answers: {}, cancelled: {} }}",
                this.inner.id,
                this.inner.answers.len(),
                this.inner.cancelled
            ))
        });
    }
}

/// Register the ask module with a Lua state
pub fn register_ask_module(lua: &Lua) -> Result<(), LuaError> {
    let ask = lua.create_table()?;

    // ask.question(header, text) -> LuaAskQuestion
    let question_fn = lua.create_function(|_, (header, text): (String, String)| {
        Ok(LuaAskQuestion::new(header, text))
    })?;
    ask.set("question", question_fn)?;

    // ask.batch() -> LuaAskBatch
    let batch_fn = lua.create_function(|_, ()| Ok(LuaAskBatch::new()))?;
    ask.set("batch", batch_fn)?;

    // ask.notify(message) - log a notification message
    let notify_fn = lua.create_function(|_, message: String| {
        tracing::info!(target: "lua_notify", "{}", message);
        Ok(())
    })?;
    ask.set("notify", notify_fn)?;

    // ask.answer(selected) -> LuaQuestionAnswer
    // Create an answer with selected indices
    let answer_fn = lua.create_function(|_, selected: Vec<usize>| {
        Ok(LuaQuestionAnswer::new(selected))
    })?;
    ask.set("answer", answer_fn)?;

    // ask.answer_other(text) -> LuaQuestionAnswer
    // Create an answer with free-text "other" input
    let answer_other_fn = lua.create_function(|_, text: String| {
        Ok(LuaQuestionAnswer::with_other(text))
    })?;
    ask.set("answer_other", answer_other_fn)?;

    // Register as global module
    lua.globals().set("ask", ask)?;

    Ok(())
}

/// Convert a LuaAskQuestion to a crucible_core AskQuestion
pub fn lua_question_to_core(question: &LuaAskQuestion) -> AskQuestion {
    question.inner.clone()
}

/// Convert a LuaAskBatch to a crucible_core AskBatch
pub fn lua_batch_to_core(batch: &LuaAskBatch) -> AskBatch {
    batch.inner.clone()
}

/// Convert a Lua table representation of an AskQuestion to core type
///
/// This supports Lua tables with the structure:
/// ```lua
/// { header = "...", question = "...", choices = {...}, multi_select = bool }
/// ```
pub fn lua_question_table_to_core(table: &Table) -> LuaResult<AskQuestion> {
    let header: String = table.get("header")?;
    let question: String = table.get("question")?;
    let multi_select: bool = table.get("multi_select").unwrap_or(false);

    let mut choices = Vec::new();
    if let Ok(choices_table) = table.get::<Table>("choices") {
        for pair in choices_table.pairs::<i64, String>() {
            let (_, choice) = pair?;
            choices.push(choice);
        }
    }

    let mut q = AskQuestion::new(header, question);
    q.choices = choices;
    q.multi_select = multi_select;
    Ok(q)
}

/// Convert a Lua table representation of an AskBatch to core type
///
/// This supports Lua tables with the structure:
/// ```lua
/// { id = "uuid", questions = { ... } }
/// ```
pub fn lua_batch_table_to_core(table: &Table) -> LuaResult<AskBatch> {
    let mut batch = AskBatch::new();

    // Parse UUID if provided (otherwise use the generated one)
    if let Ok(id_str) = table.get::<String>("id") {
        if let Ok(id) = uuid::Uuid::parse_str(&id_str) {
            batch.id = id;
        }
    }

    if let Ok(questions_table) = table.get::<Table>("questions") {
        for pair in questions_table.pairs::<i64, Value>() {
            let (_, value) = pair?;
            match value {
                Value::UserData(ud) => {
                    if let Ok(q) = ud.borrow::<LuaAskQuestion>() {
                        batch.questions.push(q.inner.clone());
                    }
                }
                Value::Table(t) => {
                    batch.questions.push(lua_question_table_to_core(&t)?);
                }
                _ => {}
            }
        }
    }

    Ok(batch)
}

/// Convert an AskQuestion to a Lua table
pub fn core_question_to_lua(lua: &Lua, question: &AskQuestion) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("header", question.header.clone())?;
    table.set("question", question.question.clone())?;
    table.set("multi_select", question.multi_select)?;

    let choices = lua.create_table()?;
    for (i, choice) in question.choices.iter().enumerate() {
        choices.set(i + 1, choice.clone())?;
    }
    table.set("choices", choices)?;

    Ok(table)
}

/// Convert an AskBatch to a Lua table
pub fn core_batch_to_lua(lua: &Lua, batch: &AskBatch) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("id", batch.id.to_string())?;

    let questions = lua.create_table()?;
    for (i, question) in batch.questions.iter().enumerate() {
        questions.set(i + 1, core_question_to_lua(lua, question)?)?;
    }
    table.set("questions", questions)?;

    Ok(table)
}

/// Convert a LuaQuestionAnswer to a crucible_core QuestionAnswer
pub fn lua_answer_to_core(answer: &LuaQuestionAnswer) -> QuestionAnswer {
    answer.inner.clone()
}

/// Convert a LuaAskBatchResponse to a crucible_core AskBatchResponse
pub fn lua_response_to_core(response: &LuaAskBatchResponse) -> AskBatchResponse {
    response.inner.clone()
}

/// Convert a QuestionAnswer to a Lua table
pub fn core_answer_to_lua(lua: &Lua, answer: &QuestionAnswer) -> LuaResult<Table> {
    let table = lua.create_table()?;

    let selected = lua.create_table()?;
    for (i, idx) in answer.selected.iter().enumerate() {
        selected.set(i + 1, *idx)?;
    }
    table.set("selected", selected)?;

    if let Some(ref other) = answer.other {
        table.set("other", other.clone())?;
    }

    Ok(table)
}

/// Convert an AskBatchResponse to a Lua table
pub fn core_response_to_lua(lua: &Lua, response: &AskBatchResponse) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("id", response.id.to_string())?;
    table.set("cancelled", response.cancelled)?;

    let answers = lua.create_table()?;
    for (i, answer) in response.answers.iter().enumerate() {
        answers.set(i + 1, core_answer_to_lua(lua, answer)?)?;
    }
    table.set("answers", answers)?;

    Ok(table)
}

/// Convert a Lua table to a QuestionAnswer
///
/// This supports Lua tables with the structure:
/// ```lua
/// { selected = {0, 1}, other = "optional text" }
/// ```
pub fn lua_answer_table_to_core(table: &Table) -> LuaResult<QuestionAnswer> {
    let mut answer = QuestionAnswer {
        selected: Vec::new(),
        other: None,
    };

    if let Ok(selected_table) = table.get::<Table>("selected") {
        for pair in selected_table.pairs::<i64, usize>() {
            let (_, idx) = pair?;
            answer.selected.push(idx);
        }
    }

    if let Ok(other) = table.get::<String>("other") {
        answer.other = Some(other);
    }

    Ok(answer)
}

/// Convert a Lua table to an AskBatchResponse
///
/// This supports Lua tables with the structure:
/// ```lua
/// { id = "uuid", cancelled = false, answers = { ... } }
/// ```
pub fn lua_response_table_to_core(table: &Table) -> LuaResult<AskBatchResponse> {
    let id = if let Ok(id_str) = table.get::<String>("id") {
        uuid::Uuid::parse_str(&id_str)
            .map_err(|e| mlua::Error::runtime(format!("Invalid UUID: {}", e)))?
    } else {
        uuid::Uuid::new_v4()
    };

    let cancelled = table.get::<bool>("cancelled").unwrap_or(false);

    let mut response = AskBatchResponse {
        id,
        answers: Vec::new(),
        cancelled,
    };

    if let Ok(answers_table) = table.get::<Table>("answers") {
        for pair in answers_table.pairs::<i64, Value>() {
            let (_, value) = pair?;
            match value {
                Value::UserData(ud) => {
                    if let Ok(a) = ud.borrow::<LuaQuestionAnswer>() {
                        response.answers.push(a.inner.clone());
                    }
                }
                Value::Table(t) => {
                    response.answers.push(lua_answer_table_to_core(&t)?);
                }
                _ => {}
            }
        }
    }

    Ok(response)
}

// =============================================================================
// LuaAskContext - Context for ask_user function with async submit bridge
// =============================================================================

use crucible_core::events::SessionEvent;
use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use crucible_core::InteractionRegistry;
use std::sync::{Arc, Mutex};

/// Callback type for pushing session events.
///
/// This abstraction allows the context to work with any event system
/// (EventRing, channels, etc.) without creating circular dependencies.
pub type EventPushCallback = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// Context for ask_user function execution in Lua.
///
/// Holds references to the interaction registry and an event push callback
/// needed to submit requests and wait for responses.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::LuaAskContext;
/// use crucible_core::InteractionRegistry;
/// use std::sync::{Arc, Mutex};
///
/// let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
/// let push_fn: EventPushCallback = Arc::new(|event| {
///     // Push event to your event system (EventRing, channel, etc.)
///     my_event_ring.push(event);
/// });
///
/// let context = LuaAskContext::new(registry, push_fn);
/// ```
#[derive(Clone)]
pub struct LuaAskContext {
    registry: Arc<Mutex<InteractionRegistry>>,
    push_event: EventPushCallback,
}

impl LuaAskContext {
    /// Create a new context with registry and event push callback.
    ///
    /// # Arguments
    ///
    /// * `registry` - Shared interaction registry for request-response correlation
    /// * `push_event` - Callback to push SessionEvent to the event system
    pub fn new(registry: Arc<Mutex<InteractionRegistry>>, push_event: EventPushCallback) -> Self {
        Self {
            registry,
            push_event,
        }
    }

    /// Submit an ask batch and wait for the response.
    ///
    /// This function:
    /// 1. Registers the batch ID with the registry (gets a receiver)
    /// 2. Pushes an InteractionRequested event via the callback
    /// 3. Blocks waiting for the response via the receiver
    ///
    /// # Note
    ///
    /// This blocks the calling thread until the TUI/UI completes the interaction.
    /// In Lua, this is typically called from a script that runs in a separate
    /// thread from the main event loop.
    pub fn ask_user(&self, batch: LuaAskBatch) -> Result<LuaAskBatchResponse, LuaAskError> {
        let core_batch = batch.inner.clone();
        let id = core_batch.id;

        // Register with the registry to get a receiver
        let rx = {
            let mut guard = self
                .registry
                .lock()
                .map_err(|e| LuaAskError::new(format!("Registry lock failed: {}", e)))?;
            guard.register(id)
        };

        // Push InteractionRequested event via callback
        (self.push_event)(SessionEvent::InteractionRequested {
            request_id: id.to_string(),
            request: InteractionRequest::AskBatch(core_batch),
        });

        // Wait for response (blocking)
        // Note: This blocks the current thread until the TUI completes the interaction
        let response = rx
            .blocking_recv()
            .map_err(|_| LuaAskError::new("Interaction was cancelled or dropped".to_string()))?;

        match response {
            InteractionResponse::AskBatch(batch_response) => {
                Ok(LuaAskBatchResponse { inner: batch_response })
            }
            InteractionResponse::Cancelled => Ok(LuaAskBatchResponse::cancelled(id)),
            _ => Err(LuaAskError::new(format!(
                "Unexpected response type: {:?}",
                response
            ))),
        }
    }
}

/// Error type for ask_user function.
#[derive(Debug, Clone)]
pub struct LuaAskError {
    /// Error message
    pub message: String,
}

impl LuaAskError {
    /// Create a new error with message.
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl std::fmt::Display for LuaAskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LuaAskError {}

/// Register the ask module with context for ask_user function.
///
/// This version includes the `ask_user` function that can submit questions
/// to the TUI and wait for responses.
///
/// # Arguments
///
/// * `lua` - The Lua state to register the module with
/// * `context` - The LuaAskContext containing registry and event callback
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::{register_ask_module_with_context, LuaAskContext, EventPushCallback};
/// use crucible_core::InteractionRegistry;
/// use std::sync::{Arc, Mutex};
///
/// let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
/// let push_fn: EventPushCallback = Arc::new(|event| {
///     my_event_ring.push(event);
/// });
/// let context = Arc::new(LuaAskContext::new(registry, push_fn));
///
/// let lua = mlua::Lua::new();
/// register_ask_module_with_context(&lua, context)?;
///
/// // Lua scripts can now use ask.ask_user(batch)
/// ```
pub fn register_ask_module_with_context(
    lua: &Lua,
    context: Arc<LuaAskContext>,
) -> Result<(), LuaError> {
    // First register base module
    register_ask_module(lua)?;

    // Get the ask table
    let ask: Table = lua.globals().get("ask")?;

    // Add ask_user function with context
    let ctx = context.clone();
    let ask_user_fn = lua.create_function(move |_, batch: LuaAskBatch| {
        ctx.ask_user(batch).map_err(|e| mlua::Error::runtime(e.message))
    })?;
    ask.set("ask_user", ask_user_fn)?;

    Ok(())
}

// =============================================================================
// LuaAgentAskContext - For asking questions to LLM agents
// =============================================================================

/// Context for asking questions to an LLM agent instead of a user.
///
/// This enables script-to-agent communication where the "user" answering
/// questions is another LLM. Mirrors the Rune AgentAskContext pattern.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::{LuaAgentAskContext, register_ask_module_with_agent};
/// use std::sync::Arc;
///
/// let backend: Arc<dyn CompletionBackend> = /* create backend */;
/// let context = LuaAgentAskContext::new(backend);
///
/// let lua = mlua::Lua::new();
/// register_ask_module_with_agent(&lua, context)?;
///
/// // Lua scripts can now use ask.agent(batch)
/// ```
#[derive(Clone)]
pub struct LuaAgentAskContext {
    backend: Arc<dyn CompletionBackend>,
}

impl LuaAgentAskContext {
    /// Create new context with a completion backend.
    pub fn new(backend: Arc<dyn CompletionBackend>) -> Self {
        Self { backend }
    }

    /// Submit a batch of questions to the LLM and get structured answers.
    ///
    /// This function:
    /// 1. Formats questions as a structured prompt
    /// 2. Sends to the LLM backend
    /// 3. Parses the response into AskBatchResponse
    pub fn ask_agent(&self, batch: LuaAskBatch) -> Result<LuaAskBatchResponse, LuaAskError> {
        // Use tokio runtime to run async code
        let rt = tokio::runtime::Handle::try_current()
            .map_err(|_| LuaAskError::new("No tokio runtime available".to_string()))?;

        let backend = self.backend.clone();
        let core_batch = batch.inner.clone();

        rt.block_on(async move { Self::ask_agent_async(&backend, core_batch).await })
            .map(|r| LuaAskBatchResponse { inner: r })
            .map_err(LuaAskError::new)
    }

    /// Async implementation of ask_agent
    async fn ask_agent_async(
        backend: &Arc<dyn CompletionBackend>,
        batch: AskBatch,
    ) -> Result<AskBatchResponse, String> {
        let prompt = Self::format_batch_prompt(&batch);
        let system_prompt = Self::system_prompt();

        let request =
            BackendCompletionRequest::new(system_prompt, vec![ContextMessage::user(prompt)]);

        let response = backend
            .complete(request)
            .await
            .map_err(|e| format!("LLM completion failed: {}", e))?;

        Self::parse_response(&response.content, batch)
    }

    /// Format the batch as a structured prompt for the LLM.
    pub fn format_batch_prompt(batch: &AskBatch) -> String {
        let mut prompt = String::from(
            "Please answer the following questions by selecting from the provided choices.\n\n",
        );
        prompt.push_str("For each question, respond with ONLY the choice number (0-indexed) or \"other: <your text>\" if none fit.\n\n");

        for (i, q) in batch.questions.iter().enumerate() {
            prompt.push_str(&format!(
                "Question {}: {} ({})\n",
                i + 1,
                q.question,
                q.header
            ));
            for (j, choice) in q.choices.iter().enumerate() {
                prompt.push_str(&format!("  {}: {}\n", j, choice));
            }
            prompt.push('\n');
        }

        prompt.push_str("\nRespond in JSON format:\n");
        prompt.push_str("```json\n");
        prompt.push_str("{\n");
        prompt.push_str("  \"answers\": [\n");
        prompt.push_str("    {\"selected\": [0], \"other\": null},  // for first question\n");
        prompt
            .push_str("    {\"selected\": [], \"other\": \"custom answer\"}  // if using other\n");
        prompt.push_str("  ]\n");
        prompt.push_str("}\n");
        prompt.push_str("```\n");

        prompt
    }

    /// System prompt for the answering agent.
    fn system_prompt() -> String {
        String::from(
            "You are a helpful assistant answering multiple-choice questions. \
             Respond ONLY with valid JSON containing your answers. \
             Select the most appropriate choice for each question. \
             If no choice fits, use the 'other' field with your custom answer.",
        )
    }

    /// Parse the LLM response into AskBatchResponse.
    pub fn parse_response(content: &str, batch: AskBatch) -> Result<AskBatchResponse, String> {
        // Try to extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = Self::extract_json(content)?;

        // Parse the JSON
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON response: {}", e))?;

        let answers = parsed
            .get("answers")
            .and_then(|a| a.as_array())
            .ok_or_else(|| "Response missing 'answers' array".to_string())?;

        let mut response = AskBatchResponse::new(batch.id);

        for (i, answer) in answers.iter().enumerate() {
            if i >= batch.questions.len() {
                break;
            }

            let selected: Vec<usize> = answer
                .get("selected")
                .and_then(|s| s.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as usize))
                        .collect()
                })
                .unwrap_or_default();

            let other: Option<String> = answer.get("other").and_then(|o| {
                if o.is_null() {
                    None
                } else {
                    o.as_str().map(|s| s.to_string())
                }
            });

            let qa = if let Some(text) = other {
                if selected.is_empty() {
                    QuestionAnswer::other(text)
                } else {
                    // Both selected and other - create struct directly
                    QuestionAnswer {
                        selected,
                        other: Some(text),
                    }
                }
            } else if !selected.is_empty() {
                QuestionAnswer::choices(selected)
            } else {
                QuestionAnswer::choice(0) // Default to first choice
            };

            response = response.answer(qa);
        }

        Ok(response)
    }

    /// Extract JSON from response text (handles markdown code blocks).
    pub fn extract_json(content: &str) -> Result<String, String> {
        // Try to find JSON in code blocks first
        if let Some(start) = content.find("```json") {
            let json_start = start + 7;
            if let Some(end) = content[json_start..].find("```") {
                return Ok(content[json_start..json_start + end].trim().to_string());
            }
        }

        // Try plain code blocks
        if let Some(start) = content.find("```") {
            let json_start = start + 3;
            // Skip optional language identifier on same line
            let json_start = content[json_start..]
                .find('\n')
                .map(|n| json_start + n + 1)
                .unwrap_or(json_start);
            if let Some(end) = content[json_start..].find("```") {
                return Ok(content[json_start..json_start + end].trim().to_string());
            }
        }

        // Try to find raw JSON object
        if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                if end > start {
                    return Ok(content[start..=end].to_string());
                }
            }
        }

        Err("Could not find JSON in response".to_string())
    }
}

/// Register the ask module with LLM agent backend.
///
/// This version includes the `ask.agent(batch)` function that sends questions
/// to an LLM instead of showing a user dialog.
///
/// # Arguments
///
/// * `lua` - The Lua state to register the module with
/// * `context` - The LuaAgentAskContext containing the completion backend
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::ask::{register_ask_module_with_agent, LuaAgentAskContext};
///
/// let backend: Arc<dyn CompletionBackend> = /* create backend */;
/// let context = LuaAgentAskContext::new(backend);
///
/// let lua = mlua::Lua::new();
/// register_ask_module_with_agent(&lua, context)?;
///
/// // Lua scripts can now use ask.agent(batch)
/// ```
pub fn register_ask_module_with_agent(
    lua: &Lua,
    context: LuaAgentAskContext,
) -> Result<(), LuaError> {
    // First register base module
    register_ask_module(lua)?;

    // Get the ask table
    let ask: Table = lua.globals().get("ask")?;

    // Add ask_agent function with context
    let ctx = context.clone();
    let ask_agent_fn = lua.create_function(move |_, batch: LuaAskBatch| {
        ctx.ask_agent(batch).map_err(|e| mlua::Error::runtime(e.message))
    })?;
    ask.set("agent", ask_agent_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_ask_module() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        // Verify ask table exists
        let ask: Table = lua.globals().get("ask").expect("ask should exist");
        assert!(ask.contains_key("question").unwrap());
        assert!(ask.contains_key("batch").unwrap());
    }

    #[test]
    fn test_ask_question_creation() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.question("Library", "Which library?")
        "#;

        let result: LuaAskQuestion = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.inner.header, "Library");
        assert_eq!(result.inner.question, "Which library?");
        assert!(result.inner.choices.is_empty());
        assert!(!result.inner.multi_select);
    }

    #[test]
    fn test_ask_question_with_choices() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.question("Library", "Which library?")
                :choice("Tokio")
                :choice("async-std")
        "#;

        let result: LuaAskQuestion = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.inner.choices.len(), 2);
        assert_eq!(result.inner.choices[0], "Tokio");
        assert_eq!(result.inner.choices[1], "async-std");
    }

    #[test]
    fn test_ask_question_with_choices_array() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.question("DB", "Which database?")
                :choices({"Postgres", "SQLite", "MongoDB"})
        "#;

        let result: LuaAskQuestion = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.inner.choices.len(), 3);
        assert_eq!(result.inner.choices[0], "Postgres");
        assert_eq!(result.inner.choices[1], "SQLite");
        assert_eq!(result.inner.choices[2], "MongoDB");
    }

    #[test]
    fn test_ask_question_multi_select() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.question("Features", "Select features")
                :choices({"Auth", "Logging", "Caching"})
                :multi_select()
        "#;

        let result: LuaAskQuestion = lua.load(script).eval().expect("Should execute");
        assert!(result.inner.multi_select);
    }

    #[test]
    fn test_ask_question_accessors() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local q = ask.question("Header", "Question text")
                :choice("A")
                :choice("B")
                :multi_select()
            return {
                header = q:header(),
                question = q:question_text(),
                choices = q:get_choices(),
                multi = q:is_multi_select()
            }
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("header").unwrap(), "Header");
        assert_eq!(result.get::<String>("question").unwrap(), "Question text");
        assert!(result.get::<bool>("multi").unwrap());

        let choices: Table = result.get("choices").expect("choices should exist");
        assert_eq!(choices.get::<String>(1).unwrap(), "A");
        assert_eq!(choices.get::<String>(2).unwrap(), "B");
    }

    #[test]
    fn test_ask_batch_creation() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.batch()
        "#;

        let result: LuaAskBatch = lua.load(script).eval().expect("Should execute");
        assert!(result.inner.questions.is_empty());
        // ID should be a valid UUID
        assert!(!result.inner.id.is_nil());
    }

    #[test]
    fn test_ask_batch_with_questions() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            return ask.batch()
                :question(ask.question("Auth", "Which authentication?")
                    :choice("JWT")
                    :choice("Session"))
                :question(ask.question("DB", "Which database?")
                    :choice("Postgres")
                    :choice("SQLite"))
        "#;

        let result: LuaAskBatch = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.inner.questions.len(), 2);
        assert_eq!(result.inner.questions[0].header, "Auth");
        assert_eq!(result.inner.questions[0].choices.len(), 2);
        assert_eq!(result.inner.questions[1].header, "DB");
        assert_eq!(result.inner.questions[1].choices.len(), 2);
    }

    #[test]
    fn test_ask_batch_accessors() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local batch = ask.batch()
                :question(ask.question("Q1", "Question 1"):choice("A"))
                :question(ask.question("Q2", "Question 2"):choice("B"))
            return {
                count = batch:question_count(),
                id = batch:id()
            }
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<usize>("count").unwrap(), 2);

        let id_str: String = result.get("id").expect("id should exist");
        // Should be a valid UUID string
        assert!(uuid::Uuid::parse_str(&id_str).is_ok());
    }

    #[test]
    fn test_lua_question_to_core() {
        let q = LuaAskQuestion::new("Header".to_string(), "Question".to_string());
        let core = lua_question_to_core(&q);

        assert_eq!(core.header, "Header");
        assert_eq!(core.question, "Question");
    }

    #[test]
    fn test_lua_batch_to_core() {
        let mut batch = LuaAskBatch::new();
        batch
            .inner
            .questions
            .push(AskQuestion::new("H", "Q").choice("A"));

        let core = lua_batch_to_core(&batch);
        assert_eq!(core.questions.len(), 1);
    }

    #[test]
    fn test_lua_question_table_to_core() {
        let lua = Lua::new();

        let script = r#"
            return {
                header = "Test Header",
                question = "Test Question",
                choices = {"A", "B", "C"},
                multi_select = true
            }
        "#;

        let table: Table = lua.load(script).eval().expect("Should execute");
        let question = lua_question_table_to_core(&table).expect("Should convert");

        assert_eq!(question.header, "Test Header");
        assert_eq!(question.question, "Test Question");
        assert_eq!(question.choices.len(), 3);
        assert!(question.multi_select);
    }

    #[test]
    fn test_lua_batch_table_to_core() {
        let lua = Lua::new();

        let script = r#"
            return {
                questions = {
                    { header = "H1", question = "Q1", choices = {"A"} },
                    { header = "H2", question = "Q2", choices = {"B", "C"} }
                }
            }
        "#;

        let table: Table = lua.load(script).eval().expect("Should execute");
        let batch = lua_batch_table_to_core(&table).expect("Should convert");

        assert_eq!(batch.questions.len(), 2);
        assert_eq!(batch.questions[0].header, "H1");
        assert_eq!(batch.questions[1].header, "H2");
    }

    #[test]
    fn test_core_question_to_lua() {
        let lua = Lua::new();

        let question = AskQuestion::new("Header", "Question")
            .choice("A")
            .choice("B")
            .multi_select();

        let table = core_question_to_lua(&lua, &question).expect("Should convert");

        assert_eq!(table.get::<String>("header").unwrap(), "Header");
        assert_eq!(table.get::<String>("question").unwrap(), "Question");
        assert!(table.get::<bool>("multi_select").unwrap());

        let choices: Table = table.get("choices").expect("choices should exist");
        assert_eq!(choices.get::<String>(1).unwrap(), "A");
        assert_eq!(choices.get::<String>(2).unwrap(), "B");
    }

    #[test]
    fn test_core_batch_to_lua() {
        let lua = Lua::new();

        let batch = AskBatch::new()
            .question(AskQuestion::new("H1", "Q1").choice("A"))
            .question(AskQuestion::new("H2", "Q2").choice("B"));

        let table = core_batch_to_lua(&lua, &batch).expect("Should convert");

        let id: String = table.get("id").expect("id should exist");
        assert!(uuid::Uuid::parse_str(&id).is_ok());

        let questions: Table = table.get("questions").expect("questions should exist");
        let q1: Table = questions.get(1).expect("q1 should exist");
        assert_eq!(q1.get::<String>("header").unwrap(), "H1");
    }

    #[test]
    fn test_question_tostring() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local q = ask.question("Header", "Question"):choice("A"):choice("B")
            return tostring(q)
        "#;

        let result: String = lua.load(script).eval().expect("Should execute");
        assert!(result.contains("Header"));
        assert!(result.contains("Question"));
        assert!(result.contains("choices: 2"));
    }

    #[test]
    fn test_batch_tostring() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local batch = ask.batch()
                :question(ask.question("H1", "Q1"):choice("A"))
            return tostring(batch)
        "#;

        let result: String = lua.load(script).eval().expect("Should execute");
        assert!(result.contains("questions: 1"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // LuaQuestionAnswer tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_question_answer_creation() {
        let answer = LuaQuestionAnswer::new(vec![0, 2]);
        assert_eq!(answer.inner.selected, vec![0, 2]);
        assert!(answer.inner.other.is_none());
    }

    #[test]
    fn test_question_answer_with_other() {
        let answer = LuaQuestionAnswer::with_other("Custom input".to_string());
        assert!(answer.inner.selected.is_empty());
        assert_eq!(answer.inner.other, Some("Custom input".to_string()));
    }

    #[test]
    fn test_question_answer_methods() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local answer = ask.answer({0, 2})
            return {
                first = answer:first_selected(),
                count = answer:selection_count(),
                has_sel = answer:has_selection(),
                has_other = answer:has_other()
            }
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<usize>("first").unwrap(), 0);
        assert_eq!(result.get::<usize>("count").unwrap(), 2);
        assert!(result.get::<bool>("has_sel").unwrap());
        assert!(!result.get::<bool>("has_other").unwrap());
    }

    #[test]
    fn test_question_answer_other_method() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local answer = ask.answer_other("Custom text")
            return {
                other = answer:other(),
                has_other = answer:has_other(),
                has_sel = answer:has_selection()
            }
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<String>("other").unwrap(), "Custom text");
        assert!(result.get::<bool>("has_other").unwrap());
        assert!(!result.get::<bool>("has_sel").unwrap());
    }

    #[test]
    fn test_question_answer_selected_table() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local answer = ask.answer({1, 3, 5})
            local sel = answer:selected()
            return { sel[1], sel[2], sel[3] }
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        assert_eq!(result.get::<usize>(1).unwrap(), 1);
        assert_eq!(result.get::<usize>(2).unwrap(), 3);
        assert_eq!(result.get::<usize>(3).unwrap(), 5);
    }

    #[test]
    fn test_question_answer_tostring() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local answer = ask.answer({0, 1})
            return tostring(answer)
        "#;

        let result: String = lua.load(script).eval().expect("Should execute");
        assert!(result.contains("selected"));
        assert!(result.contains("0"));
        assert!(result.contains("1"));
    }

    #[test]
    fn test_question_answer_other_tostring() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let script = r#"
            local answer = ask.answer_other("Custom")
            return tostring(answer)
        "#;

        let result: String = lua.load(script).eval().expect("Should execute");
        assert!(result.contains("other"));
        assert!(result.contains("Custom"));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // LuaAskBatchResponse tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_batch_response_creation() {
        let id = uuid::Uuid::new_v4();
        let response = LuaAskBatchResponse::new(id);
        assert_eq!(response.inner.id, id);
        assert!(!response.inner.cancelled);
        assert!(response.inner.answers.is_empty());
    }

    #[test]
    fn test_batch_response_cancelled() {
        let id = uuid::Uuid::new_v4();
        let response = LuaAskBatchResponse::cancelled(id);
        assert_eq!(response.inner.id, id);
        assert!(response.inner.cancelled);
    }

    #[test]
    fn test_lua_answer_to_core() {
        let answer = LuaQuestionAnswer::new(vec![1, 2]);
        let core = lua_answer_to_core(&answer);
        assert_eq!(core.selected, vec![1, 2]);
    }

    #[test]
    fn test_lua_response_to_core() {
        let id = uuid::Uuid::new_v4();
        let response = LuaAskBatchResponse::new(id);
        let core = lua_response_to_core(&response);
        assert_eq!(core.id, id);
    }

    #[test]
    fn test_core_answer_to_lua() {
        let lua = Lua::new();
        let answer = QuestionAnswer::choice(2);
        let table = core_answer_to_lua(&lua, &answer).expect("Should convert");

        let selected: Table = table.get("selected").expect("selected should exist");
        assert_eq!(selected.get::<usize>(1).unwrap(), 2);
    }

    #[test]
    fn test_core_answer_with_other_to_lua() {
        let lua = Lua::new();
        let answer = QuestionAnswer::other("Custom");
        let table = core_answer_to_lua(&lua, &answer).expect("Should convert");

        let other: String = table.get("other").expect("other should exist");
        assert_eq!(other, "Custom");
    }

    #[test]
    fn test_core_response_to_lua() {
        let lua = Lua::new();
        let response = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(0))
            .answer(QuestionAnswer::other("Text"));

        let table = core_response_to_lua(&lua, &response).expect("Should convert");

        let id: String = table.get("id").expect("id should exist");
        assert!(uuid::Uuid::parse_str(&id).is_ok());

        let cancelled: bool = table.get("cancelled").expect("cancelled should exist");
        assert!(!cancelled);

        let answers: Table = table.get("answers").expect("answers should exist");
        let answer1: Table = answers.get(1).expect("answer1 should exist");
        let selected1: Table = answer1.get("selected").expect("selected should exist");
        assert_eq!(selected1.get::<usize>(1).unwrap(), 0);
    }

    #[test]
    fn test_lua_answer_table_to_core() {
        let lua = Lua::new();

        let script = r#"
            return { selected = {0, 2}, other = "Custom" }
        "#;

        let table: Table = lua.load(script).eval().expect("Should execute");
        let answer = lua_answer_table_to_core(&table).expect("Should convert");

        assert_eq!(answer.selected, vec![0, 2]);
        assert_eq!(answer.other, Some("Custom".to_string()));
    }

    #[test]
    fn test_lua_response_table_to_core() {
        let lua = Lua::new();
        let id = uuid::Uuid::new_v4();

        let script = format!(
            r#"
            return {{
                id = "{}",
                cancelled = false,
                answers = {{
                    {{ selected = {{0}}, other = nil }},
                    {{ selected = {{}}, other = "Custom" }}
                }}
            }}
        "#,
            id
        );

        let table: Table = lua.load(&script).eval().expect("Should execute");
        let response = lua_response_table_to_core(&table).expect("Should convert");

        assert_eq!(response.id, id);
        assert!(!response.cancelled);
        assert_eq!(response.answers.len(), 2);
        assert_eq!(response.answers[0].selected, vec![0]);
        assert_eq!(response.answers[1].other, Some("Custom".to_string()));
    }

    #[test]
    fn test_notify_function() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        // notify should not error
        let script = r#"
            ask.notify("Test notification")
            return true
        "#;

        let result: bool = lua.load(script).eval().expect("Should execute");
        assert!(result);
    }

    #[test]
    fn test_module_has_new_functions() {
        let lua = Lua::new();
        register_ask_module(&lua).expect("Should register ask module");

        let ask: Table = lua.globals().get("ask").expect("ask should exist");
        assert!(ask.contains_key("notify").unwrap());
        assert!(ask.contains_key("answer").unwrap());
        assert!(ask.contains_key("answer_other").unwrap());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // LuaAskContext tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_lua_ask_error() {
        let error = LuaAskError::new("test error".to_string());
        assert_eq!(error.message, "test error");
        assert_eq!(format!("{}", error), "test error");
    }

    #[test]
    fn test_lua_ask_context_creation() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let push_fn: EventPushCallback = Arc::new(move |_event| {
            called_clone.store(true, Ordering::SeqCst);
        });

        let context = LuaAskContext::new(registry.clone(), push_fn);

        // Verify context was created with the registry
        assert!(!called.load(Ordering::SeqCst));

        // The registry should be accessible
        let guard = context.registry.lock().unwrap();
        assert_eq!(guard.pending_count(), 0);
    }

    #[test]
    fn test_register_ask_module_with_context() {
        let lua = Lua::new();
        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let push_fn: EventPushCallback = Arc::new(|_event| {});
        let context = Arc::new(LuaAskContext::new(registry, push_fn));

        register_ask_module_with_context(&lua, context).expect("Should register");

        // Verify ask_user function exists
        let ask: Table = lua.globals().get("ask").expect("ask should exist");
        assert!(ask.contains_key("ask_user").unwrap());
    }

    #[test]
    fn test_lua_ask_context_event_push() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let event_received = Arc::new(AtomicBool::new(false));
        let received_request_id = Arc::new(Mutex::new(String::new()));

        let event_received_clone = event_received.clone();
        let received_request_id_clone = received_request_id.clone();

        let push_fn: EventPushCallback = Arc::new(move |event| {
            event_received_clone.store(true, Ordering::SeqCst);
            if let SessionEvent::InteractionRequested { request_id, .. } = event {
                *received_request_id_clone.lock().unwrap() = request_id;
            }
        });

        let context = LuaAskContext::new(registry.clone(), push_fn);

        // Create a batch and manually trigger the push event path
        let batch = LuaAskBatch::new();
        let batch_id = batch.inner.id;

        // Manually push the event (simulating what ask_user does internally)
        (context.push_event)(SessionEvent::InteractionRequested {
            request_id: batch_id.to_string(),
            request: InteractionRequest::AskBatch(batch.inner.clone()),
        });

        // Verify the event was received
        assert!(event_received.load(Ordering::SeqCst));
        assert_eq!(
            *received_request_id.lock().unwrap(),
            batch_id.to_string()
        );
    }

    #[test]
    fn test_lua_ask_context_registry_integration() {
        // Test that the context correctly registers with the registry
        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let push_fn: EventPushCallback = Arc::new(|_event| {});
        let _context = LuaAskContext::new(registry.clone(), push_fn);

        // The batch ID should be registerable
        let batch = LuaAskBatch::new();
        let batch_id = batch.inner.id;

        {
            let mut guard = registry.lock().unwrap();
            let _rx = guard.register(batch_id);
            assert!(guard.is_pending(batch_id));
        }
    }

    #[test]
    fn test_lua_ask_context_clone() {
        let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
        let push_fn: EventPushCallback = Arc::new(|_event| {});
        let context = LuaAskContext::new(registry.clone(), push_fn);

        let cloned = context.clone();

        // Both should reference the same registry (they share the Arc)
        // Note: We can't lock both at once as that would deadlock with std::sync::Mutex
        // Instead, verify they point to the same underlying data via Arc::ptr_eq
        assert!(Arc::ptr_eq(&context.registry, &cloned.registry));

        // Also verify functionality works through either handle
        {
            let guard = context.registry.lock().unwrap();
            assert_eq!(guard.pending_count(), 0);
        }
        {
            let guard = cloned.registry.lock().unwrap();
            assert_eq!(guard.pending_count(), 0);
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // LuaAgentAskContext tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_extract_json_from_code_block() {
        let content = r#"Here's my answer:
```json
{"answers": [{"selected": [0], "other": null}]}
```"#;
        let result = LuaAgentAskContext::extract_json(content);
        assert!(result.is_ok());
        let json = result.unwrap();
        assert!(json.contains("answers"));
    }

    #[test]
    fn test_extract_json_from_plain_block() {
        let content = r#"Here's my answer:
```
{"answers": [{"selected": [1], "other": null}]}
```"#;
        let result = LuaAgentAskContext::extract_json(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_extract_json_raw() {
        let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
        let result = LuaAgentAskContext::extract_json(content);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), content);
    }

    #[test]
    fn test_extract_json_with_surrounding_text() {
        let content =
            r#"The answer is {"answers": [{"selected": [0], "other": null}]} and that's it."#;
        let result = LuaAgentAskContext::extract_json(content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_response_single_choice() {
        let batch =
            AskBatch::new().question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

        let content = r#"{"answers": [{"selected": [0], "other": null}]}"#;
        let result = LuaAgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 1);
        assert_eq!(response.answers[0].selected, vec![0]);
        assert!(response.answers[0].other.is_none());
    }

    #[test]
    fn test_parse_response_with_other() {
        let batch =
            AskBatch::new().question(AskQuestion::new("Q1", "First?").choice("A").choice("B"));

        let content = r#"{"answers": [{"selected": [], "other": "custom answer"}]}"#;
        let result = LuaAgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 1);
        assert!(response.answers[0].selected.is_empty());
        assert_eq!(response.answers[0].other, Some("custom answer".to_string()));
    }

    #[test]
    fn test_parse_response_multi_select() {
        let batch = AskBatch::new().question(
            AskQuestion::new("Q1", "First?")
                .choice("A")
                .choice("B")
                .choice("C"),
        );

        let content = r#"{"answers": [{"selected": [0, 2], "other": null}]}"#;
        let result = LuaAgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers[0].selected, vec![0, 2]);
    }

    #[test]
    fn test_parse_response_multiple_questions() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A").choice("B"))
            .question(AskQuestion::new("Q2", "Second?").choice("X").choice("Y"));

        let content = r#"{"answers": [
            {"selected": [0], "other": null},
            {"selected": [1], "other": null}
        ]}"#;
        let result = LuaAgentAskContext::parse_response(content, batch);

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.answers.len(), 2);
        assert_eq!(response.answers[0].selected, vec![0]);
        assert_eq!(response.answers[1].selected, vec![1]);
    }

    #[test]
    fn test_format_batch_prompt() {
        let batch = AskBatch::new().question(
            AskQuestion::new("Auth", "Method?")
                .choice("OAuth")
                .choice("JWT"),
        );

        let prompt = LuaAgentAskContext::format_batch_prompt(&batch);

        assert!(prompt.contains("Question 1: Method? (Auth)"));
        assert!(prompt.contains("0: OAuth"));
        assert!(prompt.contains("1: JWT"));
        assert!(prompt.contains("JSON format"));
    }
}
