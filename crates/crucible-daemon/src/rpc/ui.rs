//! `ui.config` — delivers Lua-defined UI configuration to an attached client.
//!
//! This is the transport that makes `crucible.theme.setup()` and
//! `runtime/themes/*.lua` real. Before it existed, both parsed correctly into a
//! process-global inside `crucible-lua` that only a *same-process* reader could
//! see — so in the normal split (TUI process ↔ daemon process) the TUI read its
//! own empty global and always fell back to a hardcoded default. The theme path
//! was worse still: nothing ever copied the parsed theme into the TUI's active
//! theme, so it was dead even under `--standalone`.
//!
//! ## Shape
//!
//! This handler serves the *snapshot* half of a handshake-plus-stream contract,
//! modelled on Neovim's `ui_send_all_hls` (full state at attach) followed by
//! diffed `hl_attr_define` pushes on `highlight_changed`.
//!
//! The snapshot must never be a precondition for rendering. Clients hold a
//! complete compiled-in default and treat this payload as an *upgrade* that may
//! arrive late, fail, or never arrive — a daemon that is unreachable degrades to
//! an unstyled-but-correct TUI, never to a blank screen.
//!
//! ## Why request params are ignored
//!
//! Clients send their terminal `background` and `color_depth`. The daemon
//! deliberately does nothing with them: colors cross the wire **unresolved**, and
//! the client resolves adaptive pairs against its own terminal. A daemon cannot
//! know the terminal a client is attached to — a remote one certainly cannot —
//! so resolving here would be wrong, not merely redundant. The fields are
//! accepted (and ignored) so the handshake shape is fixed from v1; they get read
//! when capability negotiation has an actual use site.

use crate::rpc::context::RpcContext;
use crucible_core::protocol::Request;
use crucible_lua::theme_wire::{theme_to_wire, UI_CONFIG_VERSION};

/// Build the `ui.config` snapshot.
///
/// Falls back to the built-in dark theme when Lua has not populated the config
/// store — a complete, coherent theme, never a partial one.
pub fn handle_ui_config(_ctx: &RpcContext, _req: &Request) -> serde_json::Value {
    let theme = crucible_lua::get_theme_config()
        .unwrap_or_else(crucible_lua::theme::ThemeConfig::default_dark);

    serde_json::json!({
        "version": UI_CONFIG_VERSION,
        "theme": theme_to_wire(&theme),
        "hl": crucible_lua::hl_lua::registry_to_wire(&crucible_lua::config::get_hl_registry()),
        "ui": crucible_lua::ui_geometry::geometry_to_wire(
            &crucible_lua::config::get_ui_geometry().unwrap_or_default(),
        ),
        "bars": crucible_lua::statusline_items::bars_to_wire(
            &crucible_lua::config::get_status_bars()
                .unwrap_or_else(crucible_lua::statusline_items::builtin_default),
        ),
    })
}
