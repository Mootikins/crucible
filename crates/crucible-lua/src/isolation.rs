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
//! What "host-touching" means is answered by the tool's [`ToolSurface`],
//! classified per *tool* — an exhaustive `match` whose missing arm is a compile
//! error — not by a list of names kept in step by hand, and not by the executor
//! that happens to route the call. Per executor was a confused deputy: one
//! answer covered ~20 tools of which three wrote the host filesystem. That is
//! what lets kiln tools survive a claim by construction: they are
//! `Daemon`-surface, while a tool nobody classified answers `Unknown` and is
//! refused here rather than inheriting its neighbours' trust.

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
    /// How to run a command *inside* the sandbox, when the plugin can say.
    pub exec: SandboxExec,
}

/// The argv that relocates a command into a plugin's sandbox.
///
/// Empty when the plugin cannot offer one, which is the safe default: a claim
/// says a session is sandboxed, not that anything can be launched into it.
///
/// It exists for external (ACP) agents. The daemon dispatches an internal
/// agent's tools, so a `pre_tool_call` handler sits before execution; an ACP
/// agent executes tools in its own process and only *reports* them, so
/// interception arrives too late and the session is refused outright. But an
/// ACP agent launched THROUGH this runs inside the container to begin with —
/// its tools are sandboxed by where the process is, not by anything the daemon
/// has to intercept — which is what makes that refusal liftable.
///
/// Argv, deliberately, not a shell string: the container name and the agent's
/// own arguments go in unquoted and unsplit.
#[derive(Debug, Clone, Default)]
pub struct SandboxExec {
    /// Argv up to the point where per-variable environment flags belong, e.g.
    /// `["podman", "exec", "-i", "-w", "/workspace"]`.
    pub prefix: Vec<String>,
    /// How the launcher takes environment variables.
    pub env: SandboxEnv,
    /// Argv between the environment flags and the relocated command, e.g. the
    /// container name.
    ///
    /// The split exists because a launcher's own flags must precede its
    /// positional operand — `podman exec crucible-abc -e K=V agent` passes
    /// `-e K=V` to the *agent*. Which flag, and what has to follow it, is the
    /// plugin's knowledge; all this side does is fill the hole.
    pub suffix: Vec<String>,
}

impl SandboxExec {
    /// Whether this can actually relocate anything.
    pub fn is_empty(&self) -> bool {
        self.prefix.is_empty()
    }
}

/// How a launcher accepts environment variables for the command it relocates.
///
/// Two launchers, two grammars. `podman exec` and `docker exec` repeat a flag —
/// `-e K=V -e K2=V2` — before the container operand. `ssh` has no such flag; the
/// idiom is a positional `env K=V K2=V2 -- cmd`, with `env` the last word of the
/// prefix. Both put the variables in the same place in the argv, so only the
/// presence of a flag differs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SandboxEnv {
    /// The plugin offered no way in. A launch that needs environment is refused
    /// rather than run without it — the safe default, because an agent started
    /// without its API key fails later and somewhere else.
    #[default]
    Unsupported,
    /// A flag repeated once per variable, e.g. `-e`.
    Flag(String),
    /// Bare `K=V` operands, as `env(1)` takes them.
    Inline,
}

impl SandboxEnv {
    /// The argv for one `NAME=VALUE` pair, or `None` when unsupported.
    pub fn argv_for(&self, name: &str, value: &str) -> Option<Vec<String>> {
        match self {
            Self::Unsupported => None,
            Self::Flag(flag) => Some(vec![flag.clone(), format!("{name}={value}")]),
            Self::Inline => Some(vec![format!("{name}={value}")]),
        }
    }
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

    /// The argv that runs a command inside this session's sandbox, when the
    /// claiming plugin offered one.
    pub fn sandbox_exec(&self, session_id: &str) -> Option<SandboxExec> {
        self.get(session_id)
            .map(|c| c.exec)
            .filter(|e| !e.is_empty())
    }

    /// Whether any session currently claims isolation.
    ///
    /// For callers that execute host-touching tools without knowing which
    /// session they act for — `cru.tools.call` is the case — where "some
    /// session is sandboxed" has to be enough to refuse, because the caller
    /// cannot prove it is not that one.
    pub fn any_claim(&self) -> bool {
        self.claims.lock().map(|g| !g.is_empty()).unwrap_or(false)
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

        // `exec_env_flag` wins when both are given: naming a flag is the more
        // specific statement, and silently preferring `inline` would drop it.
        let env = match opts.get::<Option<String>>("exec_env_flag").ok().flatten() {
            Some(flag) => SandboxEnv::Flag(flag),
            None if opts
                .get::<Option<bool>>("exec_env_inline")
                .ok()
                .flatten()
                .unwrap_or(false) =>
            {
                SandboxEnv::Inline
            }
            None => SandboxEnv::Unsupported,
        };

        let exec = SandboxExec {
            prefix: opts
                .get::<Option<Vec<String>>>("exec_prefix")
                .ok()
                .flatten()
                .unwrap_or_default(),
            env,
            suffix: opts
                .get::<Option<Vec<String>>>("exec_suffix")
                .ok()
                .flatten()
                .unwrap_or_default(),
        };

        tracing::info!(
            session_id = %session,
            plugin = %plugin,
            exempt = exempt.len(),
            can_exec = !exec.is_empty(),
            can_pass_env = exec.env != SandboxEnv::Unsupported,
            "plugin claimed session isolation; unhandled host tools will be refused"
        );
        registry.claim(
            &session,
            IsolationClaim {
                plugin,
                exempt,
                exec,
            },
        );
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
            exec: Default::default(),
        }
    }

    /// What the plugin writes is what the launcher reads.
    ///
    /// A field name that only one side knows fails silently: the claim is
    /// still made, the agent is still relocated, and its environment is
    /// quietly gone — which is the failure this shape exists to prevent.
    #[test]
    fn a_claims_exec_fields_survive_the_lua_boundary() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        let reg = IsolationRegistry::new();
        register_isolation_module(&lua, &crucible, reg.clone()).unwrap();
        lua.globals().set("crucible", crucible).unwrap();

        lua.load(
            r#"crucible.require_isolation{
                 session = "s1", plugin = "oci",
                 exec_prefix = { "podman", "exec", "-i" },
                 exec_env_flag = "-e",
                 exec_suffix = { "crucible-s1" },
               }"#,
        )
        .exec()
        .unwrap();

        let exec = reg.sandbox_exec("s1").expect("the claim offered a way in");
        assert_eq!(exec.prefix, vec!["podman", "exec", "-i"]);
        assert_eq!(exec.env, SandboxEnv::Flag("-e".to_string()));
        assert_eq!(exec.suffix, vec!["crucible-s1"]);
    }

    /// `ssh` has no per-variable flag; the idiom is a positional `env K=V cmd`.
    /// Without this a plugin that can only pass environment that way reads as
    /// one that cannot pass it at all, and every ACP launch is refused.
    #[test]
    fn a_launcher_with_no_env_flag_can_still_pass_variables_inline() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        let reg = IsolationRegistry::new();
        register_isolation_module(&lua, &crucible, reg.clone()).unwrap();
        lua.globals().set("crucible", crucible).unwrap();

        lua.load(
            r#"crucible.require_isolation{
                 session = "s1", plugin = "ssh",
                 exec_prefix = { "ssh", "-T", "build-box", "env" },
                 exec_env_inline = true,
               }"#,
        )
        .exec()
        .unwrap();

        let exec = reg.sandbox_exec("s1").expect("the claim offered a way in");
        assert_eq!(exec.env, SandboxEnv::Inline);
        assert_eq!(
            exec.env.argv_for("KEY", "v"),
            Some(vec!["KEY=v".to_string()]),
            "an inline launcher takes the pair as one bare operand"
        );
    }

    #[test]
    fn a_claim_that_says_nothing_about_env_cannot_pass_any() {
        let exec = SandboxExec::default();
        assert_eq!(exec.env, SandboxEnv::Unsupported);
        assert_eq!(exec.env.argv_for("KEY", "v"), None);
    }

    #[test]
    fn a_flag_launcher_repeats_its_flag_before_each_pair() {
        let env = SandboxEnv::Flag("-e".to_string());
        assert_eq!(
            env.argv_for("KEY", "v"),
            Some(vec!["-e".to_string(), "KEY=v".to_string()])
        );
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
