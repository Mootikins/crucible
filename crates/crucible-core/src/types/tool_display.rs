//! What a tool call *is*, for display purposes — one projection, computed
//! once by the daemon and rendered by every UI.
//!
//! ## Why this exists
//!
//! "Which argument of this tool call is the interesting one" was answered
//! independently in six places: the web `ToolCard`'s icon heuristic, its
//! one-line summary, its shell-tool detection, the web permission prompt's
//! argument formatting, the TUI's `prettify_tool_args`, and the daemon's
//! `brief_resource_description`. Six key-priority lists, drifting apart, and
//! the only one a plugin could influence was the Lua display hint.
//!
//! The knowledge is display-agnostic and the daemon already has it, so it
//! belongs here — "daemon owns business logic, views are thin". A UI asks
//! *how to render*, not *what matters*.
//!
//! ## What this is NOT
//!
//! Not a widget, and not a rendering. It says a bash call's payload is a
//! command and hands over the command text; whether that becomes a `$`-marked
//! block, a truncated status line, or a one-line summary is each UI's
//! business. The shared contract is DATA.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Tool names whose payload is a shell command line.
///
/// Matched exactly, or as the tail of an MCP-prefixed name (`server__bash`),
/// so an ordinary tool that merely takes a `command` argument is unaffected.
const SHELL_TOOLS: &[&str] = &[
    "bash",
    "shell",
    "sh",
    "zsh",
    "exec",
    "run_command",
    "terminal",
];

/// Argument keys that name a filesystem target, in priority order.
/// `filePath` is here because agents are inconsistent about casing and the
/// TUI's previous heuristic accepted it; dropping it would silently blank the
/// status row for those calls.
const PATH_KEYS: &[&str] = &["file_path", "filePath", "path", "file", "note", "name"];

/// Argument keys that carry a search/query string, in priority order.
const QUERY_KEYS: &[&str] = &["pattern", "query", "url"];

/// The shape of a tool call's payload, as far as display is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolDisplayKind {
    /// A shell command line. Render as a command, with newlines intact.
    Command,
    /// A filesystem path.
    Path,
    /// A search pattern, query or URL.
    Query,
    /// Nothing recognised; `primary` is a best-effort first string, if any.
    Other,
}

/// A tool call projected for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDisplay {
    pub kind: ToolDisplayKind,
    /// The argument worth showing, if one could be identified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
}

impl ToolDisplay {
    /// Project a tool call.
    ///
    /// `args` is the raw argument object. A non-object (a bare string, or
    /// nothing) yields `Other` with the string as `primary` where that makes
    /// sense — some agents pass a single positional argument.
    pub fn of(tool_name: &str, args: &Value) -> Self {
        if let Some(command) = shell_command(tool_name, args) {
            return Self {
                kind: ToolDisplayKind::Command,
                primary: Some(command),
            };
        }

        let Some(map) = args.as_object() else {
            return Self {
                kind: ToolDisplayKind::Other,
                primary: args.as_str().filter(|s| !s.is_empty()).map(str::to_string),
            };
        };

        if let Some(path) = first_string(map, PATH_KEYS) {
            return Self {
                kind: ToolDisplayKind::Path,
                primary: Some(path),
            };
        }
        if let Some(query) = first_string(map, QUERY_KEYS) {
            return Self {
                kind: ToolDisplayKind::Query,
                primary: Some(query),
            };
        }

        // Nothing recognised: any non-empty value beats showing the caller a
        // bare tool name with no context. Scalars are stringified — a call
        // whose only argument is `{"count": 42}` should still say "42".
        Self {
            kind: ToolDisplayKind::Other,
            primary: map.values().find_map(scalar_to_string),
        }
    }

    /// A one-line form, truncated on character boundaries.
    ///
    /// Multi-line commands collapse to their first line — a status row has one
    /// line to work with, and the full text lives in the expanded view.
    pub fn summary(&self, max_chars: usize) -> Option<String> {
        let primary = self.primary.as_deref()?;
        let first_line = primary.lines().next().unwrap_or(primary);
        let truncated: String = first_line.chars().take(max_chars).collect();
        if truncated.chars().count() < first_line.chars().count() || primary.contains('\n') {
            Some(format!("{truncated}…"))
        } else {
            Some(truncated)
        }
    }
}

fn is_shell_tool(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    SHELL_TOOLS
        .iter()
        .any(|t| lower == *t || lower.ends_with(&format!("__{t}")))
}

fn shell_command(tool_name: &str, args: &Value) -> Option<String> {
    if !is_shell_tool(tool_name) {
        return None;
    }
    args.get("command")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Render a scalar for display. Objects and arrays are skipped: a JSON blob
/// on a one-line status row is noise, not information.
fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn first_string(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        map.get(*k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_shell_call_projects_as_a_command() {
        let d = ToolDisplay::of("bash", &json!({"command": "ls -la"}));
        assert_eq!(d.kind, ToolDisplayKind::Command);
        assert_eq!(d.primary.as_deref(), Some("ls -la"));
    }

    #[test]
    fn shell_detection_is_case_insensitive_and_survives_mcp_prefixes() {
        for name in ["Bash", "BASH", "myserver__bash", "shell"] {
            let d = ToolDisplay::of(name, &json!({"command": "ls"}));
            assert_eq!(d.kind, ToolDisplayKind::Command, "{name} should be shell");
        }
    }

    /// The distinction the web's local heuristic could not make: a tool that
    /// merely takes a `command` argument is not a shell.
    #[test]
    fn a_non_shell_tool_with_a_command_arg_is_not_a_command() {
        let d = ToolDisplay::of("run_task", &json!({"command": "build"}));
        assert_ne!(d.kind, ToolDisplayKind::Command);
    }

    #[test]
    fn a_shell_call_without_a_command_falls_through() {
        let d = ToolDisplay::of("bash", &json!({"script": "x"}));
        assert_ne!(d.kind, ToolDisplayKind::Command);
    }

    #[test]
    fn paths_outrank_queries() {
        let d = ToolDisplay::of("grep", &json!({"pattern": "foo", "path": "src/lib.rs"}));
        assert_eq!(d.kind, ToolDisplayKind::Path);
        assert_eq!(d.primary.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn a_query_is_recognised_when_no_path_is_present() {
        let d = ToolDisplay::of("semantic_search", &json!({"query": "wikilinks"}));
        assert_eq!(d.kind, ToolDisplayKind::Query);
        assert_eq!(d.primary.as_deref(), Some("wikilinks"));
    }

    #[test]
    fn an_unrecognised_shape_still_offers_a_string() {
        let d = ToolDisplay::of("mystery", &json!({"whatever": "something"}));
        assert_eq!(d.kind, ToolDisplayKind::Other);
        assert_eq!(d.primary.as_deref(), Some("something"));
    }

    #[test]
    fn empty_strings_do_not_count_as_a_primary() {
        let d = ToolDisplay::of("write_file", &json!({"path": "", "query": ""}));
        assert_eq!(d.kind, ToolDisplayKind::Other);
        assert_eq!(d.primary, None);
    }

    /// Agents are inconsistent about casing; the TUI heuristic this replaced
    /// accepted camelCase, so dropping it would blank those status rows.
    #[test]
    fn camel_case_file_path_is_recognised() {
        let d = ToolDisplay::of("Read", &json!({"filePath": "/home/u/x.rs"}));
        assert_eq!(d.kind, ToolDisplayKind::Path);
        assert_eq!(d.primary.as_deref(), Some("/home/u/x.rs"));
    }

    #[test]
    fn a_scalar_argument_is_stringified() {
        let d = ToolDisplay::of("count_things", &json!({"count": 42}));
        assert_eq!(d.primary.as_deref(), Some("42"));
    }

    /// A nested object on a one-line status row is noise, not information.
    #[test]
    fn structured_values_are_not_used_as_a_primary() {
        let d = ToolDisplay::of("x", &json!({"opts": {"a": 1}, "items": [1, 2]}));
        assert_eq!(d.primary, None);
    }

    #[test]
    fn no_args_yields_no_primary() {
        let d = ToolDisplay::of("noop", &json!({}));
        assert_eq!(d.primary, None);
    }

    #[test]
    fn a_bare_string_argument_is_used_as_the_primary() {
        let d = ToolDisplay::of("echo", &json!("hello"));
        assert_eq!(d.primary.as_deref(), Some("hello"));
    }

    #[test]
    fn summary_truncates_and_marks_it() {
        let d = ToolDisplay::of("bash", &json!({"command": "a".repeat(80)}));
        let s = d.summary(20).unwrap();
        assert_eq!(s.chars().count(), 21, "20 chars plus the ellipsis");
        assert!(s.ends_with('…'));
    }

    /// A status row has one line; the full command lives in the expanded view.
    #[test]
    fn summary_collapses_a_multi_line_command_to_its_first_line() {
        let d = ToolDisplay::of("bash", &json!({"command": "cd /tmp\ngrep -r foo ."}));
        let s = d.summary(100).unwrap();
        assert_eq!(s, "cd /tmp…");
        assert!(!s.contains('\n'));
    }

    #[test]
    fn summary_leaves_a_short_single_line_alone() {
        let d = ToolDisplay::of("bash", &json!({"command": "ls -la"}));
        assert_eq!(d.summary(100).as_deref(), Some("ls -la"));
    }

    /// Truncation counts CHARACTERS: slicing bytes would panic mid-codepoint.
    #[test]
    fn summary_truncates_on_character_boundaries() {
        let d = ToolDisplay::of("bash", &json!({"command": "é".repeat(50)}));
        let s = d.summary(10).unwrap();
        assert_eq!(s.chars().count(), 11);
    }

    #[test]
    fn kind_serializes_lowercase_for_the_wire() {
        let d = ToolDisplay::of("bash", &json!({"command": "ls"}));
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["kind"], "command");
        assert_eq!(json["primary"], "ls");
    }

    /// `primary: None` is omitted rather than sent as null, so a UI checking
    /// for the field's presence behaves the same as one checking its value.
    #[test]
    fn an_absent_primary_is_omitted_from_the_wire() {
        let d = ToolDisplay::of("noop", &json!({}));
        let json = serde_json::to_value(&d).unwrap();
        assert!(json.get("primary").is_none());
    }
}
