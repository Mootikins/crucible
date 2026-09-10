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

use crate::manifest::Capability;
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
    /// The canonical session module: lifecycle verbs plus `current`.
    Session,
    /// The deprecated plural alias over [`CruNamespace::Session`]; removed
    /// with the alias after the deprecation window.
    Sessions,
    Shell,
    Statusline,
    Storage,
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
            | Self::Session
            | Self::Sessions
            | Self::Shell
            | Self::Statusline
            | Self::Storage
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

    /// The grant a PLUGIN must declare before it may call into this
    /// namespace, or `None` when the namespace carries no authority.
    ///
    /// Exhaustive on purpose, and for the same reason `on_plugin_vm` is: a new
    /// namespace does not compile until someone states whether it is a door.
    /// The gate is `Ns::func` (`crate::host_registry`), which wraps every
    /// function it registers in a namespace that answers `Some` here — so a
    /// new function in `cru.http` cannot be added ungated, and the check runs
    /// BEFORE argument conversion so a refusal cannot be mistaken for a type
    /// error.
    ///
    /// Absent authority is the common case, and each `None` below is a claim,
    /// not an omission:
    ///
    /// - `Check`, `Errors`, `Inspect`, `Json`, `Oq`, `Vec`, `TblGet`,
    ///   `TblDeepExtend`, `Ratelimit`, `Retry`, `Emitter`, `Health`, `Oil` —
    ///   pure computation over values the caller already holds. `cru.oil`
    ///   BUILDS a node; drawing one is the renderer's act, not the plugin's.
    /// - `Log`, `Plugin`, `Storage`, `Paths` — the plugin's own channel, its
    ///   own publications, its own SQLite namespace and its own directories.
    ///   Each is already scoped to the running plugin, so a grant would gate a
    ///   plugin's access to itself.
    /// - `Timer`, `Schedule`, `Service` — deferral, not reach. What the
    ///   deferred body then calls is gated when it calls it, because the
    ///   registering plugin's context is carried into the callback.
    /// - `On`, `OnSessionStart`, `OnSessionEnd`, `OnProviderAuth` —
    ///   registration. The handler's own calls are gated, and `cancel` (all a
    ///   handler may do without `intercept_tools`) can only narrow.
    /// - `Defaults`, `Include`, `Mcp`, `Modes`, `Permissions` — not on the
    ///   plugin VM at all (see [`Self::on_plugin_vm`]), so no plugin can reach
    ///   them to be gated.
    /// - `System` — no surface. Nothing in `cru.*` answers the manifest's
    ///   "access system information", so the variant grants nothing. Recorded
    ///   here rather than mapped to the nearest-looking namespace.
    pub fn required_capability(self) -> Option<Capability> {
        match self {
            Self::Fs => Some(Capability::Filesystem),
            Self::Http => Some(Capability::Network),
            Self::Ws => Some(Capability::WebSocket),
            Self::Shell => Some(Capability::Shell),
            // `cru.embed(kiln, text)` runs the KILN's own embedding provider,
            // named by the same kiln name `cru.kiln.*` takes.
            Self::Kiln | Self::Embed => Some(Capability::Kiln),
            Self::Config => Some(Capability::Config),
            // The agent runtime: sessions (`Sessions` is the deprecated alias
            // over `Session`), the session a call runs in, that session's
            // context window, and the tools any of them may invoke —
            // `cru.tools.call` reaches `bash` itself.
            Self::Session | Self::Sessions | Self::GetSession | Self::Context | Self::Tools => {
                Some(Capability::Agent)
            }
            // What the user sees or is asked. `cru.ui` interrupts a person and
            // collects their answer; the rest replace the interface itself.
            Self::Ui
            | Self::Statusline
            | Self::Colorscheme
            | Self::Syntax
            | Self::Hl
            | Self::Geometry => Some(Capability::Ui),
            // `cru.isolation.require` is the DECLARATIVE half of taking a tool
            // call over: `exempt` widens what may still run on the host and
            // `exec_prefix` relocates execution into the caller's sandbox. A
            // plugin that may not intercept must not be able to assert either.
            Self::Isolation => Some(Capability::InterceptTools),
            Self::Check
            | Self::Defaults
            | Self::Emitter
            | Self::Errors
            | Self::Health
            | Self::Include
            | Self::Inspect
            | Self::Json
            | Self::Log
            | Self::Mcp
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
            | Self::Schedule
            | Self::Service
            | Self::Storage
            | Self::TblDeepExtend
            | Self::TblGet
            | Self::Timer
            | Self::Vec => None,
        }
    }

    /// The namespace a registered host function belongs to, from the path
    /// `Ns` was opened at and the member's own name.
    ///
    /// `Ns::over(lua, "cru", …)` registers functions that live directly on
    /// `cru` — `cru.embed`, `cru.get_session` — so the member name is the
    /// namespace there. Anything deeper (`cru.log.messages`) belongs to its
    /// first segment. An unknown name answers `None`, which is what makes a
    /// throwaway probe namespace in a test ungated.
    pub fn for_member(ns_path: &str, member: &str) -> Option<Self> {
        let key = match ns_path.strip_prefix("cru") {
            Some("") => member,
            Some(rest) => rest.strip_prefix('.')?.split('.').next()?,
            None => return None,
        };
        Self::iter().find(|ns| ns.name() == key)
    }

    /// Every declared namespace name, by walking `strum::EnumIter` — which is
    /// what the compiler knows, not what a hand-kept list says.
    pub fn all_names() -> impl Iterator<Item = &'static str> {
        Self::iter().map(Self::name)
    }
}

/// The capabilities that deliberately gate nothing.
///
/// A grant naming no surface is worse than absent: a plugin author declares
/// it, a reader believes something is checked, and nothing is. Listing it here
/// makes the emptiness a stated decision that a test can hold, rather than an
/// omission from an exhaustive match nobody re-read.
///
/// `System` is the manifest's "access system information", and no `cru.*`
/// namespace answers to that description. It parses, it grants nothing, and it
/// should be deleted rather than mapped to the nearest-looking namespace.
pub const GRANTS_NOTHING: &[Capability] = &[Capability::System];

#[cfg(test)]
mod tests {
    use super::*;

    /// The other half of the closed set: every capability a manifest can spell
    /// either gates a namespace or is declared to gate nothing.
    ///
    /// Without this, adding a `Capability` variant and forgetting to map it
    /// produces a grant a plugin can declare, a doc can list, and nothing can
    /// check — which is precisely the state all ten variants were in.
    #[test]
    fn every_capability_gates_a_namespace_or_says_it_gates_none() {
        for cap in <Capability as IntoEnumIterator>::iter() {
            let gated: Vec<&str> = CruNamespace::iter()
                .filter(|ns| ns.required_capability() == Some(cap))
                .map(CruNamespace::name)
                .collect();
            if GRANTS_NOTHING.contains(&cap) {
                assert!(
                    gated.is_empty(),
                    "'{cap}' is listed as granting nothing, but it gates {gated:?} — \
                     remove it from GRANTS_NOTHING"
                );
            } else {
                assert!(
                    !gated.is_empty(),
                    "'{cap}' gates no namespace. Map it in required_capability, or \
                     add it to GRANTS_NOTHING and say why"
                );
            }
        }
    }

    /// `InterceptTools` is enforced in two other places besides a namespace —
    /// the tool-call seam and the loader — so its namespace mapping must not
    /// be read as the whole of it.
    #[test]
    fn the_gated_namespaces_are_the_ones_that_carry_authority() {
        let ungated: Vec<&str> = CruNamespace::iter()
            .filter(|ns| ns.required_capability().is_none())
            .map(CruNamespace::name)
            .collect();
        // A spot check with a reason, not a snapshot: each of these is a
        // namespace whose functions act only on values the caller already
        // holds, or on the calling plugin's own scoped state.
        for expected in ["json", "log", "storage", "paths", "plugin", "oil", "check"] {
            assert!(
                ungated.contains(&expected),
                "'{expected}' became gated; if that is intended, state why here"
            );
        }
        for gated in ["fs", "http", "shell", "kiln", "session", "config"] {
            assert!(
                !ungated.contains(&gated),
                "'{gated}' lost its gate — every one of these is a door out of the daemon"
            );
        }
    }

    /// A namespace path resolves to its own variant, whatever shape the
    /// registration took.
    #[test]
    fn a_member_resolves_to_its_namespace() {
        // A function on a namespace table.
        assert_eq!(
            CruNamespace::for_member("cru.fs", "read"),
            Some(CruNamespace::Fs)
        );
        // A function directly on `cru`: its own name is the namespace.
        assert_eq!(
            CruNamespace::for_member("cru", "embed"),
            Some(CruNamespace::Embed)
        );
        // A nested table belongs to its first segment.
        assert_eq!(
            CruNamespace::for_member("cru.log.messages", "show"),
            Some(CruNamespace::Log)
        );
        // A namespace nobody declared — a test probe — is ungated.
        assert_eq!(CruNamespace::for_member("cru.probe", "anything"), None);
    }
}
