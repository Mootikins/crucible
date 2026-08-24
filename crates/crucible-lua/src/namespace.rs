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
//! Names with a leading underscore (`cru._current_plugin`,
//! `cru._current_plugin_may_intercept`) are loader-internal markers, not API,
//! and are deliberately not variants; the gate skips them by prefix.

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
    Colorscheme,
    Config,
    Context,
    /// Session VMs only: seeded session defaults (`cru.defaults.x = …`).
    Defaults,
    Emitter,
    Errors,
    Fmt,
    Fs,
    Geometry,
    GetSession,
    Graph,
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
    /// Session VMs only: the mode registry.
    Modes,
    Oil,
    On,
    OnProviderAuth,
    OnSessionEnd,
    OnSessionStart,
    Oq,
    Paths,
    /// Session VMs only: `cru.permissions.on_request`.
    Permissions,
    Plugin,
    Ratelimit,
    Retry,
    Schedule,
    Service,
    Sessions,
    Shell,
    Spawn,
    Statusline,
    Storage,
    Syntax,
    TblDeepExtend,
    TblGet,
    Timer,
    Tools,
    Ui,
    Ws,
}

impl CruNamespace {
    /// The Lua key for this namespace.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// Whether the daemon's plugin VM carries this name after boot
    /// (`DaemonPluginLoader::new` plus the UI-config registration that
    /// `plugin_boot` performs).
    ///
    /// Exhaustive on purpose: a new variant does not compile until its VM
    /// placement is stated, and the gate then proves the statement.
    pub fn on_plugin_vm(self) -> bool {
        match self {
            Self::Defaults | Self::Include | Self::Mcp | Self::Modes | Self::Permissions => false,
            Self::Check
            | Self::Colorscheme
            | Self::Config
            | Self::Context
            | Self::Emitter
            | Self::Errors
            | Self::Fmt
            | Self::Fs
            | Self::Geometry
            | Self::GetSession
            | Self::Graph
            | Self::Health
            | Self::Hl
            | Self::Http
            | Self::Inspect
            | Self::Isolation
            | Self::Json
            | Self::Kiln
            | Self::Log
            | Self::Oil
            | Self::On
            | Self::OnProviderAuth
            | Self::OnSessionEnd
            | Self::OnSessionStart
            | Self::Oq
            | Self::Paths
            | Self::Plugin
            | Self::Ratelimit
            | Self::Retry
            | Self::Schedule
            | Self::Service
            | Self::Sessions
            | Self::Shell
            | Self::Spawn
            | Self::Statusline
            | Self::Storage
            | Self::Syntax
            | Self::TblDeepExtend
            | Self::TblGet
            | Self::Timer
            | Self::Tools
            | Self::Ui
            | Self::Ws => true,
        }
    }

    /// Every declared namespace name, by walking `strum::EnumIter` — which is
    /// what the compiler knows, not what a hand-kept list says.
    pub fn all_names() -> impl Iterator<Item = &'static str> {
        Self::iter().map(Self::name)
    }
}
