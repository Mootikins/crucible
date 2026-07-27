//! `ui.config` — delivers Lua-defined UI configuration to an attached client.
//!
//! This is the transport that makes `crucible.colorscheme.setup()` and
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
pub fn handle_ui_config(ctx: &RpcContext, req: &Request) -> serde_json::Value {
    let session_id = req
        .params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    style_payload(&ctx.agents, session_id)
}

/// Switch the active theme by name.
///
/// Themes live in `<config>/crucible/themes/<name>.lua` as pure data tables, so
/// loading one is evaluating a literal — no plugin surface is involved. On
/// success the new palette is broadcast, which is what makes the switch appear
/// in every attached client rather than only the one that asked.
pub fn handle_ui_set_theme(ctx: &RpcContext, req: &Request) -> Result<serde_json::Value, String> {
    let name = req
        .params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "ui.set_theme requires a 'name'".to_string())?;

    // Reject separators outright rather than sanitizing: a theme name is a bare
    // stem by construction, so anything with a path in it is a mistake or an
    // attempt to read an arbitrary file.
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(format!("invalid theme name '{name}'"));
    }

    let config_dir = dirs::config_dir()
        .ok_or_else(|| "no config directory".to_string())?
        .join("crucible");
    let path = config_dir.join("themes").join(format!("{name}.lua"));

    let source = std::fs::read_to_string(&path).map_err(|e| {
        let available = crucible_lua::list_available_themes(&config_dir);
        if available.is_empty() {
            format!("theme '{name}' not found ({e})")
        } else {
            format!(
                "theme '{name}' not found; available: {}",
                available.join(", ")
            )
        }
    })?;

    let theme = crucible_lua::theme::load_theme_from_lua(&source)
        .map_err(|e| format!("theme '{name}' failed to load: {e}"))?;

    let applied = theme.name.clone();
    crucible_lua::config::set_theme_config_public(theme);

    crate::server::ui_broadcast::broadcast_style_changed(
        &ctx.event_tx,
        &ctx.agents,
        crate::server::ui_broadcast::GLOBAL,
    );
    Ok(serde_json::json!({ "theme": applied }))
}

/// Just the expression values, for the push that follows one changing.
///
/// Deliberately not [`style_payload`]: a provider can push on every file
/// change, and re-sending the theme, highlight table, geometry and layout each
/// time would have the client reinstall four stores it already has — each of
/// which is intentionally leaked so `active()` can return `&\'static`. Values
/// live in a plain lock, so this payload costs nothing to apply repeatedly.
pub fn expr_payload(
    agents: &crate::agent_manager::AgentManager,
    session_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "version": UI_CONFIG_VERSION,
        "exprs": agents.statusline_exprs().snapshot(session_id),
    })
}

/// The style payload, shared by the `ui.config` snapshot and the
/// `ui_style_changed` push.
///
/// One builder so the two cannot drift — a client applies both with the same
/// parser, which is what makes hot reload use the attach path rather than a
/// second, subtly different one.
pub fn style_payload(
    agents: &crate::agent_manager::AgentManager,
    session_id: &str,
) -> serde_json::Value {
    // Current expression values ride along, so a TUI attaching after a provider
    // has already pushed is not blank until the next change.
    let exprs = agents.statusline_exprs().snapshot(session_id);

    let theme = crucible_lua::get_theme_config()
        .unwrap_or_else(crucible_lua::theme::ThemeConfig::default_dark);

    serde_json::json!({
        "version": UI_CONFIG_VERSION,
        "theme": theme_to_wire(&theme),
        "hl": crucible_lua::hl_lua::registry_to_wire(&crucible_lua::config::get_hl_registry()),
        "ui": crucible_lua::ui_geometry::geometry_to_wire(
            &crucible_lua::config::get_ui_geometry().unwrap_or_default(),
        ),
        "exprs": exprs,
        "syntax": crucible_lua::config::get_syntax_config(),
        "layout": crucible_lua::config::get_layout()
            .unwrap_or_else(crucible_lua::statusline_items::builtin_default)
            .to_wire(),
    })
}
