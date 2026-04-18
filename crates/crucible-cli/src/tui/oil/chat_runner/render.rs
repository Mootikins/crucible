use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::theme;
use anyhow::Result;
use crucible_oil::focus::FocusContext;
use crucible_oil::FrameRenderer;

use super::OilChatRunner;

/// Render one frame through the shared FrameRenderer trait.
///
/// This is the single rendering function used by all paths:
/// - Live TUI (via Terminal)
/// - Fixture tests (via TestRuntime)
/// - Replay (via Terminal, same as live)
///
/// Handles: full redraw detection, scroll offset sync, view building,
/// rendering, and graduation feedback.
pub fn render_frame(app: &mut OilChatApp, renderer: &mut impl FrameRenderer, focus: &FocusContext) {
    if app.take_needs_full_redraw() {
        renderer.force_full_redraw();
    }

    // Expire toast notifications (previously done on Event::Tick)
    app.expire_toasts();

    // Build ViewContext first — needed for both graduation and viewport rendering
    let terminal_size = renderer.size();
    let ctx = ViewContext::with_terminal_size(focus, theme::active(), terminal_size);

    // Drain completed containers → stdout (terminal scrollback)
    let graduation = app.drain_graduated(&ctx);
    let tree = app.view(&ctx);
    renderer.render_frame(&tree, graduation.as_ref());
}

impl OilChatRunner {
    pub(super) fn render_app_frame(&mut self, app: &mut OilChatApp) -> Result<()> {
        if app.has_shell_modal() {
            // Shell modal uses fullscreen rendering (Terminal-specific)
            if app.take_needs_full_redraw() {
                self.terminal.force_full_redraw()?;
            }
            let terminal_size = self.terminal.size();
            let ctx = ViewContext::with_terminal_size(&self.focus, theme::active(), terminal_size);
            let tree = app.view(&ctx);
            self.terminal.render_fullscreen(&tree)?;
        } else {
            // Normal rendering through the shared FrameRenderer trait
            render_frame(app, &mut self.terminal, &self.focus);
        }
        Ok(())
    }
}
