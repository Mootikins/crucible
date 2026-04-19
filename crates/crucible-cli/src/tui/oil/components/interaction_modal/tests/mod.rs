use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

mod ask;
mod edit;
mod panel;
mod perm;
mod popup;
mod show;

pub(super) fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(super) fn ctrl_c() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
}
