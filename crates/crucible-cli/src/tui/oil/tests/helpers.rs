#![allow(dead_code)] // helpers used by disabled test modules awaiting reconstruction

use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::Node;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::focus::FocusContext;

use super::vt100_runtime::Vt100TestRuntime;

pub fn view_with_default_ctx(app: &OilChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

/// Render app through the real terminal path (Terminal<Vec<u8>> → vt100)
/// and return stripped screen contents. This is the canonical test render
/// function — it exercises the same code path as production.
pub fn vt_render(app: &mut OilChatApp) -> String {
    vt_render_sized(app, 80, 24)
}

/// Like vt_render but with custom terminal dimensions.
pub fn vt_render_sized(app: &mut OilChatApp, width: u16, height: u16) -> String {
    let mut vt = Vt100TestRuntime::new(width, height);
    vt.render_frame(app);
    strip_ansi(&vt.screen_contents())
}
