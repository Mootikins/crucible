use crate::tui::oil::app::ViewContext;
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

    // Reserve the popup's rows before the frame is measured. The popup draws
    // over the rows above the prompt, so without the reserve a short
    // transcript makes the frame grow the moment a completion opens.
    renderer.set_min_viewport_rows(app.min_viewport_rows(&ctx));

    // A tool that has outrun the split threshold leaves the transcript before
    // the frame is built, so no node in the tree can still mutate.
    app.split_slow_tools();

    // No graduation: the renderer emits the whole transcript and the terminal
    // owns the scroll. A row that scrolls off the top stays in the terminal's
    // scrollback, and a resize reprints the transcript from these same nodes.
    let tree = app.view(&ctx);
    renderer.render_frame(&tree, None);
}

impl OilChatRunner {
    pub(super) fn render_app_frame(&mut self, app: &mut OilChatApp) -> Result<()> {
        // The one place the TUI reads the wall clock for a frame.
        app.set_frame_time(std::time::Instant::now());
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
