//! The default matcher: it turns an ACP tool call into a [`CanonicalToolCall`].
//!
//! The matcher reads the fields in a fixed order. Each step sets only the
//! fields that an earlier step did not set:
//!
//! 1. Diff content. A diff makes the call a `file_edit`.
//! 2. An MCP tool name (`mcp__server__tool` or `mcp.server.tool`).
//! 3. The ACP `kind` and the `locations`.
//! 4. The agent key table: the `tools` field of the agent profile. The
//!    shipped defaults in `runtime/defaults/init.luau` set it.
//! 5. The usual `rawInput` keys.
//! 6. The fallback kind `tool`.
//!
//! The ACP spec gives `kind` only for icons, so an agent can send any kind.
//! The diff is a fact about the call, so it comes first.
//!
//! A call to Crucible's own MCP server skips these steps. It is a call to a
//! Crucible tool, so [`CanonicalToolCall::crucible_tool`] classifies it.

use agent_client_protocol_schema::v1::{
    ToolCall, ToolCallContent, ToolCallLocation, ToolCallUpdate, ToolKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::acp::FileDiff;
use super::tool_call::CanonicalToolCall;

/// The fields of an ACP tool call that a matcher or a display can read.
///
/// This is provenance. No policy reads it. It has no status and no
/// `rawOutput`, because those describe the result, not the call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_input: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<ToolCallLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ToolCallContent>,
    /// The ACP `_meta` object, as opaque JSON.
    #[serde(default, rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl From<&ToolCall> for RawToolCall {
    fn from(call: &ToolCall) -> Self {
        Self {
            title: Some(call.title.clone()).filter(|t| !t.is_empty()),
            name: call.name.clone(),
            kind: Some(call.kind),
            raw_input: call.raw_input.clone(),
            locations: call.locations.clone(),
            content: call.content.clone(),
            meta: call.meta.clone().map(Value::Object),
        }
    }
}

impl From<&ToolCallUpdate> for RawToolCall {
    fn from(update: &ToolCallUpdate) -> Self {
        let f = &update.fields;
        Self {
            title: f.title.clone(),
            name: f.name.clone(),
            kind: f.kind,
            raw_input: f.raw_input.clone(),
            locations: f.locations.clone().unwrap_or_default(),
            content: f.content.clone().unwrap_or_default(),
            meta: update.meta.clone().map(Value::Object),
        }
    }
}

impl RawToolCall {
    /// Merge a later frame into this call. Each field that the frame sets
    /// replaces the field here. An empty list sets nothing, so a frame
    /// without `locations` or `content` keeps the ones that came before.
    pub fn merge(&mut self, frame: RawToolCall) {
        let RawToolCall {
            title,
            name,
            kind,
            raw_input,
            locations,
            content,
            meta,
        } = frame;
        self.title = title.or(self.title.take());
        self.name = name.or(self.name.take());
        self.kind = kind.or(self.kind.take());
        self.raw_input = raw_input.or(self.raw_input.take());
        self.meta = meta.or(self.meta.take());
        if !locations.is_empty() {
            self.locations = locations;
        }
        if !content.is_empty() {
            self.content = content;
        }
    }
}

/// One entry of an agent key table.
///
/// An entry applies to a call when each match field that it sets matches.
/// An entry that sets no match field applies to each call of the agent.
///
/// A key is a key of the argument object, for example `file_path`. The
/// argument object is `rawInput`, or the value of the `args` key. A key that
/// starts with `/` is a JSON pointer into the whole [`RawToolCall`], for
/// example `/title` or `/_meta/mcp/tool`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentKeys {
    /// Match: the ACP `name` is equal to this.
    pub name: Option<String>,
    /// Match: the ACP `kind` is equal to this.
    pub acp_kind: Option<ToolKind>,
    /// Match: the ACP `title` contains this text. The text is plain, not a
    /// pattern: a Lua pattern needs a Lua VM for each call.
    pub title: Option<String>,
    /// The canonical kind, if the diff and the ACP `kind` give no kind.
    pub kind: Option<String>,
    /// Keys for the argument object, if the agent wraps it. For example,
    /// codex sends an MCP call as `{server, tool, arguments}`.
    pub args: Vec<String>,
    /// Keys for the canonical tool name. A value here wins over the ACP
    /// `name`, because the `name` is only a match field.
    pub tool: Vec<String>,
    pub command: Vec<String>,
    pub paths: Vec<String>,
    pub url: Vec<String>,
    pub query: Vec<String>,
}

/// The MCP name prefixes of Crucible's own MCP server.
const CRUCIBLE_MCP_PREFIXES: &[&str] = &["mcp__crucible__", "mcp.crucible."];

/// The usual `rawInput` keys that the default matcher reads.
const USUAL_PATH_KEYS: &[&str] = &["file_path", "filePath", "path"];
const USUAL_URL_KEYS: &[&str] = &["url"];
const USUAL_QUERY_KEYS: &[&str] = &["query", "pattern"];

/// Classify an ACP tool call. `table` is the key table of the agent.
pub fn classify_acp(raw: RawToolCall, table: &[AgentKeys]) -> CanonicalToolCall {
    let whole = serde_json::to_value(&raw).unwrap_or(Value::Null);
    let raw_input = raw.raw_input.clone().unwrap_or(Value::Null);
    let entries: Vec<&AgentKeys> = table.iter().filter(|e| entry_matches(e, &raw)).collect();
    let input = entries
        .iter()
        .flat_map(|e| &e.args)
        .find_map(|k| lookup(&whole, &raw_input, k))
        .cloned()
        .unwrap_or_else(|| raw_input.clone());

    // An MCP tool name is also a name when it is only in the title, because
    // the TypeScript codex adapter puts it there.
    let mcp_name = raw
        .name
        .clone()
        .or_else(|| raw.title.clone())
        .filter(|n| is_mcp_name(n));

    // One hook must match a native call and an MCP call of one Crucible tool.
    if let Some(tool) = mcp_name.as_deref().and_then(crucible_mcp_tool) {
        let mut call = CanonicalToolCall::crucible_tool(tool, &input);
        call.diffs = file_diffs(&raw.content);
        call.raw = Some(raw);
        return call;
    }

    let mut call = CanonicalToolCall {
        kind: String::new(),
        tool: String::new(),
        command: None,
        paths: Vec::new(),
        url: None,
        query: None,
        diffs: Vec::new(),
        agent: None,
        raw: None,
        render: None,
    };

    // 1. Diff content.
    let diff_paths: Vec<String> = raw
        .content
        .iter()
        .filter_map(|c| match c {
            ToolCallContent::Diff(d) => Some(d.path.display().to_string()),
            _ => None,
        })
        .collect();
    if !diff_paths.is_empty() {
        call.kind = "file_edit".into();
        call.paths = diff_paths;
    }
    call.diffs = file_diffs(&raw.content);

    // 2. An MCP tool name.
    if call.kind.is_empty() && mcp_name.is_some() {
        call.kind = "mcp_tool".into();
    }

    // 3. The ACP kind and the locations.
    if call.kind.is_empty() {
        call.kind = raw.kind.and_then(acp_kind_name).unwrap_or_default().into();
    }
    if call.paths.is_empty() {
        call.paths = raw
            .locations
            .iter()
            .map(|l| l.path.display().to_string())
            .collect();
    }

    // 4. The agent key table.
    for e in &entries {
        if call.kind.is_empty() {
            call.kind = e.kind.clone().unwrap_or_default();
        }
        fill(
            &mut call, &whole, &input, &e.command, &e.paths, &e.url, &e.query,
        );
    }

    // 5. The usual keys. A `command` key makes a command line only for a
    // command call: another tool can take a `command` argument.
    let command_keys: &[&str] = if call.kind == "command" {
        &["command"]
    } else {
        &[]
    };
    fill(
        &mut call,
        &whole,
        &input,
        command_keys,
        USUAL_PATH_KEYS,
        USUAL_URL_KEYS,
        USUAL_QUERY_KEYS,
    );

    // 6. The fallback. A command or a file kind with no command line or no
    // path is also `tool`, so a command rule never matches an empty command.
    let empty = match call.kind.as_str() {
        "" => true,
        "command" => call.command.is_none(),
        "file_edit" | "file_read" => call.paths.is_empty(),
        _ => false,
    };
    if empty {
        call.kind = "tool".into();
    }

    // ACP agents do not agree on `fetch` and `search` for a web search. The
    // field that the call has decides.
    match (call.kind.as_str(), &call.url, &call.query) {
        ("fetch", None, Some(_)) => call.kind = "search".into(),
        ("search", Some(_), None) => call.kind = "fetch".into(),
        _ => {}
    }

    // A call that nothing names gets its kind as the name, so a UI and a
    // rule always have a name to show or to match.
    call.tool = entries
        .iter()
        .find_map(|e| first_string(&whole, &input, &e.tool))
        .or(mcp_name)
        .or_else(|| raw.name.clone())
        .unwrap_or_else(|| call.kind.clone());
    call.raw = Some(raw);
    call
}

/// The file diffs in the content of a call. A diff over
/// [`MAX_DIFF_BYTES`](super::acp::MAX_DIFF_BYTES) is dropped, so no UI must
/// hold or draw it.
fn file_diffs(content: &[ToolCallContent]) -> Vec<FileDiff> {
    content
        .iter()
        .filter_map(|c| match c {
            ToolCallContent::Diff(d) => Some(FileDiff::from_contents(
                d.path.display().to_string(),
                d.old_text.clone(),
                d.new_text.clone(),
            )),
            _ => None,
        })
        .filter(|d| !d.is_oversize())
        .collect()
}

/// The Crucible tool that an MCP name of Crucible's own server names.
fn crucible_mcp_tool(name: &str) -> Option<&str> {
    CRUCIBLE_MCP_PREFIXES
        .iter()
        .find_map(|p| name.strip_prefix(p))
}

fn is_mcp_name(name: &str) -> bool {
    (name.starts_with("mcp__") || name.starts_with("mcp.")) && !name.contains(char::is_whitespace)
}

/// The canonical kind for an ACP kind. `other`, `think` and `switch_mode`
/// give no kind, so a later step decides.
fn acp_kind_name(kind: ToolKind) -> Option<&'static str> {
    match kind {
        ToolKind::Read => Some("file_read"),
        ToolKind::Edit | ToolKind::Delete | ToolKind::Move => Some("file_edit"),
        ToolKind::Execute => Some("command"),
        ToolKind::Fetch => Some("fetch"),
        ToolKind::Search => Some("search"),
        _ => None,
    }
}

fn entry_matches(e: &AgentKeys, raw: &RawToolCall) -> bool {
    e.name.as_ref().is_none_or(|n| raw.name.as_ref() == Some(n))
        && e.acp_kind.is_none_or(|k| raw.kind == Some(k))
        && e.title
            .as_ref()
            .is_none_or(|t| raw.title.as_ref().is_some_and(|rt| rt.contains(t.as_str())))
}

/// Set each field that is still empty from the first key that has a value.
fn fill<S: AsRef<str>>(
    call: &mut CanonicalToolCall,
    whole: &Value,
    input: &Value,
    command: &[S],
    paths: &[S],
    url: &[S],
    query: &[S],
) {
    if call.command.is_none() {
        call.command = command
            .iter()
            .find_map(|k| lookup(whole, input, k.as_ref()).and_then(command_line));
    }
    if call.paths.is_empty() {
        call.paths = paths
            .iter()
            .find_map(|k| lookup(whole, input, k.as_ref()).and_then(string))
            .into_iter()
            .collect();
    }
    if call.url.is_none() {
        call.url = first_string(whole, input, url);
    }
    if call.query.is_none() {
        call.query = first_string(whole, input, query);
    }
}

fn first_string<S: AsRef<str>>(whole: &Value, input: &Value, keys: &[S]) -> Option<String> {
    keys.iter()
        .find_map(|k| lookup(whole, input, k.as_ref()).and_then(string))
}

fn lookup<'a>(whole: &'a Value, input: &'a Value, key: &str) -> Option<&'a Value> {
    if key.starts_with('/') {
        whole.pointer(key)
    } else {
        input.get(key)
    }
}

fn string(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// A command line from a string, or from an argv array. The array form
/// `[shell, "-c", script]` gives the script, because the user reads the
/// script and not the shell that runs it.
fn command_line(v: &Value) -> Option<String> {
    if let Some(s) = string(v) {
        return Some(s);
    }
    let argv: Vec<&str> = v.as_array()?.iter().filter_map(Value::as_str).collect();
    match argv.as_slice() {
        [] => None,
        [_, "-c" | "-lc", script] => Some((*script).to_string()),
        _ => Some(argv.join(" ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(v: Value) -> RawToolCall {
        serde_json::from_value(v).expect("a raw tool call")
    }

    fn diff(path: &str) -> Value {
        json!({"type": "diff", "path": path, "oldText": "a", "newText": "b"})
    }

    #[test]
    fn a_diff_outranks_the_acp_kind() {
        let c = classify_acp(
            raw(json!({"kind": "read", "content": [diff("/a.rs")]})),
            &[],
        );
        assert_eq!(c.kind, "file_edit");
        assert_eq!(c.paths, ["/a.rs"]);
    }

    #[test]
    fn a_diff_outranks_the_key_table() {
        let table = [AgentKeys {
            kind: Some("command".into()),
            paths: vec!["/title".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(json!({"title": "t.rs", "content": [diff("/a.rs")]})),
            &table,
        );
        assert_eq!(c.kind, "file_edit");
        assert_eq!(c.paths, ["/a.rs"]);
    }

    #[test]
    fn the_acp_kind_outranks_the_key_table() {
        let table = [AgentKeys {
            kind: Some("mcp_tool".into()),
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(json!({"kind": "read", "locations": [{"path": "/a"}]})),
            &table,
        );
        assert_eq!(c.kind, "file_read");
    }

    #[test]
    fn locations_outrank_the_key_table() {
        let table = [AgentKeys {
            paths: vec!["target".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(
                json!({"kind": "read", "locations": [{"path": "/loc"}], "rawInput": {"target": "/key"}}),
            ),
            &table,
        );
        assert_eq!(c.paths, ["/loc"]);
    }

    #[test]
    fn the_key_table_gives_the_kind_when_the_acp_kind_is_other() {
        let table = [AgentKeys {
            acp_kind: Some(ToolKind::Other),
            kind: Some("mcp_tool".into()),
            ..AgentKeys::default()
        }];
        let c = classify_acp(raw(json!({"kind": "other"})), &table);
        assert_eq!(c.kind, "mcp_tool");
    }

    #[test]
    fn the_key_table_outranks_the_usual_keys() {
        let table = [AgentKeys {
            paths: vec!["target".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(json!({"rawInput": {"target": "/key", "path": "/usual"}})),
            &table,
        );
        assert_eq!(c.paths, ["/key"]);
    }

    #[test]
    fn an_entry_applies_only_when_each_match_field_matches() {
        let table = [AgentKeys {
            name: Some("Bash".into()),
            title: Some("ls".into()),
            kind: Some("command".into()),
            command: vec!["/title".into()],
            ..AgentKeys::default()
        }];
        let hit = classify_acp(raw(json!({"name": "Bash", "title": "ls src"})), &table);
        assert_eq!(hit.kind, "command");
        let miss = classify_acp(raw(json!({"name": "Bash", "title": "pwd"})), &table);
        assert_eq!(miss.kind, "tool");
    }

    #[test]
    fn a_pointer_key_reads_the_whole_call() {
        let table = [AgentKeys {
            command: vec!["/title".into()],
            tool: vec!["/_meta/mcp/tool".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(json!({"kind": "execute", "title": "ls -la", "_meta": {"mcp": {"tool": "x"}}})),
            &table,
        );
        assert_eq!(c.command.as_deref(), Some("ls -la"));
        assert_eq!(c.tool, "x");
    }

    #[test]
    fn the_fallback_kind_is_tool() {
        let c = classify_acp(raw(json!({"title": "Something"})), &[]);
        assert_eq!(c.kind, "tool");
        assert_eq!(c.tool, "tool", "a call with no name is named by its kind");
    }

    #[test]
    fn the_usual_command_key_needs_a_command_kind() {
        let c = classify_acp(
            raw(json!({"name": "run_task", "rawInput": {"command": "build"}})),
            &[],
        );
        assert_eq!(c.kind, "tool");
        assert_eq!(c.command, None);
    }

    #[test]
    fn an_argv_command_gives_the_shell_script() {
        let c = classify_acp(
            raw(
                json!({"kind": "execute", "rawInput": {"command": ["/bin/zsh", "-lc", "cargo test"]}}),
            ),
            &[],
        );
        assert_eq!(c.command.as_deref(), Some("cargo test"));
        let c = classify_acp(
            raw(json!({"kind": "execute", "rawInput": {"command": ["git", "status"]}})),
            &[],
        );
        assert_eq!(c.command.as_deref(), Some("git status"));
    }

    #[test]
    fn a_crucible_mcp_call_classifies_as_the_native_tool() {
        let args = json!({"path": "notes/a.md"});
        let native = CanonicalToolCall::crucible_tool("update_note", &args);
        assert_eq!(native.kind, "file_edit");
        for name in ["mcp__crucible__update_note", "mcp.crucible.update_note"] {
            let mut c = classify_acp(raw(json!({"name": name, "rawInput": args})), &[]);
            assert!(c.raw.take().is_some(), "{name}: the raw call stays");
            assert_eq!(c, native, "{name}");
        }
    }

    #[test]
    fn the_args_key_names_the_argument_object() {
        let table = [AgentKeys {
            args: vec!["/rawInput/arguments".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(
            raw(json!({
                "title": "mcp.crucible.search_notes",
                "rawInput": {"server": "crucible", "arguments": {"query": "rust"}}
            })),
            &table,
        );
        assert_eq!(c.kind, "search");
        assert_eq!(c.query.as_deref(), Some("rust"));
    }

    #[test]
    fn a_title_match_is_plain_text() {
        let table = [AgentKeys {
            title: Some("mcp.".into()),
            kind: Some("mcp_tool".into()),
            ..AgentKeys::default()
        }];
        let hit = classify_acp(raw(json!({"title": "mcp.srv"})), &table);
        assert_eq!(hit.kind, "mcp_tool");
        let miss = classify_acp(raw(json!({"title": "mcpxsrv"})), &table);
        assert_eq!(miss.kind, "tool");
    }

    #[test]
    fn the_key_table_gives_the_tool_name() {
        let table = [AgentKeys {
            tool: vec!["/_meta/mcp/tool".into()],
            ..AgentKeys::default()
        }];
        let c = classify_acp(raw(json!({"_meta": {"mcp": {"tool": "x"}}})), &table);
        assert_eq!(c.tool, "x");
        let c = classify_acp(
            raw(json!({"name": "wire", "_meta": {"mcp": {"tool": "x"}}})),
            &table,
        );
        assert_eq!(
            c.tool, "x",
            "the wire name does not override a matched entry"
        );
        let c = classify_acp(raw(json!({"name": "wire"})), &table);
        assert_eq!(c.tool, "wire");
    }

    #[test]
    fn a_typed_kind_with_an_empty_field_is_tool() {
        for kind in ["execute", "read", "edit"] {
            let c = classify_acp(raw(json!({"kind": kind, "rawInput": {}})), &[]);
            assert_eq!(c.kind, "tool", "{kind}");
        }
        let c = classify_acp(
            raw(json!({"kind": "execute", "rawInput": {"command": "ls"}})),
            &[],
        );
        assert_eq!(c.kind, "command");
    }

    #[test]
    fn another_mcp_server_keeps_the_whole_name() {
        let c = classify_acp(raw(json!({"name": "mcp__srv__tool"})), &[]);
        assert_eq!(c.tool, "mcp__srv__tool");
    }

    #[test]
    fn a_web_search_is_search_whatever_the_acp_kind() {
        let c = classify_acp(
            raw(json!({"kind": "fetch", "rawInput": {"query": "q"}})),
            &[],
        );
        assert_eq!(c.kind, "search");
        let c = classify_acp(
            raw(json!({"kind": "search", "rawInput": {"url": "https://a"}})),
            &[],
        );
        assert_eq!(c.kind, "fetch");
    }

    #[test]
    fn the_raw_call_carries_no_status_and_no_raw_output() {
        let call: ToolCall = serde_json::from_value(json!({
            "toolCallId": "1", "title": "t", "status": "completed", "rawOutput": {"x": 1}
        }))
        .unwrap();
        let v = serde_json::to_value(RawToolCall::from(&call)).unwrap();
        assert_eq!(v, json!({"title": "t", "kind": "other"}));
    }
}
