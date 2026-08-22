use crate::tui::oil::app::ViewContext;

/// The per-frame render inputs that a component needs, without the focus or
/// the theme borrows.
#[derive(Debug, Clone, Copy)]
pub struct RenderState {
    pub terminal_width: u16,
    pub spinner_frame: usize,
    pub show_thinking: bool,
    pub show_diffs: bool,
}

impl RenderState {
    #[inline]
    pub fn width(self) -> usize {
        self.terminal_width as usize
    }
}

impl From<&ViewContext<'_>> for RenderState {
    fn from(ctx: &ViewContext<'_>) -> Self {
        Self {
            terminal_width: ctx.terminal_size.0,
            spinner_frame: ctx.spinner_frame,
            show_thinking: ctx.show_thinking,
            show_diffs: ctx.show_diffs,
        }
    }
}
