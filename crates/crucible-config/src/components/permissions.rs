//! Permission configuration types and rule parsing

use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Scope for writing permission rules to config files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionScope {
    /// Project-level config: `crucible.toml` in the project directory.
    Project,
    /// User-level config: `~/.config/crucible/config.toml` (or platform equivalent).
    User,
}

/// Permission mode for tool access control
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Allow tool execution
    Allow,
    /// Deny tool execution
    Deny,
    /// Ask user for permission
    #[default]
    Ask,
}

impl std::str::FromStr for PermissionMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "allow" => Ok(PermissionMode::Allow),
            "deny" => Ok(PermissionMode::Deny),
            "ask" => Ok(PermissionMode::Ask),
            other => Err(format!(
                "Invalid permission mode: '{}'. Must be allow, deny, or ask",
                other
            )),
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionMode::Allow => write!(f, "allow"),
            PermissionMode::Deny => write!(f, "deny"),
            PermissionMode::Ask => write!(f, "ask"),
        }
    }
}

/// Parsed permission rule with tool, optional server, and pattern
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRule {
    /// Tool name (bash, read, edit, write, delete, mcp, plugin, *)
    pub tool: String,
    /// Server name for MCP/plugin rules (e.g., "github" in "mcp:github:*")
    pub server: Option<String>,
    /// Pattern for matching (e.g., "cargo test *", "src/**")
    pub pattern: String,
}

/// Permission configuration for tool access control
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionConfig {
    /// Default permission mode when no rule matches
    #[serde(default)]
    pub default: PermissionMode,
    /// Rules that allow tool execution
    #[serde(default)]
    pub allow: Vec<String>,
    /// Rules that deny tool execution
    #[serde(default)]
    pub deny: Vec<String>,
    /// Rules that ask user for permission
    #[serde(default)]
    pub ask: Vec<String>,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default: PermissionMode::Ask,
            allow: Vec::new(),
            deny: Vec::new(),
            ask: Vec::new(),
        }
    }
}

impl PermissionConfig {
    /// Create a new permission configuration with default settings
    pub fn new() -> Self {
        Self::default()
    }
}

fn resolve_config_path(
    scope: PermissionScope,
    config_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    match scope {
        PermissionScope::Project => {
            if let Some(dir) = config_dir {
                Ok(dir.join("crucible.toml"))
            } else {
                std::env::current_dir()
                    .map(|d| d.join("crucible.toml"))
                    .map_err(|e| format!("Failed to get current directory: {e}"))
            }
        }
        PermissionScope::User => {
            if let Some(dir) = config_dir {
                Ok(dir.join("config.toml"))
            } else {
                dirs::config_dir()
                    .map(|d| d.join("crucible").join("config.toml"))
                    .ok_or_else(|| "Could not determine user config directory".to_string())
            }
        }
    }
}

/// Write a permission rule to the allow list in the appropriate config file.
///
/// `config_dir` overrides the default config location (useful for testing).
/// For `Project` scope, writes to `crucible.toml`. For `User` scope, writes to `config.toml`.
///
/// Creates the file and parent directories if they don't exist.
/// Preserves existing config content. Skips duplicate rules.
#[cfg(feature = "toml")]
pub fn write_permission_rule(
    scope: PermissionScope,
    rule: &str,
    config_dir: Option<&Path>,
) -> Result<(), String> {
    let path = resolve_config_path(scope, config_dir)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
    };

    let mut table: toml::Table = if content.is_empty() {
        toml::Table::new()
    } else {
        toml::from_str(&content).map_err(|e| format!("Failed to parse {}: {e}", path.display()))?
    };

    let permissions = table
        .entry("permissions")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));

    let permissions_table = permissions
        .as_table_mut()
        .ok_or_else(|| "permissions is not a table".to_string())?;

    let allow = permissions_table
        .entry("allow")
        .or_insert_with(|| toml::Value::Array(Vec::new()));

    let allow_array = allow
        .as_array_mut()
        .ok_or_else(|| "permissions.allow is not an array".to_string())?;

    if allow_array.iter().any(|v| v.as_str() == Some(rule)) {
        return Ok(());
    }

    allow_array.push(toml::Value::String(rule.to_string()));

    let output =
        toml::to_string_pretty(&table).map_err(|e| format!("Failed to serialize config: {e}"))?;

    std::fs::write(&path, output)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    Ok(())
}

/// Check if a tool invocation matches a hardcoded Layer 0 denial pattern.
///
/// Returns `Some(reason)` if the operation is denied, `None` if allowed to continue.
/// These are immutable denials that cannot be overridden by any user config.
///
/// # Arguments
/// * `tool` - The tool name (e.g., "bash", "edit", "read")
/// * `input` - The input string (e.g., command for bash, path for edit)
///
/// # Examples
/// ```
/// use crucible_config::components::permissions::is_hardcoded_denied;
///
/// assert_eq!(
///     is_hardcoded_denied("bash", "rm -rf /"),
///     Some("Destructive: removes root filesystem")
/// );
/// assert_eq!(is_hardcoded_denied("bash", "cargo test"), None);
/// assert_eq!(is_hardcoded_denied("edit", "src/main.rs"), None);
/// ```
pub fn is_hardcoded_denied(tool: &str, input: &str) -> Option<&'static str> {
    // Only bash tool gets hardcoded denials
    if tool != "bash" {
        return None;
    }

    let trimmed = input.trim();

    // Pattern 1: rm -rf /
    if trimmed == "rm -rf /" || trimmed.starts_with("rm -rf / ") {
        return Some("Destructive: removes root filesystem");
    }

    // Pattern 2: rm -rf ~
    if trimmed == "rm -rf ~" || trimmed.starts_with("rm -rf ~ ") {
        return Some("Destructive: removes home directory");
    }

    // Pattern 3: rm -rf $HOME
    if trimmed == "rm -rf $HOME" || trimmed.starts_with("rm -rf $HOME ") {
        return Some("Destructive: removes home directory");
    }

    // Pattern 4: rm -rf .
    if trimmed == "rm -rf ." || trimmed.starts_with("rm -rf . ") {
        return Some("Destructive: removes current directory");
    }

    // Pattern 5: rm -rf ..
    if trimmed == "rm -rf .." || trimmed.starts_with("rm -rf .. ") {
        return Some("Destructive: removes parent directory");
    }

    // Pattern 6: sudo rm -rf * (with any path)
    if trimmed.starts_with("sudo rm -rf") && trimmed.contains("*") {
        return Some("Destructive: root removal with wildcard");
    }

    // Pattern 7: mkfs prefix (filesystem formatting)
    if trimmed.starts_with("mkfs") {
        return Some("Destructive: formats filesystem");
    }

    // Pattern 8: dd writing to block devices
    if trimmed.contains("dd ") && trimmed.contains("of=/dev/") {
        return Some("Destructive: writes to block device");
    }

    None
}

/// Normalize a path for permission matching by collapsing `.`, `..`, and duplicate slashes.
///
/// This prevents path traversal attacks like `src/../.env` from bypassing file permission rules.
/// The function does NOT resolve symlinks or expand `~` (no filesystem access).
///
/// # Algorithm
///
/// 1. Split path on `/`
/// 2. Process each component:
///    - `.` → skip (current directory)
///    - `..` → pop from result stack if non-empty (parent directory)
///    - Other → push to result stack
/// 3. Rejoin with `/`
/// 4. Remove trailing slashes
///
/// # Examples
///
/// ```
/// # use crucible_config::components::permissions::normalize_path_for_matching;
/// assert_eq!(normalize_path_for_matching("src/../.env"), ".env");
/// assert_eq!(normalize_path_for_matching("src/./main.rs"), "src/main.rs");
/// assert_eq!(normalize_path_for_matching("src//deep///file.rs"), "src/deep/file.rs");
/// assert_eq!(normalize_path_for_matching("src/deep/"), "src/deep");
/// assert_eq!(normalize_path_for_matching("~/Documents/"), "~/Documents");
/// assert_eq!(normalize_path_for_matching("a/b/../../c"), "c");
/// assert_eq!(normalize_path_for_matching("../../etc/passwd"), "../../etc/passwd");
/// assert_eq!(normalize_path_for_matching(""), "");
/// ```
///
/// # Security Note
///
/// For permission matching, apply normalization to BOTH the rule pattern and the input path,
/// then use the most restrictive result: if either the raw path OR the normalized path matches
/// a deny rule, deny the operation.
pub fn normalize_path_for_matching(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }

    let is_absolute = path.starts_with('/');
    let mut result: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {
                // Empty (from double slashes) or current dir → skip
            }
            ".." => {
                if let Some(last) = result.last() {
                    if *last != ".." {
                        result.pop();
                    } else if !is_absolute {
                        result.push("..");
                    }
                } else if !is_absolute {
                    result.push("..");
                }
            }
            other => {
                // Regular component → push
                result.push(other);
            }
        }
    }

    let normalized = result.join("/").trim_end_matches('/').to_string();
    if is_absolute {
        if normalized.is_empty() {
            "/".to_string()
        } else {
            format!("/{normalized}")
        }
    } else {
        normalized
    }
}

/// Split a bash command string on operators (`&&`, `||`, `;`, `|`) while respecting quoted strings.
///
/// This function splits chained bash commands so the permission engine can evaluate each
/// sub-command independently. It handles double-quoted and single-quoted strings, ensuring
/// operators inside quotes are not treated as delimiters.
///
/// # Arguments
/// * `input` - A bash command string, possibly containing multiple chained commands
///
/// # Returns
/// A vector of borrowed string slices, each representing a single command. Each slice is
/// trimmed of leading/trailing whitespace. Empty segments (e.g., from trailing semicolons)
/// are filtered out.
///
/// # Examples
/// ```
/// use crucible_config::components::permissions::split_chained_commands;
///
/// // Basic chaining
/// assert_eq!(
///     split_chained_commands("cargo test && rm -rf /"),
///     vec!["cargo test", "rm -rf /"]
/// );
///
/// // Single command (no split)
/// assert_eq!(
///     split_chained_commands("cargo test"),
///     vec!["cargo test"]
/// );
///
/// // Quoted strings (no split inside quotes)
/// assert_eq!(
///     split_chained_commands("echo \"hello && world\""),
///     vec!["echo \"hello && world\""]
/// );
///
/// // Single-quoted strings
/// assert_eq!(
///     split_chained_commands("echo 'hello && world'"),
///     vec!["echo 'hello && world'"]
/// );
///
/// // Multiple operators
/// assert_eq!(
///     split_chained_commands("a && b || c; d | e"),
///     vec!["a", "b", "c", "d", "e"]
/// );
///
/// // Trailing semicolon (filtered out)
/// assert_eq!(
///     split_chained_commands("cmd;"),
///     vec!["cmd"]
/// );
///
/// // Empty input
/// assert_eq!(
///     split_chained_commands(""),
///     vec![] as Vec<&str>
/// );
/// ```
///
/// # Limitations
/// - Does not handle escaped operators like `\&\&` (treated as regular characters)
/// - Does not handle `$(...)` subshell substitution
/// - Does not handle heredocs
/// - Does not handle backtick substitution
///
/// # Security Note
/// This function is used for permission checking. Each sub-command is evaluated independently,
/// so `cargo test && rm -rf /` will check both `cargo test` and `rm -rf /` separately.
pub fn split_chained_commands(input: &str) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }

    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut current_start = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut i = 0;

    while i < bytes.len() {
        let ch = bytes[i];

        // Handle quote state
        if ch == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        if ch == b'\x27' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        // If inside quotes, skip operator detection
        if in_double_quote || in_single_quote {
            i += 1;
            continue;
        }

        // Check for operators (only when not in quotes)
        let is_operator = if i + 1 < bytes.len() {
            // Two-character operators: &&, ||
            (ch == b'&' && bytes[i + 1] == b'&') || (ch == b'|' && bytes[i + 1] == b'|')
        } else {
            false
        };

        if is_operator {
            // Found && or ||
            let segment = input[current_start..i].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current_start = i + 2;
            i += 2;
        } else if ch == b';' || ch == b'|' {
            // Single-character operators: ; or |
            let segment = input[current_start..i].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current_start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    // Add the final segment
    let segment = input[current_start..].trim();
    if !segment.is_empty() {
        segments.push(segment);
    }

    segments
}

/// Parse a permission rule string into its components
///
/// Format: `"tool:pattern"` or `"tool:server:pattern"` for MCP/plugin rules
///
/// # Examples
///
/// - `"bash:cargo test *"` → tool="bash", server=None, pattern="cargo test *"
/// - `"mcp:github:create_issue"` → tool="mcp", server=Some("github"), pattern="create_issue"
/// - `"edit:src/**"` → tool="edit", server=None, pattern="src/**"
///
/// # Errors
///
/// Returns `Err` if:
/// - No colon separator found
/// - Tool name is empty
/// - For MCP/plugin rules, server name is empty
pub fn parse_rule(s: &str) -> Result<ParsedRule, String> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();

    match parts.len() {
        2 => {
            let tool = parts[0];
            let pattern = parts[1];

            if tool.is_empty() {
                return Err("Tool name cannot be empty".to_string());
            }

            Ok(ParsedRule {
                tool: tool.to_string(),
                server: None,
                pattern: pattern.to_string(),
            })
        }
        3 => {
            let tool = parts[0];
            let second = parts[1];
            let third = parts[2];

            if tool.is_empty() {
                return Err("Tool name cannot be empty".to_string());
            }

            if matches!(tool, "mcp" | "plugin") {
                if second.is_empty() {
                    return Err("Server name cannot be empty".to_string());
                }

                return Ok(ParsedRule {
                    tool: tool.to_string(),
                    server: Some(second.to_string()),
                    pattern: third.to_string(),
                });
            }

            Ok(ParsedRule {
                tool: tool.to_string(),
                server: None,
                pattern: format!("{second}:{third}"),
            })
        }
        _ => Err("Rule must contain at least one colon (e.g., 'tool:pattern')".to_string()),
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum PermissionDecision {
    Allow,
    Deny { reason: String },
    Ask,
}

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

fn compile_glob(pattern: &str) -> Result<GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .case_insensitive(false)
        .literal_separator(false)
        .build()
        .map_err(|err| format!("Invalid glob pattern '{pattern}': {err}"))
        .map(|glob: globset::Glob| glob.compile_matcher())
}

fn compile_rules(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_rules(
        default: PermissionMode,
        allow: &[&str],
        deny: &[&str],
        ask: &[&str],
    ) -> PermissionConfig {
        PermissionConfig {
            default,
            allow: allow.iter().map(|s| (*s).to_string()).collect(),
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
            ask: ask.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn permission_mode_default_is_ask() {
        assert_eq!(PermissionMode::default(), PermissionMode::Ask);
    }

    #[test]
    fn permission_config_default_is_ask() {
        let config = PermissionConfig::default();
        assert_eq!(config.default, PermissionMode::Ask);
        assert!(config.allow.is_empty());
        assert!(config.deny.is_empty());
        assert!(config.ask.is_empty());
    }

    #[test]
    fn parse_rule_simple_bash_pattern() {
        let rule = parse_rule("bash:cargo test *").unwrap();
        assert_eq!(rule.tool, "bash");
        assert_eq!(rule.server, None);
        assert_eq!(rule.pattern, "cargo test *");
    }

    #[test]
    fn parse_rule_mcp_with_server() {
        let rule = parse_rule("mcp:github:create_issue").unwrap();
        assert_eq!(rule.tool, "mcp");
        assert_eq!(rule.server, Some("github".to_string()));
        assert_eq!(rule.pattern, "create_issue");
    }

    #[test]
    fn parse_rule_mcp_with_wildcard() {
        let rule = parse_rule("mcp:github:*").unwrap();
        assert_eq!(rule.tool, "mcp");
        assert_eq!(rule.server, Some("github".to_string()));
        assert_eq!(rule.pattern, "*");
    }

    #[test]
    fn parse_rule_edit_with_glob_pattern() {
        let rule = parse_rule("edit:src/**").unwrap();
        assert_eq!(rule.tool, "edit");
        assert_eq!(rule.server, None);
        assert_eq!(rule.pattern, "src/**");
    }

    #[test]
    fn parse_rule_plugin_with_server() {
        let rule = parse_rule("plugin:discord:send_message").unwrap();
        assert_eq!(rule.tool, "plugin");
        assert_eq!(rule.server, Some("discord".to_string()));
        assert_eq!(rule.pattern, "send_message");
    }

    #[test]
    fn parse_rule_read_pattern() {
        let rule = parse_rule("read:docs/**").unwrap();
        assert_eq!(rule.tool, "read");
        assert_eq!(rule.server, None);
        assert_eq!(rule.pattern, "docs/**");
    }

    #[test]
    fn parse_rule_no_colon_fails() {
        let result = parse_rule("invalid");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("colon"));
    }

    #[test]
    fn parse_rule_empty_tool_fails() {
        let result = parse_rule(":pattern");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Tool name"));
    }

    #[test]
    fn parse_rule_empty_server_fails() {
        let result = parse_rule("mcp::pattern");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Server name"));
    }

    #[test]
    fn permission_mode_serde_lowercase() {
        let json_allow = serde_json::to_string(&PermissionMode::Allow).unwrap();
        assert_eq!(json_allow, "\"allow\"");

        let json_deny = serde_json::to_string(&PermissionMode::Deny).unwrap();
        assert_eq!(json_deny, "\"deny\"");

        let json_ask = serde_json::to_string(&PermissionMode::Ask).unwrap();
        assert_eq!(json_ask, "\"ask\"");
    }

    #[test]
    fn permission_mode_deserialize_lowercase() {
        let allow: PermissionMode = serde_json::from_str("\"allow\"").unwrap();
        assert_eq!(allow, PermissionMode::Allow);

        let deny: PermissionMode = serde_json::from_str("\"deny\"").unwrap();
        assert_eq!(deny, PermissionMode::Deny);

        let ask: PermissionMode = serde_json::from_str("\"ask\"").unwrap();
        assert_eq!(ask, PermissionMode::Ask);
    }

    #[test]
    fn permission_config_toml_roundtrip() {
        let toml_str = r#"
default = "ask"
allow = ["read:*"]
deny = ["bash:rm *"]
ask = ["edit:src/**"]
"#;

        let config: PermissionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default, PermissionMode::Ask);
        assert_eq!(config.allow, vec!["read:*"]);
        assert_eq!(config.deny, vec!["bash:rm *"]);
        assert_eq!(config.ask, vec!["edit:src/**"]);

        // Serialize back
        let serialized = toml::to_string_pretty(&config).unwrap();
        let config2: PermissionConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(config, config2);
    }

    #[test]
    fn permission_config_yaml_roundtrip() {
        let yaml_str = r#"
default: ask
allow:
  - read:*
deny:
  - bash:rm *
ask:
  - edit:src/**
"#;

        let config: PermissionConfig = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(config.default, PermissionMode::Ask);
        assert_eq!(config.allow, vec!["read:*"]);
        assert_eq!(config.deny, vec!["bash:rm *"]);
        assert_eq!(config.ask, vec!["edit:src/**"]);

        // Serialize back
        let serialized = serde_yaml::to_string(&config).unwrap();
        let config2: PermissionConfig = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(config, config2);
    }

    #[test]
    fn permission_config_missing_section_is_none() {
        // This test verifies that when permissions section is missing from config,
        // it deserializes to None (handled by Config struct with Option<PermissionConfig>)
        let config = PermissionConfig::default();
        assert_eq!(config.default, PermissionMode::Ask);
        assert!(config.allow.is_empty());
    }

    #[test]
    fn parse_rule_with_colons_in_pattern() {
        // Pattern can contain colons (e.g., "http://example.com")
        let rule = parse_rule("read:http://example.com").unwrap();
        assert_eq!(rule.tool, "read");
        assert_eq!(rule.server, None);
        assert_eq!(rule.pattern, "http://example.com");
    }

    #[test]
    fn parse_rule_mcp_with_colons_in_pattern() {
        // MCP rule with colons in pattern
        let rule = parse_rule("mcp:api:http://example.com:8080").unwrap();
        assert_eq!(rule.tool, "mcp");
        assert_eq!(rule.server, Some("api".to_string()));
        assert_eq!(rule.pattern, "http://example.com:8080");
    }

    #[test]
    fn permission_config_equality() {
        let config1 = PermissionConfig {
            default: PermissionMode::Ask,
            allow: vec!["read:*".to_string()],
            deny: vec![],
            ask: vec![],
        };

        let config2 = PermissionConfig {
            default: PermissionMode::Ask,
            allow: vec!["read:*".to_string()],
            deny: vec![],
            ask: vec![],
        };

        assert_eq!(config1, config2);
    }

    #[test]
    fn permission_matcher_matches_bash_pattern() {
        let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
        assert!(matcher.matches("bash", "cargo test integration"));
    }

    #[test]
    fn permission_matcher_rejects_non_matching_bash_pattern() {
        let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
        assert!(!matcher.matches("bash", "npm run test"));
    }

    #[test]
    fn permission_matcher_matches_bash_wildcard() {
        let matcher = PermissionMatcher::new("bash:*").unwrap();
        assert!(matcher.matches("bash", "anything"));
    }

    #[test]
    fn permission_matcher_matches_mcp_server_wildcard_pattern() {
        let matcher = PermissionMatcher::new("mcp:github:*").unwrap();
        assert!(matcher.matches("mcp", "github:create_issue"));
    }

    #[test]
    fn permission_matcher_rejects_mcp_wrong_server() {
        let matcher = PermissionMatcher::new("mcp:github:*").unwrap();
        assert!(!matcher.matches("mcp", "gitlab:create_issue"));
    }

    #[test]
    fn permission_matcher_matches_mcp_exact_pattern() {
        let matcher = PermissionMatcher::new("mcp:github:create_issue").unwrap();
        assert!(matcher.matches("mcp", "github:create_issue"));
    }

    #[test]
    fn permission_matcher_matches_edit_glob() {
        let matcher = PermissionMatcher::new("edit:src/**").unwrap();
        assert!(matcher.matches("edit", "src/deep/nested/file.rs"));
    }

    #[test]
    fn permission_matcher_rejects_edit_outside_glob() {
        let matcher = PermissionMatcher::new("edit:src/**").unwrap();
        assert!(!matcher.matches("edit", "test/file.rs"));
    }

    #[test]
    fn permission_matcher_matches_read_any() {
        let matcher = PermissionMatcher::new("read:*").unwrap();
        assert!(matcher.matches("read", "/any/path"));
    }

    #[test]
    fn permission_matcher_invalid_glob_returns_error() {
        let result = PermissionMatcher::new("bash:invalid[");
        assert!(result.is_err());
    }

    #[test]
    fn permission_matcher_rejects_wrong_tool() {
        let matcher = PermissionMatcher::new("bash:cargo test *").unwrap();
        assert!(!matcher.matches("edit", "cargo test"));
    }

    #[test]
    fn permission_matcher_matches_wildcard_tool() {
        let matcher = PermissionMatcher::new("*:*").unwrap();
        assert!(matcher.matches("bash", "anything"));
    }

    #[test]
    fn compiled_permissions_returns_warnings_for_invalid_patterns() {
        let config = PermissionConfig {
            default: PermissionMode::Ask,
            allow: vec!["read:*".to_string(), "bash:invalid[".to_string()],
            deny: vec!["mcp:github:*".to_string(), "invalid".to_string()],
            ask: vec!["edit:src/**".to_string()],
        };

        let (compiled, warnings) = CompiledPermissions::from_config(&config);

        assert_eq!(compiled.default, PermissionMode::Ask);
        assert_eq!(compiled.allow.len(), 1);
        assert_eq!(compiled.deny.len(), 1);
        assert_eq!(compiled.ask.len(), 1);
        assert_eq!(warnings.len(), 2);
    }

    // Layer 0 hardcoded denial tests
    #[test]
    fn hardcoded_denied_rm_rf_root() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf /"),
            Some("Destructive: removes root filesystem")
        );
    }

    #[test]
    fn hardcoded_denied_rm_rf_home_tilde() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf ~"),
            Some("Destructive: removes home directory")
        );
    }

    #[test]
    fn hardcoded_denied_rm_rf_home_env_var() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf $HOME"),
            Some("Destructive: removes home directory")
        );
    }

    #[test]
    fn hardcoded_denied_rm_rf_current_dir() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf ."),
            Some("Destructive: removes current directory")
        );
    }

    #[test]
    fn hardcoded_denied_rm_rf_parent_dir() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf .."),
            Some("Destructive: removes parent directory")
        );
    }

    #[test]
    fn hardcoded_denied_sudo_rm_rf_wildcard() {
        assert_eq!(
            is_hardcoded_denied("bash", "sudo rm -rf /tmp/*"),
            Some("Destructive: root removal with wildcard")
        );
    }

    #[test]
    fn hardcoded_denied_mkfs() {
        assert_eq!(
            is_hardcoded_denied("bash", "mkfs.ext4 /dev/sda1"),
            Some("Destructive: formats filesystem")
        );
    }

    #[test]
    fn hardcoded_denied_dd_block_device() {
        assert_eq!(
            is_hardcoded_denied("bash", "dd if=/dev/zero of=/dev/sda"),
            Some("Destructive: writes to block device")
        );
    }

    #[test]
    fn hardcoded_allowed_safe_command() {
        assert_eq!(is_hardcoded_denied("bash", "cargo test"), None);
    }

    #[test]
    fn hardcoded_allowed_safe_rm() {
        assert_eq!(is_hardcoded_denied("bash", "rm -rf ~/Documents"), None);
    }

    #[test]
    fn hardcoded_allowed_edit_tool() {
        assert_eq!(is_hardcoded_denied("edit", "src/main.rs"), None);
    }

    #[test]
    fn hardcoded_allowed_read_tool() {
        assert_eq!(is_hardcoded_denied("read", "/etc/passwd"), None);
    }

    #[test]
    fn hardcoded_denied_with_whitespace() {
        assert_eq!(
            is_hardcoded_denied("bash", "  rm -rf /  "),
            Some("Destructive: removes root filesystem")
        );
    }

    #[test]
    fn hardcoded_denied_rm_rf_root_with_args() {
        assert_eq!(
            is_hardcoded_denied("bash", "rm -rf / --force"),
            Some("Destructive: removes root filesystem")
        );
    }

    #[test]
    fn hardcoded_denied_mkfs_variants() {
        assert_eq!(
            is_hardcoded_denied("bash", "mkfs.btrfs /dev/sdb"),
            Some("Destructive: formats filesystem")
        );
    }

    // Path normalization tests
    #[test]
    fn normalize_path_traversal_attack() {
        assert_eq!(normalize_path_for_matching("src/../.env"), ".env");
    }

    #[test]
    fn normalize_dot_components() {
        assert_eq!(normalize_path_for_matching("src/./main.rs"), "src/main.rs");
    }

    #[test]
    fn normalize_double_slashes() {
        assert_eq!(
            normalize_path_for_matching("src//deep///file.rs"),
            "src/deep/file.rs"
        );
    }

    #[test]
    fn normalize_trailing_slash() {
        assert_eq!(normalize_path_for_matching("src/deep/"), "src/deep");
    }

    #[test]
    fn normalize_tilde_preserved() {
        assert_eq!(normalize_path_for_matching("~/Documents/"), "~/Documents");
    }

    #[test]
    fn normalize_multiple_traversals() {
        assert_eq!(normalize_path_for_matching("a/b/../../c"), "c");
    }

    #[test]
    fn normalize_traversal_above_root() {
        assert_eq!(
            normalize_path_for_matching("../../etc/passwd"),
            "../../etc/passwd"
        );
    }

    #[test]
    fn normalize_empty_path() {
        assert_eq!(normalize_path_for_matching(""), "");
    }

    #[test]
    fn normalize_single_dot() {
        assert_eq!(normalize_path_for_matching("."), "");
    }

    #[test]
    fn normalize_single_dotdot() {
        assert_eq!(normalize_path_for_matching(".."), "..");
    }

    #[test]
    fn normalize_absolute_path() {
        assert_eq!(normalize_path_for_matching("/etc/passwd"), "/etc/passwd");
    }

    #[test]
    fn normalize_complex_traversal() {
        assert_eq!(normalize_path_for_matching("a/b/c/../../d/../e"), "a/e");
    }

    // split_chained_commands tests
    #[test]
    fn split_basic_and_operator() {
        let result = split_chained_commands("cargo test && rm -rf /");
        assert_eq!(result, vec!["cargo test", "rm -rf /"]);
    }

    #[test]
    fn split_single_command_no_split() {
        let result = split_chained_commands("cargo test");
        assert_eq!(result, vec!["cargo test"]);
    }

    #[test]
    fn split_double_quoted_string_no_split() {
        let result = split_chained_commands("echo \"hello && world\"");
        assert_eq!(result, vec!["echo \"hello && world\""]);
    }

    #[test]
    fn split_single_quoted_string_no_split() {
        let result = split_chained_commands("git commit -m 'feat: && stuff'");
        assert_eq!(result, vec!["git commit -m 'feat: && stuff'"]);
    }

    #[test]
    fn split_multiple_operators() {
        let result = split_chained_commands("a && b || c; d | e");
        assert_eq!(result, vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn split_trailing_semicolon_filtered() {
        let result = split_chained_commands("cmd;");
        assert_eq!(result, vec!["cmd"]);
    }

    #[test]
    fn split_empty_input() {
        let result = split_chained_commands("");
        assert_eq!(result, vec![] as Vec<&str>);
    }

    #[test]
    fn split_whitespace_trimmed() {
        let result = split_chained_commands("  git commit -m 'feat: && stuff'  ");
        assert_eq!(result, vec!["git commit -m 'feat: && stuff'"]);
    }

    #[test]
    fn split_pipe_operator() {
        let result = split_chained_commands("cat file.txt | grep pattern");
        assert_eq!(result, vec!["cat file.txt", "grep pattern"]);
    }

    #[test]
    fn split_or_operator() {
        let result = split_chained_commands("cmd1 || cmd2");
        assert_eq!(result, vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_semicolon_operator() {
        let result = split_chained_commands("cmd1; cmd2");
        assert_eq!(result, vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_mixed_quotes() {
        let result = split_chained_commands("echo 'single' && echo \"double\"");
        assert_eq!(result, vec!["echo 'single'", "echo \"double\""]);
    }

    #[test]
    fn split_nested_quotes_in_args() {
        let result = split_chained_commands("echo \"it's working\" && echo 'done'");
        assert_eq!(result, vec!["echo \"it's working\"", "echo 'done'"]);
    }

    #[test]
    fn split_multiple_spaces_between_operators() {
        let result = split_chained_commands("cmd1  &&  cmd2");
        assert_eq!(result, vec!["cmd1", "cmd2"]);
    }

    #[test]
    fn split_complex_command_with_args() {
        let result = split_chained_commands("cargo test --release && cargo build");
        assert_eq!(result, vec!["cargo test --release", "cargo build"]);
    }

    #[test]
    fn engine_layer_0_denies_destructive_bash() {
        let engine = PermissionEngine::new(None);
        let decision = engine.evaluate("bash", "rm -rf /", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_deny_rule_denies_matching_input() {
        let config = config_with_rules(PermissionMode::Ask, &[], &["bash:rm *"], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "rm something", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_ask_rule_returns_ask() {
        let config = config_with_rules(PermissionMode::Ask, &[], &[], &["bash:git push *"]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "git push origin main", true);
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn engine_allow_rule_returns_allow() {
        let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "cargo test", true);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn engine_deny_wins_over_allow() {
        let config = config_with_rules(PermissionMode::Ask, &["bash:rm *"], &["bash:rm *"], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "rm some-file", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_chained_command_denies_on_layer_0_subcommand() {
        let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "cargo test && rm -rf /", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_chained_command_allows_only_when_all_subcommands_allowed() {
        let config = config_with_rules(
            PermissionMode::Ask,
            &["bash:cargo test*", "bash:cargo build"],
            &[],
            &[],
        );
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "cargo test && cargo build", true);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn engine_chained_command_partial_allow_falls_back_to_ask() {
        let config = config_with_rules(PermissionMode::Ask, &["bash:cargo test*"], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "cargo test && npm run build", true);
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn engine_file_path_normalization_blocks_traversal_input() {
        let config = config_with_rules(PermissionMode::Ask, &[], &["read:.env*"], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("read", "src/../.env", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_non_interactive_ask_becomes_deny() {
        let config = config_with_rules(PermissionMode::Ask, &[], &[], &["bash:*"]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "ls", false);
        assert_eq!(
            decision,
            PermissionDecision::Deny {
                reason: "Non-interactive mode: ask rules become deny".to_string()
            }
        );
    }

    #[test]
    fn engine_non_interactive_allow_still_allows() {
        let config = config_with_rules(PermissionMode::Ask, &["bash:*"], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "ls", false);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn engine_with_no_config_defaults_to_ask() {
        let engine = PermissionEngine::new(None);
        let decision = engine.evaluate("bash", "ls", true);
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn engine_default_allow_when_no_rules_match() {
        let config = config_with_rules(PermissionMode::Allow, &[], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "unknown", true);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn engine_default_deny_when_no_rules_match() {
        let config = config_with_rules(PermissionMode::Deny, &[], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("bash", "unknown", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn engine_allows_matching_mcp_server_rule() {
        let config = config_with_rules(PermissionMode::Ask, &["mcp:github:*"], &[], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("mcp", "github:create_issue", true);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn engine_file_rules_use_most_restrictive_raw_or_normalized_match() {
        let config = config_with_rules(PermissionMode::Ask, &[], &["write:src/../secret.txt"], &[]);
        let engine = PermissionEngine::new(Some(&config));

        let decision = engine.evaluate("write", "src/../secret.txt", true);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    // --- Headless permission flow tests ---
    // These test the PermissionConfig::default() path (no explicit rules),
    // verifying is_interactive=false converts Ask→Deny at the engine level.

    #[test]
    fn non_interactive_ask_default_becomes_deny() {
        // Default config has default=Ask with no rules.
        // A non-interactive evaluation should convert Ask→Deny.
        let config = PermissionConfig::default();
        let engine = PermissionEngine::new(Some(&config));
        let decision = engine.evaluate("dangerous_tool", "{}", false);
        assert_eq!(
            decision,
            PermissionDecision::Deny {
                reason: "Non-interactive mode: ask rules become deny".to_string()
            }
        );
    }

    #[test]
    fn non_interactive_allow_default_stays_allow() {
        // When default mode is Allow and no deny rules match,
        // non-interactive should still allow (only Ask→Deny conversion).
        let mut config = PermissionConfig::default();
        config.default = PermissionMode::Allow;
        let engine = PermissionEngine::new(Some(&config));
        let decision = engine.evaluate("some_tool", "{}", false);
        assert_eq!(decision, PermissionDecision::Allow);
    }

    #[test]
    fn deny_rules_enforced_even_with_allow_default() {
        // Explicit deny rules should fire even when default=Allow.
        let mut config = PermissionConfig::default();
        config.default = PermissionMode::Allow;
        config.deny = vec!["bash:rm *".to_string()];
        let engine = PermissionEngine::new(Some(&config));
        let decision = engine.evaluate("bash", "rm /tmp/test.txt", false);
        assert!(
            matches!(decision, PermissionDecision::Deny { .. }),
            "deny rule should override allow default, got: {decision:?}"
        );
    }

    #[test]
    fn non_interactive_deny_default_stays_deny() {
        // When default mode is Deny and no allow rules match,
        // non-interactive should deny (no conversion needed, already Deny).
        let mut config = PermissionConfig::default();
        config.default = PermissionMode::Deny;
        let engine = PermissionEngine::new(Some(&config));
        let decision = engine.evaluate("dangerous_tool", "{}", false);
        assert!(matches!(decision, PermissionDecision::Deny { .. }));
    }

    #[test]
    fn interactive_ask_default_returns_ask() {
        // Same default config but interactive=true should return Ask, not Deny.
        let config = PermissionConfig::default();
        let engine = PermissionEngine::new(Some(&config));
        let decision = engine.evaluate("dangerous_tool", "{}", true);
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn permission_mode_from_str_valid() {
        assert_eq!(
            "allow".parse::<PermissionMode>().unwrap(),
            PermissionMode::Allow
        );
        assert_eq!(
            "deny".parse::<PermissionMode>().unwrap(),
            PermissionMode::Deny
        );
        assert_eq!(
            "ask".parse::<PermissionMode>().unwrap(),
            PermissionMode::Ask
        );
        assert_eq!(
            "ALLOW".parse::<PermissionMode>().unwrap(),
            PermissionMode::Allow
        );
        assert_eq!(
            "Allow".parse::<PermissionMode>().unwrap(),
            PermissionMode::Allow
        );
    }

    #[test]
    fn permission_mode_from_str_invalid() {
        let err = "bogus".parse::<PermissionMode>().unwrap_err();
        assert!(err.contains("Invalid permission mode"));
        assert!(err.contains("bogus"));
    }

    #[test]
    fn permission_mode_display_roundtrip() {
        assert_eq!(PermissionMode::Allow.to_string(), "allow");
        assert_eq!(PermissionMode::Deny.to_string(), "deny");
        assert_eq!(PermissionMode::Ask.to_string(), "ask");
    }

    #[test]
    fn write_permission_rule_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        write_permission_rule(
            PermissionScope::Project,
            "bash:cargo test *",
            Some(dir.path()),
        )
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("crucible.toml")).unwrap();
        let table: toml::Table = toml::from_str(&content).unwrap();
        let allow = table["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0].as_str().unwrap(), "bash:cargo test *");
    }

    #[test]
    fn write_permission_rule_appends_to_existing_allow_array() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crucible.toml");
        std::fs::write(&path, "[permissions]\nallow = [\"read:*\"]\n").unwrap();

        write_permission_rule(
            PermissionScope::Project,
            "bash:cargo test *",
            Some(dir.path()),
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let table: toml::Table = toml::from_str(&content).unwrap();
        let allow = table["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 2);
        assert_eq!(allow[0].as_str().unwrap(), "read:*");
        assert_eq!(allow[1].as_str().unwrap(), "bash:cargo test *");
    }

    #[test]
    fn write_permission_rule_skips_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        write_permission_rule(
            PermissionScope::Project,
            "bash:cargo test *",
            Some(dir.path()),
        )
        .unwrap();
        write_permission_rule(
            PermissionScope::Project,
            "bash:cargo test *",
            Some(dir.path()),
        )
        .unwrap();

        let content = std::fs::read_to_string(dir.path().join("crucible.toml")).unwrap();
        let table: toml::Table = toml::from_str(&content).unwrap();
        let allow = table["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
    }

    #[test]
    fn write_permission_rule_adds_section_to_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crucible.toml");
        std::fs::write(&path, "[llm]\nmodel = \"gpt-4\"\n").unwrap();

        write_permission_rule(PermissionScope::Project, "read:*", Some(dir.path())).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let table: toml::Table = toml::from_str(&content).unwrap();
        assert_eq!(table["llm"]["model"].as_str().unwrap(), "gpt-4");
        let allow = table["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0].as_str().unwrap(), "read:*");
    }

    #[test]
    fn write_permission_rule_user_scope_uses_config_toml() {
        let dir = tempfile::tempdir().unwrap();
        write_permission_rule(PermissionScope::User, "bash:*", Some(dir.path())).unwrap();

        let path = dir.path().join("config.toml");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let table: toml::Table = toml::from_str(&content).unwrap();
        let allow = table["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow[0].as_str().unwrap(), "bash:*");
    }
}
