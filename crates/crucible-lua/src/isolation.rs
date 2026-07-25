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

use mlua::{Lua, Result as LuaResult, Table};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// What a plugin claimed about a session's isolation.
#[derive(Debug, Clone, Default)]
pub struct IsolationClaim {
    /// Plugin that made the claim, for diagnostics and to attribute a refusal.
    pub plugin: String,
    /// Tools explicitly permitted to run on the host despite the claim.
    ///
    /// The escape hatch. Deliberately explicit and per-name: a claim without
    /// one refuses everything unhandled, which is the safe default, and every
    /// exemption is then a visible decision rather than an omission.
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

    /// Whether `tool` may run on the host for this session.
    ///
    /// `true` when no plugin claimed isolation (the ordinary case) or the tool
    /// is explicitly exempt.
    pub fn host_execution_allowed(&self, session_id: &str, tool: &str) -> bool {
        match self.get(session_id) {
            None => true,
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
            reg.host_execution_allowed("s1", "bash"),
            "sessions with no isolation claim must be unaffected — this gate \
             must not change behaviour for the ordinary, unsandboxed case"
        );
    }

    #[test]
    fn a_claimed_session_denies_by_default() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(!reg.host_execution_allowed("s1", "bash"));
        // The point of default-deny: a tool nobody thought about is refused,
        // rather than silently escaping to the host.
        assert!(!reg.host_execution_allowed("s1", "some_future_tool"));
    }

    #[test]
    fn only_the_named_tools_are_exempt() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&["read_note"]));
        assert!(reg.host_execution_allowed("s1", "read_note"));
        assert!(!reg.host_execution_allowed("s1", "bash"));
    }

    #[test]
    fn a_claim_is_scoped_to_its_session() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        assert!(!reg.host_execution_allowed("s1", "bash"));
        assert!(
            reg.host_execution_allowed("s2", "bash"),
            "one session's isolation must not deny another's tools"
        );
    }

    #[test]
    fn releasing_a_claim_restores_host_execution() {
        let reg = IsolationRegistry::new();
        reg.claim("s1", claim(&[]));
        reg.release("s1");
        assert!(
            reg.host_execution_allowed("s1", "bash"),
            "a released claim must not outlive the container it described"
        );
    }
}
