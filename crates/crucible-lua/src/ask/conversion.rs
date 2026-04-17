use crucible_core::interaction::{AskBatch, AskBatchResponse, AskQuestion, QuestionAnswer};
use crucible_core::uuid;
use mlua::{Lua, Result as LuaResult, Table, Value};

use super::types::{LuaAskBatch, LuaAskBatchResponse, LuaAskQuestion, LuaQuestionAnswer};

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
