use globset::{GlobBuilder, GlobMatcher};

use super::parse::parse_rule;
use super::types::{PermissionConfig, PermissionMode};

/// Compiled permission matcher for one permission rule.
#[derive(Debug, Clone)]
pub struct PermissionMatcher {
    /// Tool name (or `*` wildcard).
    pub tool: String,
    /// Server name for `mcp` and `plugin` rules.
    pub server: Option<String>,
    /// Compiled glob matcher from rule pattern.
    pub glob: GlobMatcher,
}

impl PermissionMatcher {
    /// Parse and compile a permission rule into an efficient matcher.
    pub fn new(rule: &str) -> Result<Self, String> {
        let parsed = parse_rule(rule)?;
        let glob = compile_glob(&parsed.pattern)?;

        Ok(Self {
            tool: parsed.tool,
            server: parsed.server,
            glob,
        })
    }

    /// Match a tool invocation against this compiled rule.
    pub fn matches(&self, tool: &str, input: &str) -> bool {
        if self.tool != "*" && self.tool != tool {
            return false;
        }

        if matches!(self.tool.as_str(), "mcp" | "plugin") {
            if let Some(server) = &self.server {
                let Some((input_server, input_tool)) = input.split_once(':') else {
                    return false;
                };

                if input_server != server {
                    return false;
                }

                return self.glob.is_match(input_tool);
            }
        }

        self.glob.is_match(input)
    }
}

/// Pre-compiled permission rules grouped by mode.
#[derive(Debug, Clone)]
pub struct CompiledPermissions {
    /// Default mode when no rule matches.
    pub default: PermissionMode,
    /// Compiled allow rules.
    pub allow: Vec<PermissionMatcher>,
    /// Compiled deny rules.
    pub deny: Vec<PermissionMatcher>,
    /// Compiled ask rules.
    pub ask: Vec<PermissionMatcher>,
}

impl CompiledPermissions {
    /// Compile rules from [`PermissionConfig`], returning warnings for invalid rules.
    pub fn from_config(config: &PermissionConfig) -> (Self, Vec<String>) {
        let mut warnings = Vec::new();
        let allow = compile_rules("allow", &config.allow, &mut warnings);
        let deny = compile_rules("deny", &config.deny, &mut warnings);
        let ask = compile_rules("ask", &config.ask, &mut warnings);

        (
            Self {
                default: config.default,
                allow,
                deny,
                ask,
            },
            warnings,
        )
    }
}

pub(super) fn compile_glob(pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .case_insensitive(false)
        .literal_separator(false)
        .build()
        .map_err(|err| format!("Invalid glob pattern '{pattern}': {err}"))
        .map(|glob: globset::Glob| glob.compile_matcher())
}

pub(super) fn compile_rules(
    mode: &str,
    rules: &[String],
    warnings: &mut Vec<String>,
) -> Vec<PermissionMatcher> {
    let mut compiled = Vec::new();

    for rule in rules {
        match PermissionMatcher::new(rule) {
            Ok(matcher) => compiled.push(matcher),
            Err(err) => warnings.push(format!("Invalid {mode} permission rule '{rule}': {err}")),
        }
    }

    compiled
}
