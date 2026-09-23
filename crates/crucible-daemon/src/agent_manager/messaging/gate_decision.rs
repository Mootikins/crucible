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
use crate::agent_manager::{is_safe, AgentManager, DaemonPermissions};
use crate::protocol::SessionEventMessage;
use crucible_core::agent::{ToolPolicy, ToolPolicyMap};
use crucible_core::config::components::permissions::{
    PermissionDecision, PermissionEngine, PermissionMode,
};
use crucible_core::config::PatternStore;
use crucible_core::interaction::PermRequest;
use crucible_core::types::CanonicalToolCall;
use crucible_lua::{ModeRegistry, ModeStance, PermissionHookResult};
use std::collections::HashSet;
use std::path::Path;
use tokio::sync::broadcast;

/// What one decision reads from the session of the call.
pub(crate) struct PermissionContext<'a> {
    pub session_id: &'a str,
    /// The `tool_policy` of the session's agent card.
    pub tool_policy: Option<&'a ToolPolicyMap>,
    /// The `[permissions]` rules of the session.
    pub engine: &'a PermissionEngine,
    pub permission_override: Option<PermissionMode>,
    /// The `whitelists.d` directory and the project of the saved patterns.
    /// `None`: no saved patterns.
    pub patterns: Option<(&'a Path, &'a Path)>,
    pub hooks: Option<&'a DaemonPermissions>,
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
    pub event_tx: &'a broadcast::Sender<SessionEventMessage>,
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
}

impl Decision {
    pub(crate) fn allowed(&self) -> bool {
        !matches!(self, Self::Deny(_))
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
/// `args` are the JSON arguments of `call`. `request` makes the prompt; it
/// runs only when the user is asked.
pub(crate) async fn decide_permission(
    ctx: &PermissionContext<'_>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
    request: impl FnOnce() -> PermRequest,
) -> Decision {
    let prompt = match decide_unprompted(ctx, call, args) {
        Ok(decision) => return decision,
        Err(prompt) => prompt,
    };
    let response = prompt_user(prompt.slot, ctx.session_id, prompt.event_tx, request()).await;

    if response.allowed {
        let file = ctx.patterns.and_then(|(dir, project)| {
            PatternStore::store_file_in(dir, response.scope, &project.to_string_lossy())
        });
        if let Some((pattern, file)) = response.pattern.as_deref().zip(file) {
            if let Err(e) = AgentManager::store_pattern_to(&file, call, pattern) {
                tracing::warn!(session_id = %ctx.session_id, pattern, error = %e, "Failed to store pattern");
            }
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

/// Every layer but the prompt, in the order of the policy. `Err` is the
/// prompt that must decide the call.
///
/// 1. A card `deny` refuses.
/// 2. An operator `deny` refuses, also for a card `allow`: an untrusted kiln
///    must not ship a card that walks past a configured deny.
/// 3. A card `allow` runs the call.
/// 4. The `--permissions` override `allow` or `deny` decides.
/// 5. A read-only tool runs, unless a card `ask` or an operator `ask` rule
///    names it.
/// 6. An operator `allow` runs the call.
/// 7. A saved pattern runs the call.
/// 8. A Lua `permission:request` hook decides; if none does, the mode rules
///    and the mode stance decide.
/// 9. With nobody to ask, the call is refused. Otherwise the user is asked.
///
/// The engine is asked as interactive. It folds `ask` into `deny` when
/// nobody can be asked, and step 9 does that here instead.
fn decide_unprompted<'a>(
    ctx: &PermissionContext<'a>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> Result<Decision, Prompt<'a>> {
    let tool = call.tool.as_str();
    let card = card_policy(ctx.tool_policy, call);
    if let Some(reason) = card_refusal(ctx.tool_policy, call) {
        return Ok(Decision::Deny(reason));
    }
    let rule = ctx.engine.evaluate_call(call, args, true);
    if let PermissionDecision::Deny { reason } = &rule {
        return Ok(Decision::Deny(format!(
            "Tool '{tool}' denied by permissions config: {reason}"
        )));
    }
    if card == Some(ToolPolicy::Allow) {
        return Ok(allow("agent card policy"));
    }
    match ctx.permission_override {
        Some(PermissionMode::Allow) => return Ok(allow("permission override")),
        Some(PermissionMode::Deny) => {
            return Ok(Decision::Deny(
                "Tool call denied by permission override".to_string(),
            ))
        }
        Some(PermissionMode::Ask) | None => {}
    }
    // `is_safe`, never `believed_read_only`: an MCP server must not annotate
    // its way past the hooks and the mode stance with `readOnlyHint`.
    let asked_about = matches!(rule, PermissionDecision::Ask { rule_matched: true });
    if card.is_none() && !asked_about && is_safe(tool) {
        return Ok(Decision::Allow(None));
    }
    if rule == PermissionDecision::Allow {
        return Ok(allow("permissions config"));
    }
    if let Some((dir, project)) = ctx.patterns {
        let project = project.to_string_lossy();
        let store = PatternStore::load_sync_in(dir, &project)
            .unwrap_or_default()
            .merge(&PatternStore::load_user_sync_in(dir).unwrap_or_default());
        if AgentManager::check_pattern_match(call, &store) {
            return Ok(allow("saved pattern"));
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
        PermissionHookResult::Deny => Some(Decision::Deny(format!(
            "Lua hook denied permission to {tool} {}",
            call.summary(50).unwrap_or_default()
        ))),
        PermissionHookResult::Prompt => mode_stance(ctx, call, args),
    };
    // The plugin approval check (feat/plugin-turns 75ffa651d) goes here.
    if let Some(decision) = stance {
        return Ok(decision);
    }

    match ctx.prompt {
        Some(prompt) => Err(prompt),
        None => Ok(Decision::Deny(format!(
            "Permission required for '{tool}' but this session runs non-interactively. \
             Allow it via a permission pattern, Lua permission hook, or permissions config."
        ))),
    }
}

fn allow(layer: &str) -> Decision {
    Decision::Allow(Some(layer.to_string()))
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
) -> Option<Decision> {
    let permissions = ctx.modes.get(ctx.mode)?.permissions;
    let stance = match permissions.has_rules() {
        true => match AgentManager::evaluate_mode_rules(&permissions, call, args) {
            PermissionDecision::Allow => ModeStance::Allow,
            PermissionDecision::Deny { .. } => ModeStance::Deny,
            PermissionDecision::Ask { .. } => ModeStance::Ask,
        },
        false => permissions.default,
    };
    match stance {
        ModeStance::Allow => Some(Decision::Allow(Some(format!("{} mode", ctx.mode)))),
        ModeStance::Deny => Some(Decision::Deny(format!(
            "Tool '{}' is not permitted in {} mode",
            call.tool, ctx.mode
        ))),
        ModeStance::Ask => None,
    }
}

/// `Some(reason)` if a caller with nobody to prompt must not run `name`.
///
/// The same chain as every other caller, with no card, no saved patterns, no
/// hooks, no mode and no prompt. An operator `deny` is absolute, an `allow`
/// runs, a read-only tool runs, and a tool that can mutate needs an explicit
/// `allow`.
pub(crate) fn unattended_decision(
    engine: &PermissionEngine,
    name: &str,
    args: &serde_json::Value,
) -> Decision {
    let modes = ModeRegistry::new();
    let no_mcp = HashSet::new();
    let ctx = PermissionContext {
        session_id: "",
        tool_policy: None,
        engine,
        permission_override: None,
        patterns: None,
        hooks: None,
        mode: "",
        modes: &modes,
        mcp_read_only: &no_mcp,
        prompt: None,
    };
    let call = CanonicalToolCall::crucible_tool(name, args);
    decide_unprompted(&ctx, &call, args).unwrap_or_else(|_| {
        Decision::Deny(format!(
            "Permission required for '{name}', and nobody can answer a prompt"
        ))
    })
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
            patterns: None,
            hooks: None,
            mode: "",
            modes: &modes,
            mcp_read_only: &no_mcp,
            prompt: None,
        };
        decide_unprompted(&ctx, &call, &args).unwrap_or_else(|_| panic!("no prompt"))
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
}
