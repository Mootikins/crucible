//! The one tool policy: what the daemon decides about a tool call.
//!
//! Its own module because this is a trust boundary, not a detail of tool
//! dispatch. [`decide_permission`] decides every tool call, whatever its
//! source:
//!
//! - the daemon's own agents, which dispatch tools in this process
//!   (`messaging::tool_call`);
//! - an ACP agent, which executes its own tools and asks with
//!   `session/request_permission` (`messaging::permission`);
//! - a caller with nobody to prompt: `cru.tools.call` and a workflow
//!   validation command (`tools_bridge::unattended_refusal`).
//!
//! Each gives it the canonical call, so an operator writes ONE rule, one card
//! entry and one hook for a tool, whichever agent calls it. A source differs
//! only in what its context can give: an unattended caller has no saved
//! patterns, no hooks, no mode and no prompt.

use super::permission::prompt_user;
use crate::agent_manager::slot::SessionSlot;
use crate::agent_manager::{is_safe, AgentManager, PluginHandlers};
use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
use crucible_core::config::components::permissions::{
    PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::config::PatternStore;
use crucible_core::interaction::{PermRequest, PermissionScope};
use crucible_core::session::PluginApproval;
use crucible_core::types::CanonicalToolCall;
use crucible_lua::{ModeRegistry, PermissionHookResult};
use std::collections::HashSet;
use std::path::Path;

/// What one decision reads from the session of the call.
pub(crate) struct PermissionContext<'a> {
    pub session_id: &'a str,
    /// The `tool_policy` of the session's agent card.
    pub tool_policy: Option<&'a ToolPolicyMap>,
    /// The `[permissions]` rules of the session.
    pub engine: &'a PermissionEngine,
    pub permission_override: Option<PermissionMode>,
    pub plugin: Option<&'a str>,
    pub plugin_approval: PluginApproval,
    /// The `whitelists.d` directory and the project of the saved patterns.
    /// `None`: no saved patterns.
    pub patterns: Option<(&'a Path, &'a Path)>,
    /// The session, for its "allow for this session" grants. `None`: no
    /// session.
    pub slot: Option<&'a SessionSlot>,
    pub hooks: Option<&'a PluginHandlers>,
    pub mode: &'a str,
    pub modes: &'a ModeRegistry,
    pub mcp_read_only: &'a HashSet<String>,
    /// Who answers a prompt. `None`: nobody can, so a call that needs a
    /// prompt is refused.
    pub prompt: Option<Prompt<'a>>,
}

/// Where a prompt goes.
#[derive(Clone, Copy)]
pub(crate) struct Prompt<'a> {
    pub slot: &'a SessionSlot,
    pub event_tx: &'a crate::EventBus,
}

/// The decision about one tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Run the call with no prompt. `Some` names the layer that granted it,
    /// which the caller shows as the auto-approval marker. `None`: nothing
    /// was granted, because nothing was needed (a read-only tool).
    Allow(Option<String>),
    /// The user was asked and said yes.
    UserAllowed,
    /// Refuse the call. The text says why.
    Deny(String),
    /// The prompt ended with no answer, because the turn was cancelled.
    /// Not a refusal: nobody said no.
    NoAnswer,
}

impl Decision {
    pub(crate) fn allowed(&self) -> bool {
        !matches!(self, Self::Deny(_) | Self::NoAnswer)
    }
}

/// The card entry for a call.
///
/// The card keys the same way as the operator rules: a `bash` entry applies
/// to each `command` call, and any entry to the canonical tool name. The
/// strictest entry wins.
fn card_policy(
    tool_policy: Option<&ToolPolicyMap>,
    call: &CanonicalToolCall,
) -> Option<ToolPolicy> {
    let map = tool_policy?;
    let command = (call.kind == "command").then(|| map.get("bash")).flatten();
    [command, map.get(&call.tool)]
        .into_iter()
        .flatten()
        .copied()
        .max_by_key(|policy| match policy {
            ToolPolicy::Allow => 0,
            ToolPolicy::Ask => 1,
            ToolPolicy::Deny => 2,
        })
}

/// The refusal of a card `deny`, if the card denies the call.
///
/// The daemon's own path also asks this before its `pre_tool_call`
/// handlers, so that a handler cannot take over a call that the card
/// refuses.
pub(crate) fn card_refusal(
    tool_policy: Option<&ToolPolicyMap>,
    call: &CanonicalToolCall,
) -> Option<String> {
    (card_policy(tool_policy, call) == Some(ToolPolicy::Deny)).then(|| {
        format!(
            "Tool '{}' is denied by this agent's card tool policy",
            call.tool
        )
    })
}

/// Decide one tool call, and ask the user when no layer decides.
///
/// `args` are the JSON arguments of `call`. The prompt is
/// [`PermRequest::from_call`], with the layer that asked.
pub(crate) async fn decide_permission(
    ctx: &PermissionContext<'_>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> Decision {
    let layer = match decide_unprompted(ctx, call, args) {
        Unprompted::Allow(layer) => return Decision::Allow(layer),
        Unprompted::Deny(reason) => return Decision::Deny(reason),
        Unprompted::Ask(layer) => layer,
    };
    let Some(prompt) = ctx.prompt else {
        return Decision::Deny(no_prompt_refusal(&call.tool));
    };
    let request = PermRequest {
        layer: Some(layer),
        origin: ctx
            .plugin
            .map(|p| crucible_core::turn::TurnOrigin::Plugin(p.to_owned())),
        ..PermRequest::from_call(call, args.clone())
    };
    let Some(response) = prompt_user(prompt.slot, ctx.session_id, prompt.event_tx, request).await
    else {
        return Decision::NoAnswer;
    };

    if response.allowed {
        if let Some(pattern) = response.pattern.as_deref() {
            store_grant(ctx, prompt.slot, call, pattern, response.scope);
        }
        return Decision::UserAllowed;
    }
    let target = call.summary(50).unwrap_or_default();
    Decision::Deny(match &response.reason {
        Some(reason) => format!(
            "User denied permission to {} {target}. Feedback: {reason}",
            call.tool
        ),
        None => format!("User denied permission to {} {target}", call.tool),
    })
}

/// Decide a write nested inside a tool call that this gate already allowed.
///
/// The enclosing call's grant answers what only a person could decide, so a
/// write the tool makes does not ask twice. A card deny, an operator deny, a
/// mode stance or a permission hook still refuses it.
pub(crate) fn decide_nested(
    ctx: &PermissionContext<'_>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> Decision {
    match decide_unprompted(ctx, call, args) {
        Unprompted::Allow(layer) => Decision::Allow(layer),
        Unprompted::Deny(reason) => Decision::Deny(reason),
        Unprompted::Ask(_) => Decision::Allow(Some("the enclosing tool call".into())),
    }
}

/// Keep the grant `pattern` that the user gave for `call` at `scope`.
///
/// A session grant lives in the session slot. A project or user grant goes
/// to its file in `whitelists.d`.
fn store_grant(
    ctx: &PermissionContext<'_>,
    slot: &SessionSlot,
    call: &CanonicalToolCall,
    pattern: &str,
    scope: PermissionScope,
) {
    let stored = match scope {
        PermissionScope::Once => Ok(()),
        PermissionScope::Session => {
            slot.with_session_grants(|grants| AgentManager::add_pattern(grants, call, pattern))
        }
        PermissionScope::Project | PermissionScope::User => {
            let file = ctx.patterns.and_then(|(dir, project)| {
                PatternStore::store_file_in(dir, scope, &project.to_string_lossy())
            });
            file.map_or(Ok(()), |file| {
                AgentManager::store_pattern_to(&file, call, pattern)
            })
        }
    };
    if let Err(e) = stored {
        tracing::warn!(session_id = %ctx.session_id, pattern, error = %e, "Failed to store pattern");
    }
}

/// What the layers before the prompt decide.
enum Unprompted {
    /// Run the call. `Some` names the layer that granted it.
    Allow(Option<String>),
    /// Refuse the call. The text says why.
    Deny(String),
    /// Only a person can decide the call. The text names the layer that
    /// asks.
    Ask(String),
}

/// Every layer but the prompt, in the order of the policy.
///
/// 1. A card `deny` refuses.
/// 2. An operator `deny` refuses, also for a card `allow`: an untrusted kiln
///    must not ship a card that walks past a configured deny.
/// 3. A card `allow` runs the call.
/// 4. The `--permissions` override `allow` or `deny` decides. In a plugin
///    turn whose plugin value is not `inherit`, `allow` does not decide.
/// 5. A read-only tool runs, unless a card `ask` or an operator `ask` rule
///    names it.
/// 6. An operator `allow` runs the call.
/// 7. A saved pattern or a grant for this session runs the call.
/// 8. A Lua `permission:request` hook decides; if none does, the mode rules
///    and the mode stance decide.
///    In a plugin turn the plugin value makes this stricter: `ask` asks
///    (or refuses with nobody to ask), and `stop` refuses. A hook `deny`
///    still refuses.
/// 9. The caller asks the user, or refuses the call when nobody can answer.
///
/// The engine is asked as interactive. It folds `ask` into `deny` when
/// nobody can be asked, and the caller does that instead.
fn decide_unprompted(
    ctx: &PermissionContext<'_>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> Unprompted {
    let tool = call.tool.as_str();
    let card = card_policy(ctx.tool_policy, call);
    if let Some(reason) = card_refusal(ctx.tool_policy, call) {
        return Unprompted::Deny(reason);
    }
    let rule = ctx.engine.evaluate_call(call, args, true);
    if let PermissionDecision::Deny { reason } = &rule {
        return Unprompted::Deny(format!(
            "Tool '{tool}' denied by permissions config: {reason}"
        ));
    }
    if card == Some(ToolPolicy::Allow) {
        return allow("agent card policy");
    }
    match ctx.permission_override {
        Some(PermissionMode::Allow) if ctx.plugin_approval == PluginApproval::Inherit => {
            return allow("permission override")
        }
        Some(PermissionMode::Deny) => {
            return Unprompted::Deny("Tool call denied by permission override".to_string())
        }
        Some(PermissionMode::Allow | PermissionMode::Ask) | None => {}
    }
    // `is_safe`, never `believed_read_only`: an MCP server must not annotate
    // its way past the hooks and the mode stance with `readOnlyHint`. An
    // agent's own tool with a Crucible name is not Crucible's tool.
    let asked_about = matches!(rule, PermissionDecision::Ask { rule_matched: true });
    if card.is_none() && !asked_about && is_safe(tool) && call.runs_in_crucible() {
        return Unprompted::Allow(None);
    }
    if rule == PermissionDecision::Allow {
        return allow("permissions config");
    }
    if let Some((dir, project)) = ctx.patterns {
        let project = project.to_string_lossy();
        let store = PatternStore::load_sync_in(dir, &project)
            .unwrap_or_default()
            .merge(&PatternStore::load_user_sync_in(dir).unwrap_or_default());
        if AgentManager::check_pattern_match(call, &store) {
            return allow("saved pattern");
        }
    }
    if let Some(slot) = ctx.slot {
        if slot.with_session_grants(|grants| AgentManager::check_pattern_match(call, grants)) {
            return allow("session grant");
        }
    }

    // A hook is a decision and a stance is static, so the hook comes first:
    // `cru.modes.auto` allows by default, and a hook that denies must win.
    let stance = match AgentManager::run_permission_hooks(
        ctx.hooks,
        call,
        args,
        ctx.session_id,
        ctx.mode,
        ctx.mcp_read_only,
    ) {
        PermissionHookResult::Allow => Some(allow(if ctx.mode == "auto" {
            "auto mode"
        } else {
            "Lua permission hook"
        })),
        PermissionHookResult::Deny => Some(Unprompted::Deny(format!(
            "Lua hook denied permission to {tool} {}",
            call.summary(50).unwrap_or_default()
        ))),
        PermissionHookResult::Prompt => mode_stance(ctx, call, args),
    };
    // A plugin turn takes the stricter of the stance and the plugin value.
    // With nobody to ask, `ask` refuses, and the reason names the plugin.
    if !matches!(stance, Some(Unprompted::Deny(_))) {
        let plugin = ctx.plugin.unwrap_or("plugin");
        match (ctx.plugin_approval, ctx.prompt) {
            (PluginApproval::Inherit, _) => {}
            (PluginApproval::Ask, Some(_)) => return Unprompted::Ask(format!("plugin {plugin}")),
            (PluginApproval::Ask, None) => {
                return Unprompted::Deny(format!(
                    "Plugin '{plugin}' must ask before '{tool}', but nobody can answer in this turn"
                ))
            }
            (PluginApproval::Stop, _) => {
                return Unprompted::Deny(format!(
                    "Plugin '{plugin}' is stopped from requesting tool permission"
                ))
            }
        }
    }
    if let Some(decision) = stance {
        return decision;
    }

    // A card `ask` or an operator `ask` rule makes a read-only tool ask.
    // Otherwise the mode did not decide.
    Unprompted::Ask(match (card, asked_about) {
        (Some(ToolPolicy::Ask), _) => "agent card policy".to_string(),
        (_, true) => "permissions config".to_string(),
        _ if ctx.mode.is_empty() => "agent".to_string(),
        _ => format!("{} mode", ctx.mode),
    })
}

/// The refusal of a call that needs a prompt when nobody can answer one.
fn no_prompt_refusal(tool: &str) -> String {
    format!(
        "Permission required for '{tool}' but this session runs non-interactively. \
         Allow it via a permission pattern, Lua permission hook, or permissions config."
    )
}

fn allow(layer: &str) -> Unprompted {
    Unprompted::Allow(Some(layer.to_string()))
}

/// The decision of the session mode: its rules, then its default stance.
/// `None` when the mode asks, or when no mode is declared.
///
/// The rules use the `[permissions]` grammar and the same engine, so
/// `bash:rg *` keeps its handling of chained commands: a mode that permits
/// `rg` does not permit `rg foo && rm -rf /`.
fn mode_stance(
    ctx: &PermissionContext<'_>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> Option<Unprompted> {
    let permissions = ctx.modes.get(ctx.mode)?.permissions;
    let stance = match permissions.has_rules() {
        true => match AgentManager::evaluate_mode_rules(&permissions, call, args) {
            PermissionDecision::Allow => PermissionMode::Allow,
            PermissionDecision::Deny { .. } => PermissionMode::Deny,
            PermissionDecision::Ask { .. } => PermissionMode::Ask,
        },
        false => permissions.default,
    };
    match stance {
        PermissionMode::Allow => Some(Unprompted::Allow(Some(format!("{} mode", ctx.mode)))),
        PermissionMode::Deny => Some(Unprompted::Deny(format!(
            "Tool '{}' is not permitted in {} mode",
            call.tool, ctx.mode
        ))),
        PermissionMode::Ask => None,
    }
}

/// `Some(reason)` if a caller with nobody to prompt must not run `name`.
///
/// The same chain as every other caller, with no card, no saved patterns, no
/// hooks, no mode and no prompt. An operator `deny` is absolute, an `allow`
/// runs, a read-only tool runs, and a tool that can mutate needs an explicit
/// `allow`.
pub(crate) fn unattended_refusal(
    engine: &PermissionEngine,
    name: &str,
    args: &serde_json::Value,
) -> Option<String> {
    let modes = ModeRegistry::new();
    let no_mcp = HashSet::new();
    let ctx = PermissionContext {
        session_id: "",
        tool_policy: None,
        engine,
        permission_override: None,
        plugin: None,
        plugin_approval: PluginApproval::Inherit,
        patterns: None,
        slot: None,
        hooks: None,
        mode: "",
        modes: &modes,
        mcp_read_only: &no_mcp,
        prompt: None,
    };
    let call = CanonicalToolCall::crucible_tool(name, args);
    match decide_unprompted(&ctx, &call, args) {
        Unprompted::Allow(_) => None,
        Unprompted::Deny(reason) => Some(reason),
        Unprompted::Ask(_) => Some(no_prompt_refusal(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::components::permissions::PermissionConfig;

    fn engine(config: PermissionConfig) -> PermissionEngine {
        PermissionEngine::new(Some(&config))
    }

    /// Decide a call of the Crucible tool `tool` with no arguments, with a
    /// card entry for it and no prompt.
    fn decide(card: Option<ToolPolicy>, engine: Option<&PermissionEngine>, tool: &str) -> Decision {
        decide_with(card, engine, None, tool)
    }

    fn decide_with(
        card: Option<ToolPolicy>,
        engine: Option<&PermissionEngine>,
        permission_override: Option<PermissionMode>,
        tool: &str,
    ) -> Decision {
        let args = serde_json::json!({});
        let call = CanonicalToolCall::crucible_tool(tool, &args);
        let policy: ToolPolicyMap = card.map(|p| (tool.to_string(), p)).into_iter().collect();
        let modes = ModeRegistry::new();
        let no_mcp = HashSet::new();
        let unconfigured = PermissionEngine::new(None);
        let ctx = PermissionContext {
            session_id: "s",
            tool_policy: Some(&policy),
            engine: engine.unwrap_or(&unconfigured),
            permission_override,
            plugin: None,
            plugin_approval: PluginApproval::Inherit,
            patterns: None,
            slot: None,
            hooks: None,
            mode: "",
            modes: &modes,
            mcp_read_only: &no_mcp,
            prompt: None,
        };
        match decide_unprompted(&ctx, &call, &args) {
            Unprompted::Allow(layer) => Decision::Allow(layer),
            Unprompted::Deny(reason) => Decision::Deny(reason),
            Unprompted::Ask(_) => Decision::Deny(no_prompt_refusal(tool)),
        }
    }

    fn denied(decision: &Decision, why: &str) -> bool {
        matches!(decision, Decision::Deny(reason) if reason.contains(why))
    }

    /// The read-only exemption is `is_safe`, not what an MCP server claims.
    /// A tool that a third party annotated read-only still reaches the hooks
    /// and the mode stance; here, with nobody to ask, it is refused.
    #[test]
    fn an_mcp_read_only_hint_cannot_skip_the_permission_gate() {
        assert!(denied(
            &decide(None, None, "gh_create_pr"),
            "non-interactively"
        ));
        assert_eq!(decide(None, None, "read_file"), Decision::Allow(None));
        assert!(denied(&decide(None, None, "bash"), "non-interactively"));
    }

    /// A card `allow` runs a tool the safe list gates, and a card `ask`
    /// gates a tool the safe list runs.
    #[test]
    fn a_declared_policy_decides_before_the_built_in_safe_list() {
        assert_eq!(
            decide(Some(ToolPolicy::Allow), None, "bash"),
            Decision::Allow(Some("agent card policy".to_string()))
        );
        assert!(denied(
            &decide(Some(ToolPolicy::Ask), None, "read_file"),
            "non-interactively"
        ));
    }

    #[test]
    fn a_card_deny_refuses_without_asking() {
        assert!(denied(
            &decide(Some(ToolPolicy::Deny), None, "read_note"),
            "card tool policy"
        ));
    }

    /// Without this an untrusted kiln could ship a card granting
    /// `bash: allow` and walk past a configured deny.
    #[test]
    fn an_operator_deny_outranks_a_card_allow() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let decision = decide(Some(ToolPolicy::Allow), Some(&engine(config)), "read_note");
        assert!(denied(&decision, "permissions config"), "{decision:?}");
    }

    #[test]
    fn an_operator_deny_outranks_the_read_only_exemption() {
        let config = PermissionConfig {
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let decision = decide(None, Some(&engine(config)), "read_note");
        assert!(denied(&decision, "permissions config"), "{decision:?}");
    }

    /// The operator named the tool on purpose.
    #[test]
    fn an_ask_rule_takes_the_read_only_exemption_away() {
        let named = PermissionConfig {
            ask: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        assert!(denied(
            &decide(None, Some(&engine(named)), "read_note"),
            "non-interactively"
        ));
        let elsewhere = PermissionConfig {
            ask: vec!["bash:*".to_string()],
            ..Default::default()
        };
        assert_eq!(
            decide(None, Some(&engine(elsewhere)), "read_note"),
            Decision::Allow(None)
        );
    }

    /// A card `bash` entry keys each `command` call, whatever its tool name,
    /// as an operator `bash` rule does. The strictest entry wins.
    #[test]
    fn a_card_bash_entry_keys_each_command_call() {
        let args = serde_json::json!({"command": "ls"});
        let shell = CanonicalToolCall::crucible_tool("shell", &args);
        assert_eq!(shell.kind, "command");
        let map = ToolPolicyMap::from([
            ("bash".to_string(), ToolPolicy::Deny),
            ("shell".to_string(), ToolPolicy::Allow),
        ]);
        assert_eq!(card_policy(Some(&map), &shell), Some(ToolPolicy::Deny));
        let read = CanonicalToolCall::crucible_tool("read_file", &serde_json::json!({}));
        assert_eq!(card_policy(Some(&map), &read), None);
    }

    /// `--permissions allow` runs a call that an `ask` rule names, and
    /// `--permissions deny` refuses a call that an `allow` rule names. An
    /// operator `deny` is absolute, also for the override.
    #[test]
    fn the_override_decides_after_an_operator_deny() {
        let ask = engine(PermissionConfig {
            ask: vec!["Task:*".to_string()],
            ..Default::default()
        });
        assert_eq!(
            decide_with(None, Some(&ask), Some(PermissionMode::Allow), "Task"),
            Decision::Allow(Some("permission override".to_string()))
        );
        let allow = engine(PermissionConfig {
            default: PermissionMode::Allow,
            allow: vec!["Task:*".to_string()],
            ..Default::default()
        });
        assert!(denied(
            &decide_with(None, Some(&allow), Some(PermissionMode::Deny), "Task"),
            "permission override"
        ));
        let deny = engine(PermissionConfig {
            deny: vec!["Task:*".to_string()],
            ..Default::default()
        });
        assert!(denied(
            &decide_with(None, Some(&deny), Some(PermissionMode::Allow), "Task"),
            "permissions config"
        ));
    }

    /// Decide a `bash` call in a turn of the plugin `goal`, whose value is
    /// `approval`. `prompt`: a person can answer a prompt.
    fn plugin_turn(
        approval: PluginApproval,
        permission_override: Option<PermissionMode>,
        prompt: bool,
    ) -> Unprompted {
        let args = serde_json::json!({ "command": "ls" });
        let call = CanonicalToolCall::crucible_tool("bash", &args);
        let modes = ModeRegistry::new();
        let no_mcp = HashSet::new();
        let engine = PermissionEngine::new(None);
        let slot = SessionSlot::default();
        let (event_tx, _events) = crate::EventBus::channel(1);
        let ctx = PermissionContext {
            session_id: "s",
            tool_policy: None,
            engine: &engine,
            permission_override,
            plugin: Some("goal"),
            plugin_approval: approval,
            patterns: None,
            slot: None,
            hooks: None,
            mode: "",
            modes: &modes,
            mcp_read_only: &no_mcp,
            prompt: prompt.then_some(Prompt {
                slot: &slot,
                event_tx: &event_tx,
            }),
        };
        decide_unprompted(&ctx, &call, &args)
    }

    fn names_goal(decision: &Unprompted) -> (&'static str, bool) {
        match decision {
            Unprompted::Allow(_) => ("allow", false),
            Unprompted::Ask(layer) => ("ask", layer.contains("goal")),
            Unprompted::Deny(reason) => ("deny", reason.contains("goal")),
        }
    }

    /// `ask` prompts when a person can answer. With nobody to ask it
    /// refuses, and the reason names the plugin. The override `allow` does
    /// not skip the plugin value; the override `deny` still refuses.
    #[test]
    fn a_plugin_ask_prompts_or_refuses_with_its_name() {
        use PluginApproval::Ask;
        assert_eq!(names_goal(&plugin_turn(Ask, None, true)), ("ask", true));
        assert_eq!(names_goal(&plugin_turn(Ask, None, false)), ("deny", true));
        let allow = Some(PermissionMode::Allow);
        assert_eq!(names_goal(&plugin_turn(Ask, allow, true)), ("ask", true));
        assert_eq!(names_goal(&plugin_turn(Ask, allow, false)), ("deny", true));
        let deny = plugin_turn(Ask, Some(PermissionMode::Deny), true);
        assert!(matches!(deny, Unprompted::Deny(r) if r.contains("permission override")));
    }

    /// `stop` refuses each call that needs a prompt, with or without a
    /// person, and also under the override `allow`.
    #[test]
    fn a_plugin_stop_refuses_with_its_name() {
        use PluginApproval::Stop;
        for (permission_override, prompt) in [
            (None, true),
            (None, false),
            (Some(PermissionMode::Allow), true),
        ] {
            let decision = plugin_turn(Stop, permission_override, prompt);
            assert_eq!(names_goal(&decision), ("deny", true));
        }
        let deny = plugin_turn(Stop, Some(PermissionMode::Deny), true);
        assert!(matches!(deny, Unprompted::Deny(r) if r.contains("permission override")));
    }

    /// `inherit` changes nothing: the override `allow` runs the call.
    #[test]
    fn a_plugin_inherit_keeps_the_session_decision() {
        let decision = plugin_turn(PluginApproval::Inherit, Some(PermissionMode::Allow), false);
        assert!(
            matches!(decision, Unprompted::Allow(Some(layer)) if layer == "permission override")
        );
    }
}
