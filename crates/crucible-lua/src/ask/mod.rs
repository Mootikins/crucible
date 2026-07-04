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

mod context;
mod conversion;
mod register;
mod types;

#[cfg(test)]
mod tests;

pub use context::{EventPushCallback, LuaAskContext, LuaAskError};
pub use conversion::{
    core_answer_to_lua, core_batch_to_lua, core_question_to_lua, core_response_to_lua,
    lua_answer_table_to_core, lua_answer_to_core, lua_batch_table_to_core, lua_batch_to_core,
    lua_question_table_to_core, lua_question_to_core, lua_response_table_to_core,
    lua_response_to_core,
};
pub use register::register_ask_module;
pub use types::{LuaAskBatch, LuaAskBatchResponse, LuaAskQuestion, LuaQuestionAnswer};
