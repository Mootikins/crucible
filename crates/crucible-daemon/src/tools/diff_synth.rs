//! Synthesize `FileDiff` values from a tool name + raw arguments JSON.
//!
//! Used by the permission flow so a file-mutating tool can show the approver
//! what it intends to change.
//!
//! # This module does not touch the filesystem, and that is the design
//!
//! It used to. `synth_edit` read the whole target file so it could compute
//! "the file before" and "the file after" and let the renderer diff them.
//! That was an unprompted read of a path taken from the model's raw,
//! unapproved arguments, performed *before* the permission gate resolved and
//! published on the session event stream — so `old_string: ""` matched any
//! file and returned its entire body, and clicking Deny could not un-read it.
//!
//! Four rounds of fixes tried to contain that read: confine it to the
//! workspace, hoist the decision above the gate, widen it to the session's
//! real roots. Each round's machinery grew a new edge case — a symlink
//! walker, a check that went stale across a human prompt, a preview scope
//! that normalised paths differently from the executor.
//!
//! None of it was necessary. The arguments already carry the change:
//! `edit_file` names `old_string` and `new_string`, `write_file` names
//! `content`. The read only expanded a known change into whole-file form.
//! An approver needs to see the change, not the file it lands in — so this
//! module renders straight from the arguments and never opens anything.
//!
//! What that costs: no surrounding context lines, and for a whole-file write
//! the old side is unknown. What it buys: no containment, no TOCTOU, no
//! symlink or hard-link handling, no divergence from the executor's own path
//! normalisation, and nothing to exfiltrate. It is also more honest — the
//! popup shows the change requested, not that change speculatively applied to
//! a file state that may differ by the time it executes.
//!
//! Outcome is pure: missing or malformed args, or an unknown tool name,
//! produce an empty `Vec`, never a panic. Either side exceeding
//! `MAX_DIFF_BYTES` is skipped — the renderer would suppress it anyway.

use crucible_core::types::acp::{FileDiff, MAX_DIFF_BYTES};
use serde_json::Value;

/// Build the diffs a write-shaped tool call is proposing, from its arguments
/// alone.
pub fn synthesize_diffs(tool_name: &str, args: &Value) -> Vec<FileDiff> {
    match normalize_tool_name(tool_name) {
        ToolKind::EditOrStrReplace => synth_edit(args),
        ToolKind::Write => synth_write(args),
        ToolKind::MultiEdit => synth_multi_edit(args),
        ToolKind::Unknown => Vec::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    EditOrStrReplace,
    Write,
    MultiEdit,
    Unknown,
}

fn normalize_tool_name(name: &str) -> ToolKind {
    // Strip any "mcp_" / "mcp__server__" prefix so mcp__fs__write_file maps
    // the same as write_file. We only look at the trailing segment.
    let tail = name.rsplit("__").next().unwrap_or(name);
    let tail = tail.strip_prefix("mcp_").unwrap_or(tail);

    match tail {
        "edit_file" | "edit" | "Edit" | "str_replace" | "str_replace_editor" => {
            ToolKind::EditOrStrReplace
        }
        "write_file" | "write" | "Write" | "WriteFile" | "write_text_file" | "create_file"
        | "update_note" | "create_note" => ToolKind::Write,
        "multi_edit" | "MultiEdit" => ToolKind::MultiEdit,
        _ => ToolKind::Unknown,
    }
}

fn extract_path(obj: &serde_json::Map<String, Value>) -> Option<&str> {
    obj.get("path")
        .or_else(|| obj.get("file_path"))
        .or_else(|| obj.get("file"))
        .and_then(Value::as_str)
}

fn within_size_cap(old: Option<&str>, new: &str) -> bool {
    new.len() <= MAX_DIFF_BYTES && old.is_none_or(|s| s.len() <= MAX_DIFF_BYTES)
}

/// A replacement shows both of its own sides — that is what the caller asked
/// for, and both are already in the arguments.
fn synth_edit(args: &Value) -> Vec<FileDiff> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let (Some(path), Some(old_string), Some(new_string)) = (
        extract_path(obj),
        obj.get("old_string").and_then(Value::as_str),
        obj.get("new_string").and_then(Value::as_str),
    ) else {
        return Vec::new();
    };

    if !within_size_cap(Some(old_string), new_string) {
        return Vec::new();
    }
    vec![FileDiff::from_contents(
        path,
        Some(old_string.to_string()),
        new_string,
    )]
}

/// A whole-file write replaces whatever is there, so the old side is
/// genuinely unknown without reading — and `None` is the shape the renderer
/// already uses for "no previous content", which is honest here rather than a
/// degraded case.
fn synth_write(args: &Value) -> Vec<FileDiff> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let Some(path) = extract_path(obj) else {
        return Vec::new();
    };
    let Some(new_content) = obj
        .get("content")
        .or_else(|| obj.get("new_content"))
        .or_else(|| obj.get("text"))
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };

    if !within_size_cap(None, new_content) {
        return Vec::new();
    }
    vec![FileDiff::from_contents(path, None, new_content)]
}

/// Every replacement in one diff, joined so the renderer emits a hunk per
/// edit. Sequential application is deliberately NOT simulated: doing that
/// needed the file, and an approver is deciding on the replacements the model
/// asked for, not on the daemon's guess at how they compose.
fn synth_multi_edit(args: &Value) -> Vec<FileDiff> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let (Some(path), Some(edits)) = (
        extract_path(obj),
        obj.get("edits").and_then(Value::as_array),
    ) else {
        return Vec::new();
    };

    let (mut olds, mut news) = (Vec::new(), Vec::new());
    for edit in edits {
        let Some(edit_obj) = edit.as_object() else {
            continue;
        };
        let (Some(old_string), Some(new_string)) = (
            edit_obj.get("old_string").and_then(Value::as_str),
            edit_obj.get("new_string").and_then(Value::as_str),
        ) else {
            continue;
        };
        olds.push(old_string);
        news.push(new_string);
    }
    if olds.is_empty() {
        return Vec::new();
    }

    let (old, new) = (olds.join("\n\n"), news.join("\n\n"));
    if !within_size_cap(Some(&old), &new) {
        return Vec::new();
    }
    vec![FileDiff::from_contents(path, Some(old), new)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The bug this module was rewritten to make impossible: `old_string: ""`
    /// matched any file, so the diff carried its whole body — published to the
    /// permission popup and the session event stream before anyone approved
    /// anything, and a Deny could not un-publish it.
    ///
    /// There is now nothing to contain, because nothing is read. The test
    /// names a real file with real content and asserts the content cannot
    /// appear, whatever the path says.
    #[test]
    fn a_write_tool_never_reads_the_file_it_names() {
        let tmp = tempfile::TempDir::new().unwrap();
        let secret = tmp.path().join("id_rsa");
        std::fs::write(&secret, "SENTINEL-PRIVATE-KEY-BODY").unwrap();
        // The fixture must be readable, or this passes for the wrong reason.
        assert_eq!(
            std::fs::read_to_string(&secret).unwrap(),
            "SENTINEL-PRIVATE-KEY-BODY"
        );

        let attacks = [
            json!({"path": secret.to_str().unwrap(), "old_string": "", "new_string": "x"}),
            json!({"path": secret.to_str().unwrap(), "content": "x"}),
            json!({"path": secret.to_str().unwrap(),
                   "edits": [{"old_string": "", "new_string": "x"}]}),
        ];
        for (tool, args) in ["edit_file", "write_file", "multi_edit"]
            .iter()
            .zip(attacks)
        {
            let rendered = serde_json::to_string(&synthesize_diffs(tool, &args)).unwrap();
            assert!(
                !rendered.contains("SENTINEL-PRIVATE-KEY-BODY"),
                "{tool} leaked the file body: {rendered}"
            );
        }
    }

    #[test]
    fn an_edit_shows_both_sides_of_the_replacement() {
        let diffs = synthesize_diffs(
            "edit_file",
            &json!({"path": "src/main.rs", "old_string": "alpha", "new_string": "beta"}),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "src/main.rs");
        assert_eq!(diffs[0].old_content.as_deref(), Some("alpha"));
        assert_eq!(diffs[0].new_content, "beta");
    }

    /// A path the session could never reach previews exactly like any other:
    /// there is no scope to be inside or outside of.
    #[test]
    fn an_absolute_path_outside_any_scope_previews_from_its_arguments() {
        let diffs = synthesize_diffs(
            "edit_file",
            &json!({"path": "/etc/shadow", "old_string": "a", "new_string": "b"}),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].old_content.as_deref(), Some("a"));
    }

    #[test]
    fn a_whole_file_write_has_no_old_side() {
        let diffs = synthesize_diffs("write_file", &json!({"path": "n.md", "content": "hello"}));
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].old_content, None);
        assert_eq!(diffs[0].new_content, "hello");
    }

    #[test]
    fn a_multi_edit_renders_every_replacement() {
        let diffs = synthesize_diffs(
            "multi_edit",
            &json!({"path": "a.rs", "edits": [
                {"old_string": "one", "new_string": "1"},
                {"old_string": "two", "new_string": "2"}
            ]}),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].old_content.as_deref(), Some("one\n\ntwo"));
        assert_eq!(diffs[0].new_content, "1\n\n2");
    }

    #[test]
    fn mcp_prefixed_names_map_to_the_same_kinds() {
        assert_eq!(normalize_tool_name("mcp__fs__write_file"), ToolKind::Write);
        assert_eq!(
            normalize_tool_name("mcp_edit_file"),
            ToolKind::EditOrStrReplace
        );
        assert_eq!(normalize_tool_name("bash"), ToolKind::Unknown);
    }

    #[test]
    fn an_unknown_tool_or_malformed_args_yields_nothing() {
        assert!(synthesize_diffs("bash", &json!({"command": "ls"})).is_empty());
        assert!(synthesize_diffs("edit_file", &json!("not an object")).is_empty());
        assert!(synthesize_diffs("edit_file", &json!({"path": "a"})).is_empty());
        assert!(synthesize_diffs("write_file", &json!({"content": "no path"})).is_empty());
        assert!(synthesize_diffs("multi_edit", &json!({"path": "a", "edits": []})).is_empty());
    }

    #[test]
    fn an_oversized_side_is_skipped() {
        let big = "x".repeat(MAX_DIFF_BYTES + 1);
        assert!(synthesize_diffs("write_file", &json!({"path": "a", "content": big})).is_empty());
        assert!(synthesize_diffs(
            "edit_file",
            &json!({"path": "a", "old_string": big, "new_string": "b"})
        )
        .is_empty());
    }
}
