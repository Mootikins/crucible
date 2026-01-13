use crate::tui::ink::event::Event;
use crate::tui::ink::node::Node;
use std::time::Duration;

pub trait App: Sized {
    type Msg: Send + 'static;

    fn init() -> Self;

    fn view(&self) -> Node;

    fn update(&mut self, event: Event) -> Action<Self::Msg>;

    fn on_message(&mut self, _msg: Self::Msg) -> Action<Self::Msg> {
        Action::Continue
    }

    fn tick_rate(&self) -> Option<Duration> {
        None
    }
}

#[derive(Debug)]
pub enum Action<M> {
    Continue,
    Quit,
    Send(M),
    Batch(Vec<Action<M>>),
}

impl<M> Action<M> {
    pub fn is_quit(&self) -> bool {
        matches!(self, Action::Quit)
    }
}

impl<M> Default for Action<M> {
    fn default() -> Self {
        Action::Continue
    }
}
