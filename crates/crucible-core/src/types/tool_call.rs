//! The canonical tool call: one description of a tool call, which the daemon
//! computes once. The TUI, the web, the permission gate and the deny messages
//! read it, so they agree on what a call is.
//!
//! A kind is an open name, not a closed enum. The ACP boundary and Crucible's
//! own tools produce the same type, so one hook can match both.
//!
//! The call is data. Its [`ToolRender`] is display data that a render
//! function makes from the call in the daemon. Each client draws the render
//! in its own way.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::acp::FileDiff;
use super::tool_match::RawToolCall;

/// Argument keys that name a filesystem target, in priority order.
/// `filePath` is here because agents are inconsistent about casing and the
/// TUI's previous heuristic accepted it; dropping it would silently blank the
/// status row for those calls.
const PATH_KEYS: &[&str] = &["file_path", "filePath", "path", "file", "note", "name"];

/// Argument keys that carry a search query, in priority order.
const QUERY_KEYS: &[&str] = &["pattern", "query"];

/// The kinds that [`CanonicalToolCall::crucible_tool`] and the default ACP
/// matcher give. A kind stays an open name: a plugin or an agent key table
/// can add a kind. The Lua defaults render each kind here but `tool`, which
/// [`ToolRender::fallback`] renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub enum BuiltinKind {
    Command,
    FileEdit,
    FileRead,
    McpTool,
    Fetch,
    Search,
    Tool,
}

impl BuiltinKind {
    /// The kind name on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::FileEdit => "file_edit",
            Self::FileRead => "file_read",
            Self::McpTool => "mcp_tool",
            Self::Fetch => "fetch",
            Self::Search => "search",
            Self::Tool => "tool",
        }
    }

    /// The built-in kind with the name `kind`, or `None` for another kind.
    pub fn parse(kind: &str) -> Option<Self> {
        <Self as strum::IntoEnumIterator>::iter().find(|k| k.as_str() == kind)
    }
}

/// One tool call, classified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalToolCall {
    /// The open kind name. The default matcher and [`Self::crucible_tool`]
    /// give a [`BuiltinKind`].
    pub kind: String,
    /// The canonical tool name. A display object from before this field has
    /// no name, so the default keeps an old transcript readable.
    #[serde(default)]
    pub tool: String,
    /// The shell command line, for a `command` call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The filesystem targets of the call.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// The file changes of the call. An ACP call has the diff content of its
    /// frames. A Crucible tool call has the diff that `diff_synth` makes.
    #[serde(
        default,
        deserialize_with = "lenient_diffs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub diffs: Vec<FileDiff>,
    /// The ACP agent that made the call. `None` for Crucible's own tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The fields of an ACP call, for matchers and display. No policy
    /// reads it. `None` for Crucible's own tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<RawToolCall>,
    /// What the render function of the kind says about the call. The daemon
    /// sets it before the call goes on the wire. `None` in a transcript from
    /// before the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<ToolRender>,
}

/// Display data for one tool call. A render function makes it: a Lua
/// function for a kind, or [`ToolRender::fallback`].
///
/// It holds meaning, not terminal text and not HTML. Each client draws the
/// line and the fields in its own way.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ToolRender {
    /// The one line that says what the call does, for example a command
    /// line, a path, a URL or a query.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The other facts of the call, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<RenderField>,
}

/// One fact of a [`ToolRender`]. The value is JSON, so a client can draw a
/// structured value, for example `rawInput`, in its own way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderField {
    pub label: String,
    pub value: Value,
}

impl ToolRender {
    /// The render of a kind that has no render function, and of each call
    /// when a render function fails.
    ///
    /// It shows every field of the call and never hides one: the kind, the
    /// tool, and the ACP title, name, kind, `rawInput`, locations and
    /// content. `args` is the `rawInput` of a Crucible tool. It never shows
    /// the call as another kind.
    pub fn fallback(call: &CanonicalToolCall, args: &Value) -> Self {
        let raw = call.raw.as_ref();
        let input = raw.and_then(|r| r.raw_input.as_ref()).unwrap_or(args);
        let mut fields = vec![
            field("kind", Value::from(call.kind.as_str())),
            field("tool", Value::from(call.tool.as_str())),
        ];
        if let Some(raw) = raw {
            let json = serde_json::to_value(raw).unwrap_or_default();
            for (key, label) in [
                ("title", "title"),
                ("name", "name"),
                ("kind", "acp kind"),
                ("locations", "locations"),
                ("content", "content"),
            ] {
                if let Some(value) = json.get(key) {
                    fields.push(field(label, value.clone()));
                }
            }
        }
        if !input.is_null() {
            fields.push(field("rawInput", input.clone()));
        }
        // The line is the title, or else the first plain value of the input,
        // so the one-line form of an unknown tool still says something.
        let line = raw.and_then(|r| r.title.clone()).or_else(|| match input {
            Value::Object(map) => map.values().find_map(scalar_to_string),
            other => scalar_to_string(other),
        });
        Self { line, fields }
    }
}

fn field(label: &str, value: Value) -> RenderField {
    RenderField {
        label: label.to_string(),
        value,
    }
}

impl CanonicalToolCall {
    /// Tool names whose payload is a shell command line.
    ///
    /// Matched exactly, or as the tail of an MCP-prefixed name
    /// (`server__bash`), so an ordinary tool that merely takes a `command`
    /// argument is unaffected.
    ///
    /// Public because this is the *only* list of command tools: a caller that
    /// must enumerate them — a permission gate, or a test that proves one —
    /// reads it here instead of writing a second list beside it. A second list
    /// drifted once already; see `PermRequest::suggested_pattern`.
    pub const COMMAND_TOOL_NAMES: &'static [&'static str] = &[
        "bash",
        "shell",
        "sh",
        "zsh",
        "exec",
        "run_command",
        "terminal",
    ];

    /// Crucible's tools that change a file or a note. A path argument makes
    /// a call to one of them `file_edit`. The permission gate reads this
    /// list too.
    pub const FILE_EDIT_TOOL_NAMES: &'static [&'static str] = &[
        "write_file",
        "edit_file",
        "create_note",
        "update_note",
        "delete_note",
    ];

    /// Crucible's tools that only read a file or a note. A path argument
    /// makes a call to one of them `file_read`. A path argument of another
    /// tool gives kind `tool`, because a plugin tool with a path can delete
    /// or rename, and a `read` rule must not allow it.
    pub const FILE_READ_TOOL_NAMES: &'static [&'static str] =
        &["read_file", "read_note", "read_metadata", "glob", "grep"];

    /// Classify a call to one of Crucible's own tools.
    ///
    /// `args` is the raw argument object. A non-object (a bare string, or
    /// nothing) yields the fallback `tool`; its fallback render shows the
    /// string, because some agents pass a single positional argument.
    pub fn crucible_tool(tool_name: &str, args: &Value) -> Self {
        let call = |kind: BuiltinKind| Self {
            kind: kind.as_str().to_string(),
            tool: tool_name.to_string(),
            command: None,
            paths: Vec::new(),
            url: None,
            query: None,
            diffs: Vec::new(),
            agent: None,
            raw: None,
            render: None,
        };

        // A shell tool is a command also when its command line is absent,
        // so that a `bash` deny rule still applies to it.
        if is_shell_tool(tool_name) {
            return Self {
                command: args.as_object().and_then(|m| first_string(m, &["command"])),
                ..call(BuiltinKind::Command)
            };
        }

        let Some(map) = args.as_object() else {
            return call(BuiltinKind::Tool);
        };

        if let Some(path) = first_string(map, PATH_KEYS) {
            let kind = if Self::FILE_EDIT_TOOL_NAMES.contains(&tool_name) {
                BuiltinKind::FileEdit
            } else if Self::FILE_READ_TOOL_NAMES.contains(&tool_name) {
                BuiltinKind::FileRead
            } else {
                BuiltinKind::Tool
            };
            return Self {
                paths: vec![path],
                ..call(kind)
            };
        }
        if let Some(query) = first_string(map, QUERY_KEYS) {
            return Self {
                query: Some(query),
                ..call(BuiltinKind::Search)
            };
        }
        if let Some(url) = first_string(map, &["url"]) {
            return Self {
                url: Some(url),
                ..call(BuiltinKind::Fetch)
            };
        }

        call(BuiltinKind::Tool)
    }

    /// The render line in one line, truncated on character boundaries.
    ///
    /// Multi-line commands collapse to their first line — a status row has one
    /// line to work with, and the full text lives in the expanded view.
    pub fn summary(&self, max_chars: usize) -> Option<String> {
        let primary = self.render.as_ref()?.line.as_deref()?;
        let first_line = primary.lines().next().unwrap_or(primary);
        let truncated: String = first_line.chars().take(max_chars).collect();
        if truncated.chars().count() < first_line.chars().count() || primary.contains('\n') {
            Some(format!("{truncated}…"))
        } else {
            Some(truncated)
        }
    }
}

/// Deserialize `diffs` tolerantly: a value that is not a `Vec<FileDiff>`
/// gives an empty Vec, and the rest of the call still loads. A tool card is
/// worth a render without its diff, so a malformed diff must not drop the
/// whole event.
fn lenient_diffs<'de, D>(deserializer: D) -> Result<Vec<FileDiff>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Value::deserialize(deserializer)?;
    if raw.is_null() {
        return Ok(Vec::new());
    }
    match serde_json::from_value(raw.clone()) {
        Ok(diffs) => Ok(diffs),
        Err(e) => {
            tracing::warn!(
                error = %e,
                raw = %raw,
                "a tool call carried a malformed `diffs` field; continuing with an empty Vec",
            );
            Ok(Vec::new())
        }
    }
}

pub(super) fn is_shell_tool(tool_name: &str) -> bool {
    let lower = tool_name.to_ascii_lowercase();
    CanonicalToolCall::COMMAND_TOOL_NAMES
        .iter()
        .any(|t| lower == *t || lower.ends_with(&format!("__{t}")))
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

    fn of(tool_name: &str, args: &Value) -> CanonicalToolCall {
        CanonicalToolCall::crucible_tool(tool_name, args)
    }

    #[test]
    fn a_shell_call_projects_as_a_command() {
        let d = of("bash", &json!({"command": "ls -la"}));
        assert_eq!(d.kind, "command");
        assert_eq!(d.tool, "bash");
        assert_eq!(d.command.as_deref(), Some("ls -la"));
    }

    #[test]
    fn shell_detection_is_case_insensitive_and_survives_mcp_prefixes() {
        for name in ["Bash", "BASH", "myserver__bash", "shell"] {
            let d = of(name, &json!({"command": "ls"}));
            assert_eq!(d.kind, "command", "{name} should be shell");
        }
    }

    /// The distinction the web's local heuristic could not make: a tool that
    /// merely takes a `command` argument is not a shell.
    #[test]
    fn a_non_shell_tool_with_a_command_arg_is_not_a_command() {
        let d = of("run_task", &json!({"command": "build"}));
        assert_ne!(d.kind, "command");
        assert_eq!(d.command, None);
    }

    /// A `bash` deny rule must still apply to a shell call with no command
    /// line.
    #[test]
    fn a_shell_call_without_a_command_is_still_a_command() {
        let d = of("bash", &json!({"script": "x"}));
        assert_eq!(d.kind, "command");
        assert_eq!(d.command, None);
    }

    #[test]
    fn paths_outrank_queries() {
        let d = of("grep", &json!({"pattern": "foo", "path": "src/lib.rs"}));
        assert_eq!(d.kind, "file_read");
        assert_eq!(d.paths, ["src/lib.rs"]);
        assert_eq!(d.query, None);
    }

    #[test]
    fn a_query_is_recognised_when_no_path_is_present() {
        let d = of("semantic_search", &json!({"query": "wikilinks"}));
        assert_eq!(d.kind, "search");
        assert_eq!(d.query.as_deref(), Some("wikilinks"));
    }

    /// A query key outranks `url`, as it did when both were query keys.
    #[test]
    fn a_url_is_recognised_when_no_query_is_present() {
        let d = of("fetch", &json!({"url": "https://a.test"}));
        assert_eq!(d.kind, "fetch");
        assert_eq!(d.url.as_deref(), Some("https://a.test"));

        let d = of("fetch", &json!({"url": "https://a.test", "query": "q"}));
        assert_eq!(d.query.as_deref(), Some("q"));
        assert_eq!(d.url, None);
    }

    /// The fallback line of a Crucible tool, which has no title.
    fn fallback_line(tool_name: &str, args: &Value) -> Option<String> {
        ToolRender::fallback(&of(tool_name, args), args).line
    }

    #[test]
    fn an_unrecognised_shape_still_offers_a_string() {
        let args = json!({"whatever": "something"});
        assert_eq!(of("mystery", &args).kind, "tool");
        assert_eq!(
            fallback_line("mystery", &args).as_deref(),
            Some("something")
        );
    }

    #[test]
    fn empty_strings_do_not_count_as_a_primary() {
        let d = of("write_file", &json!({"path": "", "query": ""}));
        assert_eq!(d.kind, "tool");
        assert!(d.paths.is_empty());
    }

    /// Agents are inconsistent about casing; the TUI heuristic this replaced
    /// accepted camelCase, so dropping it would blank those status rows.
    #[test]
    fn camel_case_file_path_is_recognised() {
        let d = of("read_file", &json!({"filePath": "/home/u/x.rs"}));
        assert_eq!(d.kind, "file_read");
        assert_eq!(d.paths, ["/home/u/x.rs"]);
    }

    /// A path makes a Crucible tool `file_edit` only when the tool changes
    /// files, so a rule for `file_edit` matches the same calls as the
    /// permission gate's file rules.
    #[test]
    fn a_path_is_an_edit_only_for_a_tool_that_changes_files() {
        for name in CanonicalToolCall::FILE_EDIT_TOOL_NAMES {
            assert_eq!(
                of(name, &json!({"path": "a.md"})).kind,
                "file_edit",
                "{name}"
            );
        }
        for name in CanonicalToolCall::FILE_READ_TOOL_NAMES {
            assert_eq!(
                of(name, &json!({"path": "a.md"})).kind,
                "file_read",
                "{name}"
            );
        }
        // A tool that Crucible does not know can delete or rename.
        let unknown = of("delete_file", &json!({"path": "a.md"}));
        assert_eq!(unknown.kind, "tool");
        assert_eq!(unknown.paths, ["a.md"]);
    }

    /// A nested object on a one-line status row is noise, not information.
    #[test]
    fn structured_values_are_not_used_as_the_line() {
        let args = json!({"opts": {"a": 1}, "items": [1, 2]});
        assert_eq!(fallback_line("x", &args), None);
    }

    #[test]
    fn no_args_yields_no_line() {
        assert_eq!(fallback_line("noop", &json!({})), None);
    }

    #[test]
    fn a_bare_string_argument_is_used_as_the_line() {
        assert_eq!(
            fallback_line("echo", &json!("hello")).as_deref(),
            Some("hello")
        );
    }

    /// A call whose render line is `line`.
    fn rendered(line: &str) -> CanonicalToolCall {
        CanonicalToolCall {
            render: Some(ToolRender {
                line: Some(line.to_string()),
                fields: Vec::new(),
            }),
            ..of("bash", &json!({}))
        }
    }

    #[test]
    fn summary_truncates_and_marks_it() {
        let s = rendered(&"a".repeat(80)).summary(20).unwrap();
        assert_eq!(s.chars().count(), 21, "20 chars plus the ellipsis");
        assert!(s.ends_with('…'));
    }

    /// A status row has one line; the full command lives in the expanded view.
    #[test]
    fn summary_collapses_a_multi_line_command_to_its_first_line() {
        let s = rendered("cd /tmp\ngrep -r foo .").summary(100).unwrap();
        assert_eq!(s, "cd /tmp…");
        assert!(!s.contains('\n'));
    }

    #[test]
    fn summary_leaves_a_short_single_line_alone() {
        assert_eq!(rendered("ls -la").summary(100).as_deref(), Some("ls -la"));
    }

    /// Truncation counts CHARACTERS: slicing bytes would panic mid-codepoint.
    #[test]
    fn summary_truncates_on_character_boundaries() {
        let s = rendered(&"é".repeat(50)).summary(10).unwrap();
        assert_eq!(s.chars().count(), 11);
    }

    /// Absent fields are omitted rather than sent as null, so a UI checking
    /// for a field's presence behaves the same as one checking its value.
    #[test]
    fn the_wire_form_carries_only_the_fields_that_are_set() {
        let d = of("bash", &json!({"command": "ls"}));
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            json!({"kind": "command", "tool": "bash", "command": "ls"})
        );
        let d = of("noop", &json!({}));
        assert_eq!(
            serde_json::to_value(&d).unwrap(),
            json!({"kind": "tool", "tool": "noop"})
        );
    }

    /// A transcript written before this type holds `{kind, primary}` only.
    #[test]
    fn an_old_display_object_still_loads() {
        let d: CanonicalToolCall =
            serde_json::from_value(json!({"kind": "path", "primary": "a.rs"})).unwrap();
        assert_eq!(d.kind, "path");
        assert_eq!(d.tool, "");
        assert_eq!(d.render, None);
    }

    /// The fallback shows every field of an ACP call and keeps its kind.
    #[test]
    fn the_fallback_shows_every_field_of_the_call() {
        let raw: crate::types::RawToolCall = serde_json::from_value(json!({
            "title": "Do a thing",
            "name": "thing",
            "kind": "other",
            "rawInput": { "x": 1 },
            "locations": [{ "path": "/w/a.rs" }],
            "content": [{ "type": "content", "content": { "type": "text", "text": "hi" } }],
        }))
        .unwrap();
        let call = crate::types::classify_acp(raw, &[]);
        let render = ToolRender::fallback(&call, &Value::Null);

        let labels: Vec<&str> = render.fields.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "kind",
                "tool",
                "title",
                "name",
                "acp kind",
                "locations",
                "content",
                "rawInput"
            ]
        );
        assert_eq!(render.fields[0].value, "tool", "never another kind");
        assert_eq!(render.line.as_deref(), Some("Do a thing"));
    }

    /// A Crucible tool has no ACP fields. Its arguments are its `rawInput`,
    /// and its first plain argument is its line.
    #[test]
    fn the_fallback_of_a_crucible_tool_shows_its_arguments() {
        let args = json!({ "count": 42 });
        let render = ToolRender::fallback(&of("count_things", &args), &args);
        assert_eq!(render.line.as_deref(), Some("42"));
        assert_eq!(render.fields.last().unwrap().value, args);
    }
}
