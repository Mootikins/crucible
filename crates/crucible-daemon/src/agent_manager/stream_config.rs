//! The per-turn snapshot of everything the agent stream loop reads.
//!
//! Split from `agent_manager/mod.rs` for the file-size gate. The two types
//! here are a pair: `TurnEnvironment` is what the daemon knows at turn start,
//! `AgentStreamConfig` is that plus the session's own config, frozen.

use super::*;

#[derive(Clone)]
pub(crate) struct AgentStreamConfig {
    pub(crate) model: String,
    // No temperature/max_tokens/thinking_budget/system_prompt here. Those
    // reach the LLM through the agent handle, built from the same
    // `SessionAgent` — the copies that used to sit in this struct were never
    // read, and a second place to look for the authoritative value is worse
    // than none. Surfaced by rustc once the struct moved out of `mod.rs`.
    //
    // That was true of thinking_budget and system_prompt and *not* of
    // temperature/max_tokens: the factory dropped those two, so deleting the
    // copies here left them reaching nothing at all. Fixed in the factory
    // (`with_generation_settings`); the invariant this comment asserts is now
    // pinned by `generation_settings_reach_the_outgoing_chat_options`.
    pub(crate) max_iterations: Option<u32>,
    pub(crate) execution_timeout_secs: Option<u64>,
    /// Snapshot of the session's `context_budget` for auto-compaction.
    /// `None` disables auto-compaction (no budget to compare against).
    pub(crate) context_budget: Option<usize>,
    /// Fraction of `context_budget` that triggers auto-compaction.
    /// `None` falls back to `DEFAULT_AUTOCOMPACT_THRESHOLD`. See
    /// [`crate::agent_manager::autocompact`].
    pub(crate) autocompact_threshold: Option<f32>,
    /// Validation mode for assistant text responses. Drives the
    /// validate-retry loop in `execute_agent_stream`.
    pub(crate) output_validation: OutputValidation,
    /// Maximum retry count when output validation fails.
    pub(crate) validation_retries: u32,
    /// From the session's `delegation_config.timeout_secs`; sizes the
    /// tool-dispatch timeout for `delegate_session` (a blocking delegation
    /// legitimately outlives the standard 30 s tool timeout).
    pub(crate) delegation_timeout_secs: Option<u64>,
    /// Per-tool policy from the session's agent card: Deny blocks execution,
    /// Ask forces a prompt (even for safe tools), Allow skips the gate.
    pub(crate) tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    /// Registry of Lua-defined validators, populated when the daemon
    /// has a plugin loader. The agent stream loop dispatches
    /// `OutputValidation::Lua { name }` against this registry.
    /// `None` outside daemon contexts (tests, isolated managers) — the
    /// stream loop treats that as a validation failure with a clear reason.
    pub(crate) lua_validators: Option<Arc<LuaValidatorRegistry>>,
    /// Plugin runtime `Lua` handle used to call into validator functions.
    /// Paired with `lua_validators`; both are `Some` together or both `None`.
    pub(crate) plugin_lua: Option<Arc<Lua>>,
    /// Hooks registered by plugins via `crucible.on`, with the `Lua` state
    /// their bodies live in. Separate from the per-session registry: plugins
    /// load once into the loader's VM, and a `RegistryKey` is only valid
    /// against the state that created it.
    pub(crate) plugin_handlers: Option<(Arc<LuaScriptHandlerRegistry>, Arc<Lua>)>,
    /// Sessions a plugin claimed isolation for. When set and the session is
    /// claimed, a host-touching tool that no handler took over is refused.
    pub(crate) isolation: Option<crucible_lua::IsolationRegistry>,
    /// Plugin-declared tool names, for the plan-mode dispatch guard: the
    /// dispatcher always contains plugin tools and the mode can change
    /// mid-run, so plan must deny them at dispatch, not only at creation.
    pub(crate) plugin_tool_names: std::collections::HashSet<String>,
    /// Modes declared in Lua, snapshotted for this turn so a mid-turn
    /// redefinition cannot reshape a turn already in progress.
    pub(crate) modes: crucible_lua::ModeRegistry,
    /// Prefixed names of MCP tools whose server declared `readOnlyHint: true`.
    /// Snapshotted per turn for the same reason as `modes`, and threaded
    /// exactly like `plugin_tool_names`: the permission path sees a tool name
    /// and nothing else, so what the daemon knows about that tool has to
    /// arrive with the turn.
    pub(crate) mcp_read_only_tools: std::collections::HashSet<String>,
    /// The session's ACP agent name (`claude`, `opencode`, …), when the
    /// session delegates to an external agent. `None` for internal providers.
    /// Carried so pass-through tool-call events can name which agent ran the
    /// tool — the renderer badges it as `acp:<agent>`.
    pub(crate) agent_name: Option<String>,
    /// `"internal"` or `"acp"`, straight from `SessionAgent`.
    ///
    /// The review gate needs it and `agent_name` will not do: `agent_name` is
    /// `None` for every internal agent *and* for an owns-history agent that is
    /// not ACP, so it cannot distinguish "the daemon dispatches this agent's
    /// tools" from "it doesn't". That distinction decides whether a pre-write
    /// gate can be enforced at all. See
    /// [`crucible_core::types::mode::ReviewPolicy::enforceable_by`].
    pub(crate) agent_type: String,
    /// The session's review ledgers, when the manager wired them in.
    ///
    /// `None` leaves the review gate vacuous, which is what tests and any
    /// manager built without ledgers want — the gate is an overlay on
    /// attribution, so a session with no ledger has nothing to gate on.
    pub(crate) review: Option<Arc<crate::review::ReviewLedgers>>,
}

/// What the daemon knows at turn start that the session's own config does not:
/// the Lua VMs, the plugin registries, the declared modes, and what external
/// MCP servers said about their tools.
///
/// Every field is snapshotted per turn, so a plugin loaded or a mode redefined
/// mid-run cannot reshape a turn already in progress.
pub(crate) struct TurnEnvironment {
    pub(crate) lua_validators: Option<Arc<LuaValidatorRegistry>>,
    pub(crate) plugin_lua: Option<Arc<Lua>>,
    pub(crate) plugin_handlers: Option<(Arc<LuaScriptHandlerRegistry>, Arc<Lua>)>,
    pub(crate) isolation: Option<crucible_lua::IsolationRegistry>,
    pub(crate) plugin_tool_names: std::collections::HashSet<String>,
    pub(crate) modes: crucible_lua::ModeRegistry,
    pub(crate) mcp_read_only_tools: std::collections::HashSet<String>,
}

impl AgentStreamConfig {
    pub(crate) fn from_session_agent(session_agent: &SessionAgent, env: TurnEnvironment) -> Self {
        let TurnEnvironment {
            lua_validators,
            plugin_lua,
            plugin_handlers,
            isolation,
            plugin_tool_names,
            modes,
            mcp_read_only_tools,
        } = env;
        Self {
            model: session_agent.model.clone(),
            max_iterations: session_agent.max_iterations,
            execution_timeout_secs: session_agent.execution_timeout_secs,
            context_budget: session_agent.context_budget,
            autocompact_threshold: session_agent.autocompact_threshold,
            output_validation: session_agent.output_validation.clone(),
            validation_retries: session_agent.validation_retries,
            delegation_timeout_secs: session_agent
                .delegation_config
                .as_ref()
                .map(|c| c.timeout_secs),
            tool_policy: session_agent.tool_policy.clone(),
            lua_validators,
            plugin_lua,
            plugin_handlers,
            isolation,
            plugin_tool_names,
            modes,
            mcp_read_only_tools,
            agent_name: session_agent.agent_name.clone(),
            agent_type: session_agent.agent_type.clone(),
            review: None,
        }
    }

    /// Attach the manager's review ledgers to this turn.
    ///
    /// Separate from `from_session_agent` because the ledgers are the
    /// *manager's* state, not the session agent's or the turn environment's,
    /// and because a turn built without them must still run — with the review
    /// gate simply doing nothing.
    pub(crate) fn with_review(mut self, review: Arc<crate::review::ReviewLedgers>) -> Self {
        self.review = Some(review);
        self
    }
}
