use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone)]
pub enum Event {
    Key(KeyEvent),
    Resize { width: u16, height: u16 },
    Tick,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    Insert(char),
    Backspace,
    Delete,
    DeleteWord,
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
    Submit,
    Cancel,
    Complete,
    HistoryPrev,
    HistoryNext,
    Clear,
    None,
}

impl From<KeyEvent> for InputAction {
    fn from(key: KeyEvent) -> Self {
        match (key.code, key.modifiers) {
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => InputAction::DeleteWord,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => InputAction::Clear,
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => InputAction::Home,
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => InputAction::End,
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => InputAction::Left,
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => InputAction::Right,
            (KeyCode::Char('b'), KeyModifiers::ALT) => InputAction::WordLeft,
            (KeyCode::Char('f'), KeyModifiers::ALT) => InputAction::WordRight,
            (KeyCode::Left, KeyModifiers::CONTROL) => InputAction::WordLeft,
            (KeyCode::Right, KeyModifiers::CONTROL) => InputAction::WordRight,
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => InputAction::HistoryPrev,
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => InputAction::HistoryNext,
            (KeyCode::Char('j'), KeyModifiers::CONTROL) => InputAction::Insert('\n'),
            (KeyCode::Tab, _) => InputAction::Complete,
            (KeyCode::Char(c), _) => InputAction::Insert(c),
            (KeyCode::Backspace, _) => InputAction::Backspace,
            (KeyCode::Delete, _) => InputAction::Delete,
            (KeyCode::Left, _) => InputAction::Left,
            (KeyCode::Right, _) => InputAction::Right,
            (KeyCode::Home, _) => InputAction::Home,
            (KeyCode::End, _) => InputAction::End,
            (KeyCode::Enter, _) => InputAction::Submit,
            (KeyCode::Up, _) => InputAction::HistoryPrev,
            (KeyCode::Down, _) => InputAction::HistoryNext,
            _ => InputAction::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputBuffer {
    content: String,
    cursor: usize,
    history: Vec<String>,
    history_index: Option<usize>,
    draft: String,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn handle(&mut self, action: InputAction) -> Option<String> {
        match action {
            InputAction::Insert(c) => {
                self.content.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                self.history_index = None;
            }
            InputAction::Backspace => {
                if self.cursor > 0 {
                    let prev_char_boundary = self.content[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.content.remove(prev_char_boundary);
                    self.cursor = prev_char_boundary;
                }
            }
            InputAction::Delete => {
                if self.cursor < self.content.len() {
                    self.content.remove(self.cursor);
                }
            }
            InputAction::DeleteWord => {
                if self.cursor > 0 {
                    let before = &self.content[..self.cursor];
                    let trimmed = before.trim_end();
                    let word_start = trimmed
                        .rfind(|c: char| c.is_whitespace())
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    self.content = format!(
                        "{}{}",
                        &self.content[..word_start],
                        &self.content[self.cursor..]
                    );
                    self.cursor = word_start;
                }
            }
            InputAction::Left => {
                if self.cursor > 0 {
                    self.cursor = self.content[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
            }
            InputAction::Right => {
                if self.cursor < self.content.len() {
                    self.cursor = self.content[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.content.len());
                }
            }
            InputAction::WordLeft => {
                if self.cursor > 0 {
                    let before = &self.content[..self.cursor];
                    let trimmed = before.trim_end();
                    self.cursor = trimmed
                        .rfind(|c: char| c.is_whitespace())
                        .map(|i| i + 1)
                        .unwrap_or(0);
                }
            }
            InputAction::WordRight => {
                if self.cursor < self.content.len() {
                    let after = &self.content[self.cursor..];
                    let trimmed = after.trim_start();
                    let skip = after.len() - trimmed.len();
                    self.cursor += skip
                        + trimmed
                            .find(|c: char| c.is_whitespace())
                            .unwrap_or(trimmed.len());
                }
            }
            InputAction::Home => {
                self.cursor = 0;
            }
            InputAction::End => {
                self.cursor = self.content.len();
            }
            InputAction::Clear => {
                self.content.clear();
                self.cursor = 0;
            }
            InputAction::Submit => {
                if !self.content.is_empty() {
                    let submitted = std::mem::take(&mut self.content);
                    self.history.push(submitted.clone());
                    self.cursor = 0;
                    self.history_index = None;
                    return Some(submitted);
                }
            }
            InputAction::HistoryPrev => {
                if self.history.is_empty() {
                    return None;
                }
                match self.history_index {
                    None => {
                        self.draft = std::mem::take(&mut self.content);
                        self.history_index = Some(self.history.len() - 1);
                        self.content = self.history[self.history.len() - 1].clone();
                    }
                    Some(0) => {}
                    Some(i) => {
                        self.history_index = Some(i - 1);
                        self.content = self.history[i - 1].clone();
                    }
                }
                self.cursor = self.content.len();
            }
            InputAction::HistoryNext => {
                match self.history_index {
                    None => {}
                    Some(i) if i + 1 >= self.history.len() => {
                        self.history_index = None;
                        self.content = std::mem::take(&mut self.draft);
                    }
                    Some(i) => {
                        self.history_index = Some(i + 1);
                        self.content = self.history[i + 1].clone();
                    }
                }
                self.cursor = self.content.len();
            }
            InputAction::Cancel | InputAction::Complete | InputAction::None => {}
        }
        None
    }

    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.content.len();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor, s);
        self.cursor += s.len();
    }
}
