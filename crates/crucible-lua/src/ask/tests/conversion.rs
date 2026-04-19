use super::super::*;
use crucible_core::interaction::{AskBatch, AskBatchResponse, AskQuestion, QuestionAnswer};
use crucible_core::uuid;
use mlua::{Lua, Table};

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
