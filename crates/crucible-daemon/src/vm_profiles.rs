//! The VM shapes Crucible builds, and the definitions file each one earns.
//!
//! One VM runs Lua files: the daemon VM, which loads every plugin, the shipped
//! defaults file and the user's `init.lua`, in that order. The other profiles
//! are not VMs a session owns — they are the deliberately tiny states a
//! statusline layout and a theme file are evaluated in.
//!
//! | profile | built by | what runs on it |
//! |---|---|---|
//! | [`VmProfile::Daemon`] | [`crate::daemon_plugins::DaemonPluginLoader::new`] | every plugin, `runtime/defaults/init.lua`, the user's `init.lua` |
//! | [`VmProfile::Statusline`] | [`crucible_lua::statusline_lua::statusline_vm`] | a statusline layout file |
//! | [`VmProfile::Theme`] | a bare `Lua::new()` | a theme file |
//!
//! Each profile builds its VM with the same functions production calls, and
//! the definitions come out of that VM's own `HostSignatures` — never from a
//! table that hopes to match.

use mlua::Lua;

/// One VM shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VmProfile {
    /// Plugins, the shipped defaults file, and the user's `init.lua`.
    Daemon,
    /// A statusline layout file.
    Statusline,
    /// A theme file.
    Theme,
}

impl VmProfile {
    /// Every profile, for a caller that renders or checks all of them.
    pub fn all() -> [VmProfile; 3] {
        [VmProfile::Daemon, VmProfile::Statusline, VmProfile::Theme]
    }

    /// The definitions file this profile renders to.
    pub fn definitions_file(&self) -> &'static str {
        match self {
            VmProfile::Daemon => "cru.d.luau",
            VmProfile::Statusline => "cru-statusline.d.luau",
            VmProfile::Theme => "cru-theme.d.luau",
        }
    }

    /// The name used in messages.
    pub fn name(&self) -> &'static str {
        match self {
            VmProfile::Daemon => "daemon",
            VmProfile::Statusline => "statusline",
            VmProfile::Theme => "theme",
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;

/// Build a VM carrying the STATUSLINE profile's surface.
///
/// Delegates to the function `default_layout_from_lua` uses, so the checked
/// surface cannot drift from the evaluated one. A statusline file used to be
/// checked against the config profile, which also had `cru.colorscheme`,
/// `cru.hl` and `cru.syntax` — the gate proved a property of a strictly more
/// permissive VM, and a layout reaching for `cru.hl` passed the check and
/// failed to load.
pub fn statusline_vm() -> anyhow::Result<Lua> {
    crucible_lua::statusline_lua::statusline_vm().map_err(|e| anyhow::anyhow!("statusline vm: {e}"))
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
/// writes the editor index and the doc map for it. The other two — statusline
/// and theme — are checked, not authored against, so they need declarations
/// only.
pub fn write_other_definitions(output_dir: &std::path::Path) -> anyhow::Result<()> {
    for profile in VmProfile::all() {
        let lua = match profile {
            VmProfile::Daemon => continue,
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
