//! The VM shapes Crucible builds, and the definitions file each one earns.
//!
//! Crucible does not run one Lua VM. It runs three, and they do not carry the
//! same `cru.*` surface:
//!
//! | profile | built by | what runs on it |
//! |---|---|---|
//! | [`VmProfile::Daemon`] | [`crate::daemon_plugins::DaemonPluginLoader::new`] | every plugin, and the user's `init.lua` |
//! | [`VmProfile::Session`] | `AgentManager::build_session_state` | `runtime/defaults/init.lua` |
//! | [`VmProfile::Config`] | [`crucible_lua::config::LuaConfigLoader`] | the CLI's config read (`cru config`, `cru doctor`) |
//!
//! One definitions file used to describe the daemon profile and stand in for
//! all three. `runtime/defaults/init.lua` runs on the SESSION VM and uses
//! `cru.modes`, `cru.defaults` and `cru.permissions`, none of which the daemon
//! VM has — so the shipped defaults reported five type errors against the only
//! stub file Crucible published, for API that works perfectly.
//!
//! Prior art disagrees about whether to unify. Neovim runs ONE `lua_State` for
//! `init.lua` and every plugin, and publishes ONE generated definitions set
//! (`runtime/lua/vim/_meta`); it has no isolation at all as a result. WezTerm —
//! the same Rust plus mlua stack as this crate — cannot unify, because its Lua
//! files "may be re-loaded and re-evaluated multiple times in different
//! contexts or in different threads", and it added `wezterm.GLOBAL` to carry
//! state across the boundary. `mlua::Lua` is `Send` but not `Sync`, which is
//! the same constraint here, and a session VM is 1:1 with its session by
//! design.
//!
//! So: one REGISTRY, several profiles. Take Neovim's single generated surface
//! and WezTerm's several VMs. Each profile below builds its VM with the same
//! functions production calls, and the definitions come out of that VM's own
//! `HostSignatures` — never from a table that hopes to match.

use mlua::Lua;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

/// One VM shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VmProfile {
    /// Plugins and the user's `init.lua`.
    Daemon,
    /// Per-session files: the shipped defaults and a workspace `init.lua`.
    Session,
    /// The CLI's standalone config read.
    Config,
    /// A statusline layout file.
    Statusline,
    /// A theme file.
    Theme,
}

impl VmProfile {
    /// Every profile, for a caller that renders or checks all of them.
    pub fn all() -> [VmProfile; 5] {
        [
            VmProfile::Daemon,
            VmProfile::Session,
            VmProfile::Config,
            VmProfile::Statusline,
            VmProfile::Theme,
        ]
    }

    /// The definitions file this profile renders to.
    pub fn definitions_file(&self) -> &'static str {
        match self {
            VmProfile::Daemon => "cru.d.luau",
            VmProfile::Session => "cru-session.d.luau",
            VmProfile::Config => "cru-config.d.luau",
            VmProfile::Statusline => "cru-statusline.d.luau",
            VmProfile::Theme => "cru-theme.d.luau",
        }
    }

    /// The name used in messages.
    pub fn name(&self) -> &'static str {
        match self {
            VmProfile::Daemon => "daemon",
            VmProfile::Session => "session",
            VmProfile::Config => "config",
            VmProfile::Statusline => "statusline",
            VmProfile::Theme => "theme",
        }
    }
}

/// Build a VM carrying the SESSION profile's surface.
///
/// Registers exactly what `AgentManager::build_session_state` registers, with
/// fresh registries and no session bound. The behaviour that follows there —
/// loading the shipped defaults, then the workspace `init.lua` — is not
/// surface, so it is not repeated here.
///
/// Held to the real thing by `the_session_profile_matches_a_real_session_vm`,
/// which compares this VM's `cru.*` paths against a session VM that
/// `AgentManager` actually built. Add a registration there and the test fails
/// here until this function grows it too.
pub fn session_vm() -> anyhow::Result<Lua> {
    let lua = Lua::new();
    let registry = crucible_lua::LuaScriptHandlerRegistry::new();

    crucible_lua::register_cru_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )?;
    crucible_lua::register_permission_hook_api(
        &lua,
        Arc::new(StdMutex::new(Vec::new())),
        Arc::new(StdMutex::new(HashMap::new())),
    )?;
    crucible_lua::register_session_defaults(&lua, crucible_lua::SessionDefaults::new())?;
    crucible_lua::register_modes(&lua, crucible_lua::ModeRegistry::new())?;

    let cru = crucible_lua::lua_util::get_or_create_namespace(&lua, "cru")
        .map_err(|e| anyhow::anyhow!("cru namespace: {e}"))?;
    crucible_lua::register_hooks_module(&lua, &cru)?;
    // `cru.log` and `cru.log.notify`, as `build_session_state` registers them
    // before it installs the session's notify sink.
    crucible_lua::register_log_function(&lua, &cru)?;
    crucible_lua::register_notify_module(&lua, &cru)?;

    // `AgentManager` uses the default budget too. Nothing here reads it; a
    // registry is required to register the API at all.
    crucible_lua::register_context_attach(
        &lua,
        Arc::new(crucible_lua::ContextAttachRegistry::default()),
    )
    .map_err(|e| anyhow::anyhow!("cru.context.attach: {e}"))?;

    // `cru.session.*` is bound to a live session on the real VM. The table
    // SHAPE is the same either way, and the shape is what a definitions file
    // states.
    crucible_lua::register_sessions_module(&lua)
        .map_err(|e| anyhow::anyhow!("cru.session: {e}"))?;

    crucible_lua::register_statusline_exprs(
        &lua,
        &cru,
        Arc::new(crucible_lua::StatuslineExprRegistry::new()),
    )
    .map_err(|e| anyhow::anyhow!("cru.statusline: {e}"))?;

    Ok(lua)
}

/// Build a VM carrying the CONFIG profile's surface.
///
/// The CLI reads `init.lua` for its settings without a daemon, so this VM has
/// the UI and config namespaces and NOT `http`, `fs` or `shell`. That absence
/// is deliberate: `cru config` and `cru doctor` evaluate a config file to read
/// values out of it, and a config read should not be able to reach the network
/// or the filesystem.
pub fn config_vm() -> anyhow::Result<Lua> {
    let lua = Lua::new();
    crucible_lua::config::register_ui_namespaces(&lua)
        .map_err(|e| anyhow::anyhow!("ui namespaces: {e}"))?;
    let cru = crucible_lua::lua_util::get_or_create_namespace(&lua, "cru")
        .map_err(|e| anyhow::anyhow!("cru namespace: {e}"))?;
    crucible_lua::config::register_app_config_api(&lua, &cru)
        .map_err(|e| anyhow::anyhow!("cru.config: {e}"))?;
    Ok(lua)
}

#[cfg(test)]
pub(crate) mod tests;

/// Build a VM carrying the STATUSLINE profile's surface.
///
/// `statusline_lua::default_layout_from_lua` builds exactly this: a bare VM
/// with `cru.statusline` and its item constructors, and nothing else. A
/// statusline file used to be checked against the CONFIG profile, which has
/// `cru.colorscheme`, `cru.hl`, `cru.syntax` and `cru.config` on it too — so
/// the gate proved a property of a VM strictly more permissive than the one
/// that runs the file, and a layout reaching for `cru.hl` would have passed
/// the check and failed to load.
pub fn statusline_vm() -> anyhow::Result<Lua> {
    let lua = Lua::new();
    let cru = lua.create_table()?;
    let statusline = lua.create_table()?;
    crucible_lua::register_statusline_items(&lua, &statusline)
        .map_err(|e| anyhow::anyhow!("cru.statusline items: {e}"))?;
    cru.set("statusline", statusline)?;
    lua.globals().set("cru", cru)?;
    Ok(lua)
}

/// Build a VM carrying the THEME profile's surface, which is NOTHING.
///
/// `theme::load_theme_from_lua` evaluates a theme with a bare `Lua::new()`.
/// There is no `cru` table at all: a theme file returns a table of colours and
/// calls nothing. Checking one against the config profile said `cru.hl.set`
/// was available; it is not, and a theme using it raises
/// "attempt to index nil with 'hl'" at load.
pub fn theme_vm() -> anyhow::Result<Lua> {
    Ok(Lua::new())
}

/// Write the definitions file for every profile OTHER than the daemon one.
///
/// The daemon profile is written by
/// [`crate::daemon_plugins::DaemonPluginLoader::generate_stubs`], which also
/// writes the editor index and the doc map for it. The other four — session,
/// config, statusline and theme — are checked, not authored against, so they
/// need declarations only.
pub fn write_other_definitions(output_dir: &std::path::Path) -> anyhow::Result<()> {
    for profile in VmProfile::all() {
        let lua = match profile {
            VmProfile::Daemon => continue,
            VmProfile::Session => session_vm()?,
            VmProfile::Config => config_vm()?,
            VmProfile::Statusline => statusline_vm()?,
            VmProfile::Theme => theme_vm()?,
        };
        crucible_lua::stubs::StubGenerator::write_declarations(
            &lua,
            &output_dir.join(profile.definitions_file()),
        )
        .map_err(|e| anyhow::anyhow!("{} definitions: {e}", profile.name()))?;
    }
    Ok(())
}
