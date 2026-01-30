use crate::tui::oil::event::Event;
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::node::Node;
use crate::tui::oil::theme::ThemeTokens;
use std::time::Duration;

pub struct ViewContext<'a> {
    pub focus: &'a FocusContext,
    pub theme: &'a ThemeTokens,
    pub terminal_size: (u16, u16),
}

impl<'a> ViewContext<'a> {
    pub fn new(focus: &'a FocusContext) -> Self {
        Self {
            focus,
            theme: ThemeTokens::default_ref(),
            terminal_size: (80, 24),
        }
    }

    pub fn with_theme(focus: &'a FocusContext, theme: &'a ThemeTokens) -> Self {
        Self {
            focus,
            theme,
            terminal_size: (80, 24),
        }
    }

    pub fn with_terminal_size(
        focus: &'a FocusContext,
        theme: &'a ThemeTokens,
        terminal_size: (u16, u16),
    ) -> Self {
        Self {
            focus,
            theme,
            terminal_size,
        }
    }

    pub fn theme(&self) -> &ThemeTokens {
        self.theme
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
