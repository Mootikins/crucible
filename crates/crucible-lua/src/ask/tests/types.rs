use super::super::*;
use crucible_core::uuid;
use mlua::{Lua, Table};

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
fn test_lua_ask_error() {
    let error = LuaAskError::new("test error".to_string());
    assert_eq!(error.message, "test error");
    assert_eq!(format!("{}", error), "test error");
}
