use crate::error::LuaError;
use mlua::{Lua, Table};
use std::sync::Arc;

use super::agent_context::LuaAgentAskContext;
use super::context::LuaAskContext;
use super::types::{LuaAskBatch, LuaAskQuestion, LuaQuestionAnswer};

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
    let answer_fn =
        lua.create_function(|_, selected: Vec<usize>| Ok(LuaQuestionAnswer::new(selected)))?;
    ask.set("answer", answer_fn)?;

    // ask.answer_other(text) -> LuaQuestionAnswer
    // Create an answer with free-text "other" input
    let answer_other_fn =
        lua.create_function(|_, text: String| Ok(LuaQuestionAnswer::with_other(text)))?;
    ask.set("answer_other", answer_other_fn)?;

    // Register as global module
    lua.globals().set("ask", ask)?;

    Ok(())
}

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
        ctx.ask_user(batch)
            .map_err(|e| mlua::Error::runtime(e.message))
    })?;
    ask.set("ask_user", ask_user_fn)?;

    Ok(())
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
        ctx.ask_agent(batch)
            .map_err(|e| mlua::Error::runtime(e.message))
    })?;
    ask.set("agent", ask_agent_fn)?;

    Ok(())
}
