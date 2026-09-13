//! The closed set of top-level names on the `cru` global.
//!
//! `cru` is the ONE Lua global (the old `crucible` global is deleted). Every
//! name a VM hangs off it is a variant here, so adding a namespace is a
//! reviewable event rather than a side effect of registration. The gate is
//! `crucible-daemon/tests/cru_namespace_gate.rs`, which derives the expected
//! set from a running plugin VM and compares it against the variant walk
//! in both directions — the `tools/surface.rs` pattern applied to the
//! namespace itself.
//!
//! The set is now complete: nothing hangs a loader-internal marker off `cru`.
//! The two that used to (`cru._current_plugin` and
//! `cru._current_plugin_may_intercept`) were forgeable authority markers and
//! live in Rust-side app data — see [`crate::plugin_context`]. The gate
//! therefore compares EVERY live key, with no prefix exemption.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use strum::IntoEnumIterator;

/// One top-level key on the `cru` global.
///
/// The serialized form (via `IntoStaticStr`) is the Lua name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter, strum::IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum CruNamespace {
    Check,
    /// Retires registrations the calling plugin made: `cru.clear{ … }`.
    ///
    /// Registered beside [`CruNamespace::On`], so it lands wherever that
    /// does. The pair write and unwrite one store, and a VM carrying
    /// `cru.on` without `cru.clear` could register a handler that nothing
    /// could retire.
    Clear,
    Colorscheme,
    Config,
    Context,
    /// The kiln's own embedding provider: `cru.embed(kiln, text)`.
    Embed,
    Emitter,
    Errors,
    Fs,
    Geometry,
    GetSession,
    Health,
    Hl,
    Http,
    /// Config-loader VMs only (`ConfigLoader::load`): `cru.include(path)`.
    Include,
    Inspect,
    Isolation,
    Json,
    Kiln,
    Log,
    /// The crate-local stub-generator VM only; the plugin VM has no MCP
    /// client API of its own.
    Mcp,
    /// The mode registry, on the daemon VM.
    Modes,
    Oil,
    On,
    OnProviderAuth,
    OnSessionEnd,
    OnSessionStart,
    Oq,
    Paths,
    /// `cru.permissions.on_request`, on the VM that runs Lua files.
    Permissions,
    Plugin,
    Ratelimit,
    Retry,
    /// `cru.rtp.append/prepend/get` — sugar over a `runtimepath` write.
    ///
    /// Registered beside `cru.config`, so it lands wherever that does. It
    /// needs no VM restriction of its own: `runtimepath` is a location key,
    /// and `ConfigStore`'s policy accepts one only during the boot phase, so
    /// a call from a plugin after boot is withheld exactly as a direct
    /// `cru.config.set` would be.
    Rtp,
    Schedule,
    Service,
    /// The canonical session module: lifecycle verbs plus `current`.
    Session,
    /// The deprecated plural alias over [`CruNamespace::Session`]; removed
    /// with the alias after the deprecation window.
    Sessions,
    Shell,
    Statusline,
    Storage,
    /// Panels a plugin declares for every client to draw: `cru.surface.*`.
    Surface,
    Syntax,
    TblDeepExtend,
    TblGet,
    Timer,
    Tools,
    Ui,
    /// Vector geometry over plain number arrays: `cru.vec.arc_best`.
    Vec,
    Ws,
}

impl CruNamespace {
    /// The Lua key for this namespace.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// Whether the daemon's plugin VM carries this name after boot
    /// (`DaemonPluginLoader::new` plus the UI-config registration that
    /// `Server::boot_plugins` performs).
    ///
    /// Exhaustive on purpose: a new variant does not compile until its VM
    /// placement is stated, and the gate then proves the statement.
    pub fn on_plugin_vm(self) -> bool {
        match self {
            Self::Include | Self::Mcp => false,
            Self::Check
            | Self::Clear
            | Self::Colorscheme
            | Self::Config
            | Self::Context
            | Self::Embed
            | Self::Emitter
            | Self::Errors
            | Self::Fs
            | Self::Geometry
            | Self::GetSession
            | Self::Health
            | Self::Hl
            | Self::Http
            | Self::Inspect
            | Self::Isolation
            | Self::Json
            | Self::Kiln
            | Self::Log
            | Self::Modes
            | Self::Oil
            | Self::On
            | Self::OnProviderAuth
            | Self::OnSessionEnd
            | Self::OnSessionStart
            | Self::Oq
            | Self::Paths
            | Self::Permissions
            | Self::Plugin
            | Self::Ratelimit
            | Self::Retry
            | Self::Rtp
            | Self::Schedule
            | Self::Service
            | Self::Session
            | Self::Sessions
            | Self::Shell
            | Self::Statusline
            | Self::Storage
            | Self::Surface
            | Self::Syntax
            | Self::TblDeepExtend
            | Self::TblGet
            | Self::Timer
            | Self::Tools
            | Self::Ui
            | Self::Vec
            | Self::Ws => true,
        }
    }

    /// Every declared namespace name, by walking `strum::EnumIter` — which is
    /// what the compiler knows, not what a hand-kept list says.
    pub fn all_names() -> impl Iterator<Item = &'static str> {
        Self::iter().map(Self::name)
    }
}
