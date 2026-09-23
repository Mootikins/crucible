//! The one tool policy: what the daemon decides about a tool call.
//!
//! Its own module because this is a trust boundary, not a detail of tool
//! dispatch. Every tool call the daemon decides passes through
//! [`decide_tool_gate`], whichever agent made it:
//!
//! - the daemon's own agents, which dispatch tools in this process
//!   (`messaging::tool_call`);
//! - an ACP agent, which executes its own tools and asks with
//!   `session/request_permission` (`permission_bridge::DaemonPermissionGate`).
//!
//! Both give it the canonical call, so an operator writes ONE rule and one
//! card entry for a tool no matter which agent calls it. A `bash` rule reads
//! the command line of each `command` call, a file rule the paths, and any
//! other rule the canonical tool name. There is no second gate.

use crate::agent_manager::is_safe;
use crucible_core::agent::ToolPolicy;
use crucible_core::config::components::permissions::{PermissionDecision, PermissionEngine};
use crucible_core::types::CanonicalToolCall;

/// What the daemon decides about one tool call before anybody is asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolGate {
    /// Refuse the call. Nobody is prompted, and the text says why.
    Refuse(String),
    /// Run the call with no prompt.
    ///
    /// `Some` names the layer that granted it, which the caller shows as the
    /// auto-approval marker. `None` means nothing was granted, because
    /// nothing was needed: a read-only tool no rule names.
    Approve(Option<String>),
    /// Put the call to the caller's own gate — the layers only that caller
    /// has. See the module docs of the two callers for what those are.
    Ask,
}

/// Decide one tool call.
///
/// `call` is the canonical call and `args` its JSON arguments. The rules
/// read them through [`PermissionEngine::evaluate_call`]. The card and the
/// read-only exemption read only the canonical tool name.
///
/// The order below is the policy, and each step states what it outranks:
///
/// 1. A card `deny` refuses. It is the strongest statement about a tool, and
///    the tool is not advertised either.
/// 2. A call that needs the gate goes to the gate. The caller's own layers —
///    the `--permissions` override, the saved patterns, the Lua hooks, the
///    mode stance — run there, so this function must not answer over them.
/// 3. An operator `deny` refuses a call that skips the gate. A card `allow`
///    skips the PROMPT, never the operator's config: an untrusted kiln could
///    otherwise ship a card granting `bash: allow` past a configured deny.
/// 4. Everything else runs.
///
/// The engine is always asked as interactive. It folds `ask` into `deny` when
/// nobody can be asked, which is a statement about the session, not about the
/// rules; each caller decides interactivity for itself.
pub(crate) fn decide_tool_gate(
    card_policy: Option<ToolPolicy>,
    engine: Option<&PermissionEngine>,
    call: &CanonicalToolCall,
    args: &serde_json::Value,
) -> ToolGate {
    let tool_name = call.tool.as_str();
    if card_policy == Some(ToolPolicy::Deny) {
        return ToolGate::Refuse(format!(
            "Tool '{tool_name}' is denied by this agent's card tool policy"
        ));
    }

    let rule = engine.map(|engine| engine.evaluate_call(call, args, true));

    if needs_the_gate(card_policy, tool_name, rule.as_ref()) {
        return ToolGate::Ask;
    }

    match rule {
        Some(PermissionDecision::Deny { reason }) => ToolGate::Refuse(format!(
            "Tool '{tool_name}' denied by permissions config: {reason}"
        )),
        Some(PermissionDecision::Allow | PermissionDecision::Ask { .. }) | None => {
            ToolGate::Approve(match card_policy {
                // A card `allow` is a grant the user never saw, so it gets a
                // marker. A call that came here through the read-only
                // exemption gets none: nothing was granted, because nothing
                // was needed.
                Some(ToolPolicy::Allow) => Some("agent card policy".to_string()),
                Some(ToolPolicy::Ask | ToolPolicy::Deny) | None => None,
            })
        }
    }
}

/// Whether the call must face the caller's full gate.
///
/// Answering `false` does not merely skip a prompt — it skips every layer the
/// caller owns. So the read-only exemption consults [`is_safe`] and never
/// `believed_read_only`: the latter includes what a third-party MCP server
/// claims about its own tools with `readOnlyHint`, and an upstream must not
/// annotate its way past a mode's `default = "deny"`.
///
/// An `ask` rule that names the tool takes the exemption away. The operator
/// wrote that rule about this tool on purpose, and skipping it is the same
/// defect as ignoring a `deny` — see [`PermissionDecision::Ask`]. A rule that
/// decides the call outright, `allow` or `deny`, needs no prompt, so it
/// leaves the exemption alone.
fn needs_the_gate(
    card_policy: Option<ToolPolicy>,
    tool_name: &str,
    rule: Option<&PermissionDecision>,
) -> bool {
    match card_policy {
        Some(ToolPolicy::Allow) => false,
        // `Deny` refused before this ran, and it never reaches a prompt.
        Some(ToolPolicy::Ask | ToolPolicy::Deny) => true,
        None => {
            !is_safe(tool_name)
                || matches!(rule, Some(PermissionDecision::Ask { rule_matched: true }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{decide_tool_gate, ToolGate};
    use crucible_core::agent::ToolPolicy;
    use crucible_core::config::components::permissions::{
        PermissionConfig, PermissionEngine, PermissionMode,
    };

    fn engine(config: PermissionConfig) -> PermissionEngine {
        PermissionEngine::new(Some(&config))
    }

    /// Decide a call of the Crucible tool `tool` with no arguments.
    fn gate(
        card_policy: Option<ToolPolicy>,
        engine: Option<&PermissionEngine>,
        tool: &str,
    ) -> ToolGate {
        let args = serde_json::json!({});
        let call = crucible_core::types::CanonicalToolCall::crucible_tool(tool, &args);
        decide_tool_gate(card_policy, engine, &call, &args)
    }

    /// The trust boundary, asserted at the point it is decided.
    ///
    /// Asserting `is_safe` alone is not enough — it proves the function is
    /// narrow, not that this decision consults it. Swapping the body to
    /// `believed_read_only` leaves an `is_safe`-only test green while an
    /// upstream MCP server can annotate its way past every mode rule and Lua
    /// hook, because an `Approve` here skips the caller's whole gate.
    #[test]
    fn an_mcp_read_only_hint_cannot_skip_the_permission_gate() {
        // `gh_create_pr` is in no built-in safe list, so the only thing that
        // could make this `Approve` is trusting the upstream's annotation.
        assert_eq!(
            gate(None, None, "gh_create_pr"),
            ToolGate::Ask,
            "a tool a third party annotated read-only must still reach the mode \
             stance, the mode rules and the Lua hooks"
        );
        assert_eq!(
            gate(None, None, "read_file"),
            ToolGate::Approve(None),
            "a genuinely built-in read-only tool still skips the gate, and it \
             was granted nothing, so it carries no marker"
        );
        assert_eq!(
            gate(None, None, "bash"),
            ToolGate::Ask,
            "and a built-in mutating tool always gates"
        );
    }

    /// A declared `allow` is what lets a non-interactive session do anything
    /// at all.
    ///
    /// Plugin turns pass `is_interactive = false`, which the engine turns into
    /// `Ask` -> `Deny`. That is the right answer for *who approves this* — a
    /// chat-room username is not a Crucible principal — and the wrong answer
    /// for *what may this session do*. The card's policy is the second input
    /// to this decision and the one that survives non-interactivity, so a
    /// plugin can grant a deliberate set without granting a prompt.
    #[test]
    fn a_declared_policy_decides_before_the_built_in_safe_list() {
        assert_eq!(
            gate(Some(ToolPolicy::Allow), None, "bash"),
            ToolGate::Approve(Some("agent card policy".to_string())),
            "a declared allow runs a tool the safe list would have gated"
        );
        assert_eq!(
            gate(Some(ToolPolicy::Ask), None, "read_file"),
            ToolGate::Ask,
            "a declared ask gates a tool the safe list would have skipped"
        );
    }

    /// A card `deny` refuses before any rule is even read.
    #[test]
    fn a_card_deny_refuses_without_asking() {
        let refusal = gate(Some(ToolPolicy::Deny), None, "read_note");
        assert!(
            matches!(&refusal, ToolGate::Refuse(reason) if reason.contains("card tool policy")),
            "a card deny must refuse, not gate: {refusal:?}"
        );
    }

    /// A card `allow` skips the prompt. It never skips an operator `deny`.
    ///
    /// Without this an untrusted kiln could ship a card granting `bash: allow`
    /// and sidestep a configured deny.
    #[test]
    fn an_operator_deny_outranks_a_card_allow() {
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let refusal = gate(Some(ToolPolicy::Allow), Some(&engine(config)), "read_note");
        assert!(
            matches!(&refusal, ToolGate::Refuse(reason) if reason.contains("permissions config")),
            "an operator deny must outrank a card allow: {refusal:?}"
        );
    }

    /// An operator `deny` refuses a read-only tool too.
    ///
    /// The exemption was once checked first, so `deny = ["read_note:*"]` was
    /// ignored without a word.
    #[test]
    fn an_operator_deny_outranks_the_read_only_exemption() {
        let config = PermissionConfig {
            deny: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        let refusal = gate(None, Some(&engine(config)), "read_note");
        assert!(
            matches!(&refusal, ToolGate::Refuse(reason) if reason.contains("permissions config")),
            "an explicit deny must hold even for a read-only tool: {refusal:?}"
        );
    }

    /// An `ask` rule that names a read-only tool takes its exemption away.
    ///
    /// The operator named the tool on purpose. Both callers used to disagree
    /// here: the daemon's own path approved it and the ACP gate prompted.
    #[test]
    fn an_ask_rule_takes_the_read_only_exemption_away() {
        let named = PermissionConfig {
            ask: vec!["read_note:*".to_string()],
            ..Default::default()
        };
        assert_eq!(
            gate(None, Some(&engine(named)), "read_note"),
            ToolGate::Ask,
            "an operator who wrote `ask` about this tool must be asked"
        );

        // A rule that names some OTHER tool leaves the exemption in place.
        let elsewhere = PermissionConfig {
            ask: vec!["bash:*".to_string()],
            ..Default::default()
        };
        assert_eq!(
            gate(None, Some(&engine(elsewhere)), "read_note"),
            ToolGate::Approve(None),
            "a rule about another tool must not gate this one"
        );
    }
}
