use crate::tui::ink::event::Event;
use crate::tui::ink::focus::FocusContext;
use crate::tui::ink::node::Node;
use std::time::Duration;

pub struct ViewContext<'a> {
    pub focus: &'a FocusContext,
}

impl<'a> ViewContext<'a> {
    pub fn new(focus: &'a FocusContext) -> Self {
        Self { focus }
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focus.is_focused(id)
    }
}

pub trait App: Sized {
    type Msg: Send + 'static;

    fn init() -> Self;

    fn view(&self, ctx: &ViewContext<'_>) -> Node;

    fn update(&mut self, event: Event) -> Action<Self::Msg>;

    fn on_message(&mut self, _msg: Self::Msg) -> Action<Self::Msg> {
        Action::Continue
    }

    fn tick_rate(&self) -> Option<Duration> {
        None
    }
}

#[derive(Debug, Default)]
pub enum Action<M> {
    #[default]
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
