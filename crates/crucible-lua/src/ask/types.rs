use crucible_core::interaction::{AskBatch, AskBatchResponse, AskQuestion, QuestionAnswer};
use crucible_core::uuid;
use mlua::{FromLua, Lua, MetaMethod, Result as LuaResult, UserData, UserDataMethods, Value};

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
            let idx = index
                .checked_sub(1)
                .ok_or_else(|| mlua::Error::runtime("Answer index must be >= 1"))?;
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
                table.set(
                    i + 1,
                    LuaQuestionAnswer {
                        inner: answer.clone(),
                    },
                )?;
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
