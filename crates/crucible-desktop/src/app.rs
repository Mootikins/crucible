//! Application shell and entry point
//!
//! Sets up the GPUI application, window, and root view.

use anyhow::Result;
use gpui::prelude::*;
use gpui::{px, size, Application, Bounds, WindowBounds, WindowOptions};

use crate::chat::ChatView;

/// Run the desktop application
pub fn run_app() -> Result<()> {
    Application::new().run(|cx| {
        // Initialize gpui-component (includes theme)
        gpui_component::init(cx);

        // Open main window
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(800.0), px(600.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| ChatView::new(window, cx)),
        )
        .expect("Failed to open window");

        cx.activate(true);
    });

    Ok(())
}
