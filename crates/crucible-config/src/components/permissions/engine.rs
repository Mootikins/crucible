use super::hardcoded::is_hardcoded_denied;
use super::matcher::{CompiledPermissions, PermissionMatcher};
use super::normalize::{normalize_path_for_matching, split_chained_commands};
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

        if !is_interactive && decision == PermissionDecision::Ask {
            return PermissionDecision::Deny {
                reason: "Non-interactive mode: ask rules become deny".to_string(),
            };
        }

        decision
    }

    fn evaluate_bash(&self, input: &str) -> PermissionDecision {
        let commands = split_chained_commands(input);

        if commands.is_empty() {
            return self.evaluate_single("bash", input);
        }

        let mut has_ask_match = false;
        let mut all_allow_match = true;

        for command in &commands {
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
            return PermissionDecision::Ask;
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
            return PermissionDecision::Ask;
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

    fn default_decision(&self) -> PermissionDecision {
        match self.compiled.default {
            PermissionMode::Allow => PermissionDecision::Allow,
            PermissionMode::Deny => PermissionDecision::Deny {
                reason: "Default mode is deny".to_string(),
            },
            PermissionMode::Ask => PermissionDecision::Ask,
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
