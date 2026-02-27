//! Ask-related interaction types.
//!
//! Types for asking questions to users, including single questions,
//! multi-select, and batched question flows.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Ask Request/Response
// ─────────────────────────────────────────────────────────────────────────────

/// A question to ask the user.
///
/// Supports single-select, multi-select, and free-text input modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskRequest {
    /// The question text to display.
    pub question: String,
    /// Optional list of choices. If None, expects free-text input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    /// Allow selecting multiple choices.
    #[serde(default)]
    pub multi_select: bool,
    /// Allow free-text input in addition to choices.
    #[serde(default)]
    pub allow_other: bool,
}

impl AskRequest {
    /// Create a new question.
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            choices: None,
            multi_select: false,
            allow_other: false,
        }
    }

    /// Add choices to the question.
    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices = Some(choices.into_iter().map(Into::into).collect());
        self
    }

    /// Enable multi-select mode.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }

    /// Allow free-text "other" input.
    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }
}

/// Response to an [`AskRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskResponse {
    /// Indices of selected choices (empty if using "other").
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Free-text input if "other" was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl AskResponse {
    /// Create a response with a single selection.
    pub fn selected(index: usize) -> Self {
        Self {
            selected: vec![index],
            other: None,
        }
    }

    /// Create a response with multiple selections.
    pub fn selected_many<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    /// Create a response with free-text input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Batched Ask Request/Response (for multi-question interactions)
// ─────────────────────────────────────────────────────────────────────────────

/// A batch of questions to ask the user.
///
/// Supports 1-4 questions shown together. Each question has choices,
/// and an "Other" free-text option is always implicitly available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskBatch {
    /// Unique ID for correlating request with response.
    pub id: uuid::Uuid,
    /// Questions to ask (1-4).
    pub questions: Vec<AskQuestion>,
}

impl AskBatch {
    /// Create a new empty batch with a generated ID.
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            questions: Vec::new(),
        }
    }

    /// Create a batch with a specific ID.
    pub fn with_id(id: uuid::Uuid) -> Self {
        Self {
            id,
            questions: Vec::new(),
        }
    }

    /// Add a question to the batch.
    pub fn question(mut self, q: AskQuestion) -> Self {
        self.questions.push(q);
        self
    }
}

impl Default for AskBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// A single question in an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskQuestion {
    /// Short label (max 12 chars) displayed as header.
    pub header: String,
    /// Full question text.
    pub question: String,
    /// Available choices.
    pub choices: Vec<String>,
    /// Allow multiple selections.
    #[serde(default)]
    pub multi_select: bool,
    /// Allow free-text "other" input.
    #[serde(default)]
    pub allow_other: bool,
}

impl AskQuestion {
    /// Create a new question.
    pub fn new(header: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            question: question.into(),
            choices: Vec::new(),
            multi_select: false,
            allow_other: false,
        }
    }

    /// Add a choice.
    pub fn choice(mut self, c: impl Into<String>) -> Self {
        self.choices.push(c.into());
        self
    }

    /// Add multiple choices at once.
    pub fn choices<I, S>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.choices.extend(choices.into_iter().map(Into::into));
        self
    }

    /// Enable multi-select mode.
    pub fn multi_select(mut self) -> Self {
        self.multi_select = true;
        self
    }
}

/// Response to an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskBatchResponse {
    /// The request ID this responds to.
    pub id: uuid::Uuid,
    /// One answer per question, in order.
    pub answers: Vec<QuestionAnswer>,
    /// True if user cancelled the whole interaction.
    #[serde(default)]
    pub cancelled: bool,
}

impl AskBatchResponse {
    /// Create a new response for a request ID.
    pub fn new(id: uuid::Uuid) -> Self {
        Self {
            id,
            answers: Vec::new(),
            cancelled: false,
        }
    }

    /// Add an answer.
    pub fn answer(mut self, a: QuestionAnswer) -> Self {
        self.answers.push(a);
        self
    }

    /// Mark as cancelled.
    pub fn cancelled(id: uuid::Uuid) -> Self {
        Self {
            id,
            answers: Vec::new(),
            cancelled: true,
        }
    }
}

/// Answer to a single question in an [`AskBatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionAnswer {
    /// Selected choice indices (empty if "Other" was chosen).
    #[serde(default)]
    pub selected: Vec<usize>,
    /// Free-text input if "Other" was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
}

impl QuestionAnswer {
    /// Create answer with a single choice selection.
    pub fn choice(index: usize) -> Self {
        Self {
            selected: vec![index],
            other: None,
        }
    }

    /// Create answer with multiple choice selections.
    pub fn choices<I: IntoIterator<Item = usize>>(indices: I) -> Self {
        Self {
            selected: indices.into_iter().collect(),
            other: None,
        }
    }

    /// Create answer with free-text "Other" input.
    pub fn other(text: impl Into<String>) -> Self {
        Self {
            selected: Vec::new(),
            other: Some(text.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_request_with_choices() {
        let ask = AskRequest::new("Which option?").choices(["Option A", "Option B", "Option C"]);

        assert_eq!(ask.question, "Which option?");
        assert_eq!(
            ask.choices,
            Some(vec![
                "Option A".into(),
                "Option B".into(),
                "Option C".into()
            ])
        );
        assert!(!ask.multi_select);
        assert!(!ask.allow_other);
    }

    #[test]
    fn ask_request_multi_select() {
        let ask = AskRequest::new("Select all that apply")
            .choices(["A", "B", "C"])
            .multi_select();

        assert!(ask.multi_select);
    }

    #[test]
    fn ask_request_allows_free_text() {
        let ask = AskRequest::new("Pick or type custom")
            .choices(["Preset 1", "Preset 2"])
            .allow_other();

        assert!(ask.allow_other);
    }

    #[test]
    fn ask_response_single_selection() {
        let response = AskResponse::selected(1);

        assert_eq!(response.selected, vec![1]);
        assert!(response.other.is_none());
    }

    #[test]
    fn ask_response_multi_selection() {
        let response = AskResponse::selected_many([0, 2]);

        assert_eq!(response.selected, vec![0, 2]);
    }

    #[test]
    fn ask_response_custom_text() {
        let response = AskResponse::other("Custom input");

        assert!(response.selected.is_empty());
        assert_eq!(response.other, Some("Custom input".into()));
    }

    #[test]
    fn ask_batch_creation() {
        let batch = AskBatch::new()
            .question(
                AskQuestion::new("Auth", "Which authentication?")
                    .choice("JWT")
                    .choice("Session"),
            )
            .question(
                AskQuestion::new("DB", "Which database?")
                    .choice("Postgres")
                    .choice("SQLite"),
            );

        assert_eq!(batch.questions.len(), 2);
        assert_eq!(batch.questions[0].header, "Auth");
        assert_eq!(batch.questions[0].choices.len(), 2);
    }

    #[test]
    fn ask_batch_response_creation() {
        let response = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(1))
            .answer(QuestionAnswer::other("Custom DB"));

        assert_eq!(response.answers.len(), 2);
        assert_eq!(response.answers[0].selected, vec![1]);
        assert_eq!(response.answers[1].other, Some("Custom DB".into()));
    }

    #[test]
    fn ask_question_multi_select() {
        let q = AskQuestion::new("Features", "Select features")
            .choices(["A", "B", "C"])
            .multi_select();

        assert!(q.multi_select);
        assert_eq!(q.choices.len(), 3);
    }

    #[test]
    fn question_answer_multi_choices() {
        let answer = QuestionAnswer::choices([0, 2]);

        assert_eq!(answer.selected, vec![0, 2]);
        assert!(answer.other.is_none());
    }

    #[test]
    fn ask_batch_cancelled() {
        let id = uuid::Uuid::new_v4();
        let response = AskBatchResponse::cancelled(id);

        assert!(response.cancelled);
        assert!(response.answers.is_empty());
        assert_eq!(response.id, id);
    }

    #[test]
    fn ask_batch_serialization() {
        let batch = AskBatch::new().question(
            AskQuestion::new("Test", "Question?")
                .choice("A")
                .choice("B"),
        );
        let json = serde_json::to_string(&batch).unwrap();
        let restored: AskBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(batch.questions.len(), restored.questions.len());
        assert_eq!(batch.questions[0].header, restored.questions[0].header);
    }

    #[test]
    fn ask_batch_response_serialization() {
        let response = AskBatchResponse::new(uuid::Uuid::new_v4())
            .answer(QuestionAnswer::choice(0))
            .answer(QuestionAnswer::other("Custom"));
        let json = serde_json::to_string(&response).unwrap();
        let restored: AskBatchResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(response.answers.len(), restored.answers.len());
    }

    #[test]
    fn ask_request_serialization() {
        let ask = AskRequest::new("Question?").choices(["A", "B"]);
        let json = serde_json::to_string(&ask).unwrap();
        let restored: AskRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(ask, restored);
    }
}
