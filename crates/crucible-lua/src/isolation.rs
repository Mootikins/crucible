//! Session isolation claimed by a plugin.
//!
//! A plugin that sandboxes tool execution (`oci` and its container) calls
//! `crucible.require_isolation{...}` during `on_session_start`. From then on
//! the daemon refuses any host-touching tool the plugin did not handle.
//!
//! This exists because interception was previously an *allowlist*: the plugin
//! named six tools and claimed to sandbox the session. That is complete only
//! by coincidence — a seventh workspace tool, a plugin-contributed tool, or an
//! MCP gateway tool bypasses the sandbox silently, and the boundary is
//! invisible. Under a default-deny claim the plugin instead asserts a
//! *category*: anything that can touch the filesystem or execute is handled
//! in-container or refused.
//!
//! What "host-touching" means is answered by the tool's
//! [`ToolSurface`], declared by the executor that would run it, not by a list
//! of names kept in step by hand. That is what lets kiln tools survive a claim
//! by construction: they are `Daemon`-surface, and a kiln tool added later
//! inherits that instead of falling off an allowlist nobody updated.

use crucible_core::traits::tools::ToolSurface;
use mlua::{Lua, Result as LuaResult, Table};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// What a plugin claimed about a session's isolation.
#[derive(Debug, Clone, Default)]
pub struct IsolationClaim {
    /// Plugin that made the claim, for diagnostics and to attribute a refusal.
    pub plugin: String,
    /// Host- or unknown-surface tools explicitly permitted to run on the host
    /// despite the claim.
    ///
    /// The escape hatch, and only for the two surfaces that need one —
    /// `Daemon`-surface tools pass without appearing here. Deliberately
    /// explicit and per-name: a claim without one refuses everything
    /// unhandled, which is the safe default, and every exemption is then a
    /// visible decision rather than an omission.
    pub exempt: HashSet<String>,
}

/// Isolation claims by session id.
///
/// Shared between the plugin Lua runtime (which writes) and the tool-call
/// dispatcher (which reads), the same way handlers and validators are.
#[derive(Debug, Clone, Default)]
pub struct IsolationRegistry {
    claims: Arc<Mutex<HashMap<String, IsolationClaim>>>,
}

impl IsolationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim(&self, session_id: &str, claim: IsolationClaim) {
        if let Ok(mut g) = self.claims.lock() {
            g.insert(session_id.to_string(), claim);
        }
    }

    pub fn get(&self, session_id: &str) -> Option<IsolationClaim> {
        self.claims.lock().ok()?.get(session_id).cloned()
    }

    /// Drop a session's claim. Called at session end so a finished session's
    /// claim can't outlive the container it described.
    pub fn release(&self, session_id: &str) {
        if let Ok(mut g) = self.claims.lock() {
            g.remove(session_id);
        }
    }

    /// Whether `tool` may run for this session given what its execution can
    /// reach.
    ///
    /// `true` when no plugin claimed isolation (the ordinary case), when the
    /// tool's surface is [`ToolSurface::Daemon`], or when the tool is
    /// explicitly exempt.
    ///
    /// The surface is what makes a claim usable. Gating by name alone refused
    /// `semantic_search`, `read_note` and every other kiln tool in a sandboxed
    /// session unless someone hand-listed them — so turning on the sandbox
    /// turned off Crucible, and a kiln tool added later silently broke every
    /// sandboxed session. Daemon-surface tools reach daemon-side storage, which
    /// containerizing a *workspace* says nothing about; they pass by
    /// construction. `Host` and `Unknown` keep the default-deny, with `exempt`
    /// as the one visible way back onto the host.
    pub fn host_execution_allowed(
        &self,
        session_id: &str,
        tool: &str,
        surface: ToolSurface,
    ) -> bool {
        match self.get(session_id) {
            None => true,
            Some(_) if surface == ToolSurface::Daemon => true,
            Some(claim) => claim.exempt.contains(tool),
        }
    }
}

/// Register `crucible.require_isolation` on the plugin runtime.
///
/// ```lua
/// crucible.on_session_start(function(session)
///   start_container(session)
///   crucible.require_isolation{
///     session = session.id,
///     plugin  = "oci",
///     exempt  = { "read_note", "semantic_search" },
///   }
/// end, { required = true })
/// ```
pub fn register_isolation_module(
    lua: &Lua,
    crucible: &Table,
    registry: IsolationRegistry,
) -> LuaResult<()> {
    let require_isolation = lua.create_function(move |_, opts: Table| {
        let session: String = opts.get("session").map_err(|_| {
            mlua::Error::runtime(
                "crucible.require_isolation: `session` is required (use session.id)",
            )
        })?;
        let plugin: String = opts.get("plugin").unwrap_or_else(|_| "unknown".to_string());
        let exempt: HashSet<String> = opts
            .get::<Option<Vec<String>>>("exempt")
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .collect();

        tracing::info!(
            session_id = %session,
            plugin = %plugin,
            exempt = exempt.len(),
            "plugin claimed session isolation; unhandled host tools will be refused"
        );
        registry.claim(&session, IsolationClaim { plugin, exempt });
        Ok(())
    })?;
    crucible.set("require_isolation", require_isolation)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(exempt: &[&str]) -> IsolationClaim {
        IsolationClaim {
            plugin: "oci".to_string(),
            exempt: exempt.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn an_unclaimed_session_allows_host_execution() {
        let reg = IsolationRegistry::new();
        assert!(
            reg.host_execution_allowed("s1", "bash", ToolSurface::Host),
            "sessions with no isolation claim must be unaffected — this gate \
             must not change behaviour for the ordinary, unsandboxed case"
        );
    }

    #[test]
    fn a_claimed_session_denies_host_tools_by_default() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(!reg.host_execution_allowed("s1", "bash", ToolSurface::Host));
        // The point of default-deny: a tool nobody thought about is refused,
        // rather than silently escaping to the host.
        assert!(!reg.host_execution_allowed("s1", "some_future_tool", ToolSurface::Host));
    }

    /// The property the surface classification buys.
    ///
    /// Not "get_kiln_info happens to be allowed" — nothing named it. A tool is
    /// allowed because the executor that runs it reaches daemon-side storage
    /// and nothing else, so a kiln tool added tomorrow is allowed for the same
    /// reason without anyone editing a list.
    #[test]
    fn a_daemon_surface_tool_survives_isolation_unnamed() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(
            reg.host_execution_allowed("s1", "semantic_search", ToolSurface::Daemon),
            "a daemon-surface tool must pass an isolation claim with no exemption"
        );
        assert!(
            reg.host_execution_allowed("s1", "a_kiln_tool_added_next_year", ToolSurface::Daemon),
            "and so must one that did not exist when the claim was written"
        );
    }

    /// An MCP gateway tool runs daemon-side but is third-party code that can
    /// reach anything, so it is refused like a host tool.
    #[test]
    fn an_unknown_surface_tool_is_refused_like_a_host_tool() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(!reg.host_execution_allowed("s1", "fs__read_file", ToolSurface::Unknown));
    }

    #[test]
    fn exempt_reopens_host_and_unknown_surfaces_only() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&["bash", "fs__read_file"]));
        assert!(reg.host_execution_allowed("s1", "bash", ToolSurface::Host));
        assert!(reg.host_execution_allowed("s1", "fs__read_file", ToolSurface::Unknown));
        assert!(!reg.host_execution_allowed("s1", "write_file", ToolSurface::Host));
    }

    #[test]
    fn a_claim_is_scoped_to_its_session() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(!reg.host_execution_allowed("s1", "bash", ToolSurface::Host));
        assert!(
            reg.host_execution_allowed("s2", "bash", ToolSurface::Host),
            "one session's isolation must not deny another's tools"
        );
    }

    #[test]
    fn releasing_a_claim_restores_host_execution() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        reg.release("s1");
        assert!(
            reg.host_execution_allowed("s1", "bash", ToolSurface::Host),
            "a released claim must not outlive the container it described"
        );
    }
}
