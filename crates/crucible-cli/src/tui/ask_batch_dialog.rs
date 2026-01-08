//! Dialog for batched ask interactions.
//!
//! Renders multiple questions with choices and an "Other" text input option.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crucible_core::interaction::{AskBatch, AskBatchResponse, AskQuestion, QuestionAnswer};

/// Re-export Uuid for external use
pub type Uuid = crucible_core::uuid::Uuid;

use super::widgets::{MultiLineInputState, MultiLineInputWidget};

/// Result from ask batch dialog interaction.
#[derive(Debug, Clone)]
pub enum AskBatchResult {
    /// User completed all questions.
    Complete(AskBatchResponse),
    /// User cancelled.
    Cancelled(Uuid),
    /// Still in progress.
    Pending,
}

/// State for an ask batch dialog.
#[derive(Debug, Clone)]
pub struct AskBatchDialogState {
    /// The batch being asked.
    batch: AskBatch,
    /// Current question index.
    current_question: usize,
    /// Answers collected so far.
    answers: Vec<QuestionAnswer>,
    /// Currently selected choice index per question.
    selected_choice: usize,
    /// Whether "Other" is selected.
    other_selected: bool,
    /// "Other" input state.
    other_input: MultiLineInputState,
}

impl AskBatchDialogState {
    /// Create a new dialog state from an AskBatch.
    pub fn new(batch: AskBatch) -> Self {
        Self {
            batch,
            current_question: 0,
            answers: Vec::new(),
            selected_choice: 0,
            other_selected: false,
            other_input: MultiLineInputState::new(),
        }
    }

    /// Get the batch ID.
    pub fn id(&self) -> Uuid {
        self.batch.id
    }

    /// Get the current question.
    pub fn current(&self) -> Option<&AskQuestion> {
        self.batch.questions.get(self.current_question)
    }

    /// Handle key input.
    pub fn handle_key(&mut self, key: KeyEvent) -> AskBatchResult {
        // If in "Other" text input mode
        if self.other_selected {
            match key.code {
                KeyCode::Esc => {
                    // Exit "Other" mode back to choices
                    self.other_selected = false;
                    self.other_input = MultiLineInputState::new();
                }
                KeyCode::Enter => {
                    // Submit "Other" answer if not empty
                    if !self.other_input.is_empty() {
                        return self.submit_answer(QuestionAnswer::other(self.other_input.text()));
                    }
                }
                _ => {
                    self.other_input.handle_key(key);
                }
            }
            return AskBatchResult::Pending;
        }

        // Choice selection mode
        let choice_count = self.current().map(|q| q.choices.len()).unwrap_or(0);

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_choice = self.selected_choice.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                // +1 for "Other" option
                self.selected_choice = (self.selected_choice + 1).min(choice_count);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.selected_choice >= choice_count {
                    // "Other" selected
                    self.other_selected = true;
                } else {
                    // Choice selected
                    return self.submit_answer(QuestionAnswer::choice(self.selected_choice));
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = (c as usize) - ('1' as usize);
                if idx < choice_count {
                    return self.submit_answer(QuestionAnswer::choice(idx));
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                return AskBatchResult::Cancelled(self.batch.id);
            }
            _ => {}
        }
        AskBatchResult::Pending
    }

    /// Submit an answer and advance to next question.
    fn submit_answer(&mut self, answer: QuestionAnswer) -> AskBatchResult {
        self.answers.push(answer);
        self.current_question += 1;
        self.selected_choice = 0;
        self.other_selected = false;
        self.other_input = MultiLineInputState::new();

        if self.current_question >= self.batch.questions.len() {
            let mut response = AskBatchResponse::new(self.batch.id);
            response.answers = std::mem::take(&mut self.answers);
            AskBatchResult::Complete(response)
        } else {
            AskBatchResult::Pending
        }
    }

    /// Get progress (current/total).
    pub fn progress(&self) -> (usize, usize) {
        (self.current_question + 1, self.batch.questions.len())
    }
}

/// Widget for rendering ask batch dialog.
pub struct AskBatchDialogWidget<'a> {
    state: &'a AskBatchDialogState,
}

impl<'a> AskBatchDialogWidget<'a> {
    pub fn new(state: &'a AskBatchDialogState) -> Self {
        Self { state }
    }

    fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(area);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

impl Widget for AskBatchDialogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let Some(question) = self.state.current() else {
            return;
        };

        // Dim background
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Style::default().bg(Color::Black));
                }
            }
        }

        // Dialog area
        let dialog_area = Self::centered_rect(60, 60, area);
        Clear.render(dialog_area, buf);

        // Progress indicator
        let (current, total) = self.state.progress();
        let title = format!(" {} ({}/{}) ", question.header, current, total);

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(dialog_area);
        block.render(dialog_area, buf);

        // Question text
        let question_para = Paragraph::new(question.question.as_str())
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        let question_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 2,
        };
        question_para.render(question_area, buf);

        // Choices
        let choices_start_y = inner.y + 3;
        for (i, choice) in question.choices.iter().enumerate() {
            let y = choices_start_y + i as u16;
            if y >= inner.y + inner.height - 3 {
                break;
            }

            let is_selected = !self.state.other_selected && i == self.state.selected_choice;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if is_selected { "> " } else { "  " };
            let number = format!("{}. ", i + 1);
            buf.set_string(inner.x, y, prefix, style);
            buf.set_string(
                inner.x + 2,
                y,
                &number,
                Style::default().fg(Color::DarkGray),
            );
            buf.set_string(inner.x + 5, y, choice, style);
        }

        // "Other" option
        let other_y = choices_start_y + question.choices.len() as u16;
        if other_y < inner.y + inner.height - 3 {
            let is_other_selected =
                !self.state.other_selected && self.state.selected_choice >= question.choices.len();
            let style = if is_other_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Yellow)
            };

            let prefix = if is_other_selected { "> " } else { "  " };
            buf.set_string(inner.x, other_y, prefix, style);
            buf.set_string(inner.x + 2, other_y, "Other (type custom answer)", style);
        }

        // "Other" input area if active
        if self.state.other_selected {
            use crate::tui::constants::UiConstants;
            let input_y = other_y + 2;
            let input_area = Rect {
                x: inner.x + 2,
                y: input_y,
                width: UiConstants::dialog_text_width(inner.width),
                height: 3,
            };

            // Input background
            for y in input_area.y..input_area.y + input_area.height {
                for x in input_area.x..input_area.x + input_area.width {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_char(' ');
                        cell.set_style(Style::default().bg(Color::DarkGray));
                    }
                }
            }

            MultiLineInputWidget::new(&self.state.other_input)
                .placeholder("Type your answer...")
                .render(input_area, buf);
        }

        // Hint
        let hint = if self.state.other_selected {
            "[Enter] Submit  [Esc] Back"
        } else {
            "[↑↓] Navigate  [Enter] Select  [Esc] Cancel"
        };
        let hint_y = inner.y + inner.height - 1;
        use crate::tui::geometry::PopupGeometry;
        let hint_x = PopupGeometry::center_text_horizontally(inner, hint.len() as u16);
        buf.set_string(hint_x, hint_y, hint, Style::default().fg(Color::DarkGray));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn select_first_choice() {
        let batch =
            AskBatch::new().question(AskQuestion::new("Test", "Pick one").choice("A").choice("B"));
        let mut state = AskBatchDialogState::new(batch);

        let result = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(result, AskBatchResult::Complete(_)));

        if let AskBatchResult::Complete(response) = result {
            assert_eq!(response.answers[0].selected, vec![0]);
        }
    }

    #[test]
    fn navigate_and_select() {
        let batch =
            AskBatch::new().question(AskQuestion::new("Test", "Pick one").choice("A").choice("B"));
        let mut state = AskBatchDialogState::new(batch);

        state.handle_key(key(KeyCode::Down));
        let result = state.handle_key(key(KeyCode::Enter));

        if let AskBatchResult::Complete(response) = result {
            assert_eq!(response.answers[0].selected, vec![1]);
        }
    }

    #[test]
    fn other_option() {
        let batch = AskBatch::new().question(AskQuestion::new("Test", "Pick one").choice("A"));
        let mut state = AskBatchDialogState::new(batch);

        // Navigate to "Other"
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter)); // Enter "Other" mode

        assert!(state.other_selected);

        // Type something
        state.handle_key(key(KeyCode::Char('X')));
        state.handle_key(key(KeyCode::Char('Y')));
        state.handle_key(key(KeyCode::Char('Z')));

        let result = state.handle_key(key(KeyCode::Enter));

        if let AskBatchResult::Complete(response) = result {
            assert_eq!(response.answers[0].other, Some("XYZ".into()));
        }
    }

    #[test]
    fn escape_cancels() {
        let batch = AskBatch::new().question(AskQuestion::new("Test", "Pick").choice("A"));
        let mut state = AskBatchDialogState::new(batch.clone());

        let result = state.handle_key(key(KeyCode::Esc));
        assert!(matches!(result, AskBatchResult::Cancelled(id) if id == batch.id));
    }

    #[test]
    fn multi_question_flow() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A"))
            .question(AskQuestion::new("Q2", "Second?").choice("B"));
        let mut state = AskBatchDialogState::new(batch);

        // Answer first question
        let result = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(result, AskBatchResult::Pending));
        assert_eq!(state.current_question, 1);

        // Answer second question
        let result = state.handle_key(key(KeyCode::Enter));
        assert!(matches!(result, AskBatchResult::Complete(_)));

        if let AskBatchResult::Complete(response) = result {
            assert_eq!(response.answers.len(), 2);
        }
    }

    #[test]
    fn number_keys_select_directly() {
        let batch = AskBatch::new().question(
            AskQuestion::new("Test", "Pick")
                .choice("A")
                .choice("B")
                .choice("C"),
        );
        let mut state = AskBatchDialogState::new(batch);

        // Press '2' to select second option directly
        let result = state.handle_key(key(KeyCode::Char('2')));

        if let AskBatchResult::Complete(response) = result {
            assert_eq!(response.answers[0].selected, vec![1]);
        }
    }

    #[test]
    fn escape_from_other_returns_to_choices() {
        let batch = AskBatch::new().question(AskQuestion::new("Test", "Pick").choice("A"));
        let mut state = AskBatchDialogState::new(batch);

        // Enter "Other" mode
        state.handle_key(key(KeyCode::Down));
        state.handle_key(key(KeyCode::Enter));
        assert!(state.other_selected);

        // Escape back to choices
        state.handle_key(key(KeyCode::Esc));
        assert!(!state.other_selected);
    }

    #[test]
    fn progress_tracking() {
        let batch = AskBatch::new()
            .question(AskQuestion::new("Q1", "First?").choice("A"))
            .question(AskQuestion::new("Q2", "Second?").choice("B"))
            .question(AskQuestion::new("Q3", "Third?").choice("C"));
        let mut state = AskBatchDialogState::new(batch);

        assert_eq!(state.progress(), (1, 3));

        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.progress(), (2, 3));

        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.progress(), (3, 3));
    }
}
