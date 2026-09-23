use super::hardcoded::is_hardcoded_denied;
use super::matcher::{CompiledPermissions, PermissionMatcher};
use super::normalize::{
    normalize_path_for_matching, resolve_command_word, split_command_line, UnmodellableConstruct,
};
use super::types::{PermissionConfig, PermissionDecision, PermissionMode};
use crate::types::CanonicalToolCall;

#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PermissionEngine {
    compiled: CompiledPermissions,
}

#[allow(missing_docs)]
impl PermissionEngine {
    pub fn new(config: Option<&PermissionConfig>) -> Self {
        let default_config = PermissionConfig::default();
        let config = config.unwrap_or(&default_config);
        let (compiled, _warnings) = CompiledPermissions::from_config(config);
        Self { compiled }
    }

    pub fn evaluate(&self, tool: &str, input: &str, is_interactive: bool) -> PermissionDecision {
        let decision = if tool == "bash" {
            self.evaluate_bash(input)
        } else {
            self.evaluate_single(&[tool], input)
        };
        self.finish(decision, is_interactive)
    }

    /// Decide one tool call.
    ///
    /// The key of a rule decides what the rule reads. A `bash` rule reads
    /// the command line of each `command` call, whichever tool made it. A
    /// file rule reads each path of a call of its file kind: `read` reads a
    /// `file_read` call, and `edit`, `write` and `delete` read a `file_edit`
    /// call. Any other rule reads the canonical tool name, with the JSON
    /// `args` as its input. So one rule decides Crucible's own shell and the
    /// shell of each ACP agent.
    ///
    /// The strongest answer wins: `deny`, then `ask`, then `allow`. A file
    /// rule allows only when it allows each path.
    pub fn evaluate_call(
        &self,
        call: &CanonicalToolCall,
        args: &serde_json::Value,
        is_interactive: bool,
    ) -> PermissionDecision {
        let command = (call.kind == "command")
            .then(|| self.evaluate_bash(call.command.as_deref().unwrap_or_default()));
        let keys = file_rule_keys(&call.kind);
        let paths = (!keys.is_empty() && !call.paths.is_empty())
            .then(|| every_input(call.paths.iter().map(|p| self.evaluate_single(keys, p))));
        let named = (call.tool != "bash" && !is_file_tool(&call.tool))
            .then(|| self.evaluate_single(&[&call.tool], &args.to_string()));
        self.finish(
            strongest([command, paths, named].into_iter().flatten().flatten()),
            is_interactive,
        )
    }

    /// The decision of the rules, or the default when no rule decides.
    ///
    /// Nobody can answer an `ask` in a non-interactive session, so it
    /// becomes a `deny` there.
    fn finish(
        &self,
        decision: Option<PermissionDecision>,
        is_interactive: bool,
    ) -> PermissionDecision {
        let decision = decision.unwrap_or_else(|| self.default_decision());
        if !is_interactive && matches!(decision, PermissionDecision::Ask { .. }) {
            return PermissionDecision::Deny {
                reason: "Non-interactive mode: ask rules become deny".to_string(),
            };
        }
        decision
    }

    /// The rules for a command line. `None` when no rule decides.
    fn evaluate_bash(&self, input: &str) -> Option<PermissionDecision> {
        let split = split_command_line(input);

        if split.segments.is_empty() {
            return self.evaluate_single(&["bash"], input);
        }

        let mut has_ask_match = false;
        let mut all_allow_match = true;

        let mut indirect = None;

        for command in &split.segments {
            // What the statement actually invokes, with `sudo`/`env`/`(`/`/bin/` and the
            // rest stripped. Only the restrictive lists consult it — see `any_match`.
            let resolved = resolve_command_word(command);
            indirect = indirect.or(resolved.unmodellable);

            if let Some(reason) = is_hardcoded_denied("bash", command)
                .or_else(|| is_hardcoded_denied("bash", &resolved.resolved))
            {
                return Some(PermissionDecision::Deny {
                    reason: format!("Hardcoded deny: {reason}"),
                });
            }

            if self.any_match_restrictive(&self.compiled.deny, command, &resolved.resolved) {
                return Some(PermissionDecision::Deny {
                    reason: "Matched deny rule".to_string(),
                });
            }

            if self.any_match_restrictive(&self.compiled.ask, command, &resolved.resolved) {
                has_ask_match = true;
            }

            // Deliberately literal: broadening `allow` would let `time git status` inherit
            // `bash:git *`, which widens the gate. Resolution may only ever tighten.
            if !self.any_match(&self.compiled.allow, "bash", command) {
                all_allow_match = false;
            }
        }

        if has_ask_match {
            return Some(PermissionDecision::Ask { rule_matched: true });
        }

        // Placed after the deny and ask checks so it can only ever tighten: an explicit
        // `deny` still denies and an explicit `ask` still names its rule. What it stops is
        // the leading command's `allow` glob deciding a line the splitter could not read —
        // `git log $(curl evil)` used to come back `Allow` on the strength of `bash:git *`.
        if let Some(construct) = split.unmodellable.or(indirect) {
            return Some(self.unmodellable_decision(construct));
        }

        all_allow_match.then_some(PermissionDecision::Allow)
    }

    /// The rules for `input` under any of the rule keys `keys`. `None` when
    /// no rule decides.
    fn evaluate_single(&self, keys: &[&str], input: &str) -> Option<PermissionDecision> {
        if let Some(reason) = keys.iter().find_map(|key| is_hardcoded_denied(key, input)) {
            return Some(PermissionDecision::Deny {
                reason: format!("Hardcoded deny: {reason}"),
            });
        }
        let any = |matchers: &[PermissionMatcher]| {
            keys.iter().any(|key| self.any_match(matchers, key, input))
        };

        if any(&self.compiled.deny) {
            return Some(PermissionDecision::Deny {
                reason: "Matched deny rule".to_string(),
            });
        }

        if any(&self.compiled.ask) {
            return Some(PermissionDecision::Ask { rule_matched: true });
        }

        any(&self.compiled.allow).then_some(PermissionDecision::Allow)
    }

    /// `any_match` for the lists that refuse or prompt, widened to the resolved command.
    ///
    /// Only `deny`, `ask` and the hardcoded table get this. A rule that grants must keep
    /// matching the text the operator wrote, or stripping a wrapper would hand a command
    /// an `allow` its author never granted.
    fn any_match_restrictive(
        &self,
        matchers: &[PermissionMatcher],
        raw: &str,
        resolved: &str,
    ) -> bool {
        self.any_match(matchers, "bash", raw)
            || (resolved != raw && self.any_match(matchers, "bash", resolved))
    }

    fn any_match(&self, matchers: &[PermissionMatcher], tool: &str, input: &str) -> bool {
        matchers.iter().any(|matcher| {
            if !is_file_tool(tool) {
                return matches_bash_with_optional_args(matcher, tool, input);
            }

            let normalized = normalize_path_for_matching(input);
            matches_bash_with_optional_args(matcher, tool, input)
                || matches_bash_with_optional_args(matcher, tool, &normalized)
        })
    }

    /// The configured default, with a reason naming the construct that forced it.
    ///
    /// One departure from the plain default: under `default = "allow"` with `deny` rules
    /// configured, an unreadable statement prompts instead of being allowed. Allowing it
    /// would mean the operator's `deny` list is silently not enforced on exactly the lines
    /// where it cannot be checked — `eval "rm -rf /"` under a blocklist config. With no
    /// `deny` rules there is nothing to fail to enforce, so the default stands.
    fn unmodellable_decision(&self, construct: UnmodellableConstruct) -> PermissionDecision {
        match self.compiled.default {
            PermissionMode::Allow if !self.compiled.deny.is_empty() => {
                let _ = construct;
                PermissionDecision::Ask {
                    rule_matched: false,
                }
            }
            PermissionMode::Allow => PermissionDecision::Allow,
            PermissionMode::Deny => PermissionDecision::Deny {
                reason: format!(
                    "Cannot check {} against the rules; default mode is deny",
                    construct.describe()
                ),
            },
            PermissionMode::Ask => PermissionDecision::Ask {
                rule_matched: false,
            },
        }
    }

    fn default_decision(&self) -> PermissionDecision {
        match self.compiled.default {
            PermissionMode::Allow => PermissionDecision::Allow,
            PermissionMode::Deny => PermissionDecision::Deny {
                reason: "Default mode is deny".to_string(),
            },
            PermissionMode::Ask => PermissionDecision::Ask {
                rule_matched: false,
            },
        }
    }
}

fn is_file_tool(tool: &str) -> bool {
    matches!(tool, "read" | "edit" | "write" | "delete")
}

/// The file rules that read the paths of a call of `kind`. A `read` rule
/// never reads an edit, so an operator who allows reads allows no edit.
fn file_rule_keys(kind: &str) -> &'static [&'static str] {
    match kind {
        "file_read" => &["read"],
        "file_edit" => &["edit", "write", "delete"],
        _ => &[],
    }
}

/// The strongest decision: `deny`, then an `ask` that a rule named, then
/// another `ask`, then `allow`.
fn strongest(
    decisions: impl IntoIterator<Item = PermissionDecision>,
) -> Option<PermissionDecision> {
    decisions.into_iter().max_by_key(|decision| match decision {
        PermissionDecision::Allow => 0,
        PermissionDecision::Ask {
            rule_matched: false,
        } => 1,
        PermissionDecision::Ask { rule_matched: true } => 2,
        PermissionDecision::Deny { .. } => 3,
    })
}

/// The decision for several inputs, as for the statements of a command
/// line: a `deny` or an `ask` for any input, and an `allow` only when each
/// input has one.
fn every_input(
    decisions: impl Iterator<Item = Option<PermissionDecision>>,
) -> Option<PermissionDecision> {
    let decisions: Vec<_> = decisions.collect();
    if decisions
        .iter()
        .all(|decision| decision == &Some(PermissionDecision::Allow))
    {
        return Some(PermissionDecision::Allow);
    }
    strongest(
        decisions
            .into_iter()
            .flatten()
            .filter(|decision| decision != &PermissionDecision::Allow),
    )
}

fn matches_bash_with_optional_args(matcher: &PermissionMatcher, tool: &str, input: &str) -> bool {
    matcher.matches(tool, input)
        || (tool == "bash" && !input.ends_with(' ') && matcher.matches(tool, &format!("{input} ")))
}
