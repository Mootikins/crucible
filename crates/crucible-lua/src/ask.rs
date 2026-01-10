//! Ask module for Lua scripts
//!
//! Provides batched user questions with choices for Lua and Fennel scripts.
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
//! ```

use crate::error::LuaError;
use crucible_core::interaction::{AskBatch, AskQuestion};
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
}
