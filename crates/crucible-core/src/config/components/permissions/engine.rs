use super::hardcoded::is_hardcoded_denied;
use super::matcher::{CompiledPermissions, PermissionMatcher};
use super::normalize::{normalize_path_for_matching, split_command_line, UnmodellableConstruct};
use super::types::{PermissionConfig, PermissionDecision, PermissionMode};

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
            self.evaluate_single(tool, input)
        };

        if !is_interactive && matches!(decision, PermissionDecision::Ask { .. }) {
            return PermissionDecision::Deny {
                reason: "Non-interactive mode: ask rules become deny".to_string(),
            };
        }

        decision
    }

    fn evaluate_bash(&self, input: &str) -> PermissionDecision {
        let split = split_command_line(input);

        if split.segments.is_empty() {
            return self.evaluate_single("bash", input);
        }

        let mut has_ask_match = false;
        let mut all_allow_match = true;

        for command in &split.segments {
            if let Some(reason) = is_hardcoded_denied("bash", command) {
                return PermissionDecision::Deny {
                    reason: format!("Hardcoded deny: {reason}"),
                };
            }

            if self.any_match(&self.compiled.deny, "bash", command) {
                return PermissionDecision::Deny {
                    reason: "Matched deny rule".to_string(),
                };
            }

            if self.any_match(&self.compiled.ask, "bash", command) {
                has_ask_match = true;
            }

            if !self.any_match(&self.compiled.allow, "bash", command) {
                all_allow_match = false;
            }
        }

        if has_ask_match {
            return PermissionDecision::Ask { rule_matched: true };
        }

        // Placed after the deny and ask checks so it can only ever tighten: an explicit
        // `deny` still denies and an explicit `ask` still names its rule. What it stops is
        // the leading command's `allow` glob deciding a line the splitter could not read —
        // `git log $(curl evil)` used to come back `Allow` on the strength of `bash:git *`.
        if let Some(construct) = split.unmodellable {
            return self.unmodellable_decision(construct);
        }

        if all_allow_match {
            return PermissionDecision::Allow;
        }

        self.default_decision()
    }

    fn evaluate_single(&self, tool: &str, input: &str) -> PermissionDecision {
        if let Some(reason) = is_hardcoded_denied(tool, input) {
            return PermissionDecision::Deny {
                reason: format!("Hardcoded deny: {reason}"),
            };
        }

        if self.any_match(&self.compiled.deny, tool, input) {
            return PermissionDecision::Deny {
                reason: "Matched deny rule".to_string(),
            };
        }

        if self.any_match(&self.compiled.ask, tool, input) {
            return PermissionDecision::Ask { rule_matched: true };
        }

        if self.any_match(&self.compiled.allow, tool, input) {
            return PermissionDecision::Allow;
        }

        self.default_decision()
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
    fn unmodellable_decision(&self, construct: UnmodellableConstruct) -> PermissionDecision {
        match self.compiled.default {
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

fn matches_bash_with_optional_args(matcher: &PermissionMatcher, tool: &str, input: &str) -> bool {
    matcher.matches(tool, input)
        || (tool == "bash" && !input.ends_with(' ') && matcher.matches(tool, &format!("{input} ")))
}
