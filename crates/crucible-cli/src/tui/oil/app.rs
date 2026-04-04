use crate::tui::oil::event::Event;
use crucible_oil::focus::FocusContext;
use crucible_oil::node::Node;
use crate::tui::oil::theme::{self, ThemeConfig};
use std::time::Duration;

pub struct ViewContext<'a> {
    pub focus: &'a FocusContext,
    pub theme: &'a ThemeConfig,
    pub terminal_size: (u16, u16),
    pub spinner_frame: usize,
    pub show_thinking: bool,
}

impl<'a> ViewContext<'a> {
    pub fn new(focus: &'a FocusContext) -> Self {
        Self {
            focus,
            theme: theme::active(),
            terminal_size: (80, 24),
            spinner_frame: 0,
            show_thinking: false,
        }
    }

    pub fn with_theme(focus: &'a FocusContext, theme: &'a ThemeConfig) -> Self {
        Self {
            focus,
            theme,
            terminal_size: (80, 24),
            spinner_frame: 0,
            show_thinking: false,
        }
    }

    pub fn with_terminal_size(
        focus: &'a FocusContext,
        theme: &'a ThemeConfig,
        terminal_size: (u16, u16),
    ) -> Self {
        Self {
            focus,
            theme,
            terminal_size,
            spinner_frame: 0,
            show_thinking: false,
        }
    }

    /// Terminal width as usize.
    pub fn width(&self) -> usize {
        self.terminal_size.0 as usize
    }

    pub fn theme(&self) -> &ThemeConfig {
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
