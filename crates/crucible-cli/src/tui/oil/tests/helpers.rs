use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::Node;
use crate::tui::oil::TestRuntime;

use super::generators::RpcEvent;

pub fn view_with_default_ctx(app: &OilChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

pub fn render_app(app: &OilChatApp, width: usize) -> String {
    let tree = view_with_default_ctx(app);
    render_to_string(&tree, width)
}

pub fn render_and_strip(app: &OilChatApp, width: usize) -> String {
    strip_ansi(&render_app(app, width))
}

pub fn combined_output(runtime: &TestRuntime) -> String {
    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());
    format!("{}{}", stdout, viewport)
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
