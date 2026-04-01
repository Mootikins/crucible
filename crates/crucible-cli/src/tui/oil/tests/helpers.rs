use crucible_oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crucible_oil::focus::FocusContext;
use crate::tui::oil::Node;

use super::generators::RpcEvent;
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

pub fn apply_rpc_event(app: &mut OilChatApp, event: &RpcEvent) {
    match event {
        RpcEvent::TextDelta(text) => {
            app.on_message(ChatAppMsg::TextDelta(text.clone()));
        }
        RpcEvent::ThinkingDelta(text) => {
            app.on_message(ChatAppMsg::ThinkingDelta(text.clone()));
        }
        RpcEvent::ToolCall { name, args } => {
            app.on_message(ChatAppMsg::ToolCall {
                name: name.clone(),
                args: args.clone(),
                call_id: None,
                description: None,
                source: None,
                lua_primary_arg: None,
            });
        }
        RpcEvent::ToolResultDelta { name, delta } => {
            app.on_message(ChatAppMsg::ToolResultDelta {
                name: name.clone(),
                delta: delta.clone(),
                call_id: None,
            });
        }
        RpcEvent::ToolResultComplete { name } => {
            app.on_message(ChatAppMsg::ToolResultComplete {
                name: name.clone(),
                call_id: None,
            });
        }
    }
}
