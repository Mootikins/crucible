use crate::error::LuaError;
use mlua::Lua;

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
