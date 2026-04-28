//! Synthesize `FileDiff` values from a tool name + raw arguments JSON.
//!
//! Used by the permission flow so that file-mutating tools attach their
//! pending-change diffs to the `PermRequest` shown in the TUI popup.
//!
//! The synthesizer is intentionally **pure** with respect to outcome:
//! - missing or malformed args produce an empty `Vec`, never a panic
//! - unknown tool names produce an empty `Vec`
//! - if the disk read fails, `old_content` is `None` (treated as "create")
//! - if `old_string` is not present in the on-disk content, no diff is
//!   synthesized for that edit (graceful degradation — the popup will
//!   render the header without a diff body)
//!
//! Size cap: any diff where either side exceeds 1 MiB is skipped — the
//! renderer would suppress it anyway, and we'd rather not push it onto
//! the wire.

use std::path::{Path, PathBuf};

use crucible_core::types::acp::FileDiff;
use serde_json::Value;

/// Per-side size limit (1 MiB) for diff content. Beyond this, the diff
/// is dropped entirely.
const MAX_DIFF_BYTES: usize = 1024 * 1024;

/// Synthesize `FileDiff`s for a permission request, given the tool name
/// and the raw JSON arguments the agent passed.
///
/// `workspace_root` is the cwd a relative `path` should resolve against.
///
/// Returns an empty `Vec` for non-file-mutating tools, malformed args,
/// or oversized content. Never panics.
pub fn synthesize_diffs(tool_name: &str, args: &Value, workspace_root: &Path) -> Vec<FileDiff> {
    let normalized = normalize_tool_name(tool_name);
    match normalized {
        ToolKind::EditOrStrReplace => synth_edit(args, workspace_root),
        ToolKind::Write => synth_write(args, workspace_root),
        ToolKind::MultiEdit => synth_multi_edit(args, workspace_root),
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

/// Map both Crucible-internal names (`edit_file`, `write_file`),
/// Anthropic-style names (`Edit`, `Write`, `MultiEdit`, `str_replace`),
/// and MCP-prefixed variants (`mcp_edit`, `mcp_write`) to a single kind.
fn normalize_tool_name(name: &str) -> ToolKind {
    // Strip any "mcp_" / "mcp__server__" prefix so mcp__fs__write_file maps
    // the same as write_file. We only look at the trailing segment.
    let tail = name.rsplit("__").next().unwrap_or(name);
    let tail = tail.strip_prefix("mcp_").unwrap_or(tail);

    match tail {
        "edit_file" | "edit" | "Edit" | "str_replace" | "str_replace_editor" => {
            ToolKind::EditOrStrReplace
        }
        "write_file" | "write" | "Write" | "WriteFile" | "write_text_file" | "create_file" => {
            ToolKind::Write
        }
        "multi_edit" | "MultiEdit" => ToolKind::MultiEdit,
        _ => ToolKind::Unknown,
    }
}

fn extract_path<'a>(obj: &'a serde_json::Map<String, Value>) -> Option<&'a str> {
    obj.get("path")
        .or_else(|| obj.get("file_path"))
        .or_else(|| obj.get("file"))
        .and_then(Value::as_str)
}

fn resolve_path(path: &str, workspace_root: &Path) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace_root.join(p)
    }
}

fn read_old_content(resolved: &Path) -> Option<String> {
    std::fs::read_to_string(resolved).ok()
}

fn within_size_cap(old: Option<&str>, new: &str) -> bool {
    new.len() <= MAX_DIFF_BYTES && old.map_or(true, |s| s.len() <= MAX_DIFF_BYTES)
}

fn synth_edit(args: &Value, workspace_root: &Path) -> Vec<FileDiff> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let Some(path) = extract_path(obj) else {
        return Vec::new();
    };
    let Some(old_string) = obj.get("old_string").and_then(Value::as_str) else {
        return Vec::new();
    };
    let Some(new_string) = obj.get("new_string").and_then(Value::as_str) else {
        return Vec::new();
    };

    let resolved = resolve_path(path, workspace_root);
    // For an edit, the file must already exist; if it doesn't or the
    // disk read fails, we can't synthesize the diff sensibly.
    let Some(old_content) = read_old_content(&resolved) else {
        return Vec::new();
    };

    if !old_content.contains(old_string) {
        // old_string not in file — graceful skip.
        return Vec::new();
    }

    let replace_all = obj
        .get("replace_all")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let new_content = if replace_all {
        old_content.replace(old_string, new_string)
    } else {
        old_content.replacen(old_string, new_string, 1)
    };

    if !within_size_cap(Some(&old_content), &new_content) {
        return Vec::new();
    }

    vec![FileDiff::from_contents(
        path,
        Some(old_content),
        new_content,
    )]
}

fn synth_write(args: &Value, workspace_root: &Path) -> Vec<FileDiff> {
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

    let resolved = resolve_path(path, workspace_root);
    let old_content = read_old_content(&resolved);

    if !within_size_cap(old_content.as_deref(), new_content) {
        return Vec::new();
    }

    vec![FileDiff::from_contents(path, old_content, new_content)]
}

fn synth_multi_edit(args: &Value, workspace_root: &Path) -> Vec<FileDiff> {
    let Some(obj) = args.as_object() else {
        return Vec::new();
    };
    let Some(path) = extract_path(obj) else {
        return Vec::new();
    };
    let Some(edits) = obj.get("edits").and_then(Value::as_array) else {
        return Vec::new();
    };

    let resolved = resolve_path(path, workspace_root);
    let Some(original) = read_old_content(&resolved) else {
        return Vec::new();
    };

    let mut current = original.clone();
    for edit in edits {
        let Some(edit_obj) = edit.as_object() else {
            continue;
        };
        let Some(old_string) = edit_obj.get("old_string").and_then(Value::as_str) else {
            continue;
        };
        let Some(new_string) = edit_obj.get("new_string").and_then(Value::as_str) else {
            continue;
        };
        if !current.contains(old_string) {
            // Sequential apply — if any edit doesn't match, skip it.
            // We don't error: the agent may still get useful permission
            // info from the partial result.
            continue;
        }
        let replace_all = edit_obj
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        current = if replace_all {
            current.replace(old_string, new_string)
        } else {
            current.replacen(old_string, new_string, 1)
        };
    }

    if current == original {
        return Vec::new();
    }
    if !within_size_cap(Some(&original), &current) {
        return Vec::new();
    }

    vec![FileDiff::from_contents(path, Some(original), current)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn unknown_tool_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let diffs = synthesize_diffs("bash", &json!({"command": "ls"}), tmp.path());
        assert!(diffs.is_empty());
    }

    #[test]
    fn edit_synthesizes_old_and_new_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("a.txt");
        std::fs::write(&path, "hello world").unwrap();

        let args = json!({
            "path": path.to_str().unwrap(),
            "old_string": "world",
            "new_string": "Crucible",
        });
        let diffs = synthesize_diffs("edit_file", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].old_content.as_deref(), Some("hello world"));
        assert_eq!(diffs[0].new_content, "hello Crucible");
    }

    #[test]
    fn edit_with_old_string_not_found_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("a.txt");
        std::fs::write(&path, "hello world").unwrap();

        let args = json!({
            "path": path.to_str().unwrap(),
            "old_string": "absent",
            "new_string": "x",
        });
        assert!(synthesize_diffs("edit_file", &args, tmp.path()).is_empty());
    }

    #[test]
    fn edit_replace_all_replaces_every_occurrence() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("a.txt");
        std::fs::write(&path, "ab ab ab").unwrap();

        let args = json!({
            "path": path.to_str().unwrap(),
            "old_string": "ab",
            "new_string": "X",
            "replace_all": true,
        });
        let diffs = synthesize_diffs("edit_file", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].new_content, "X X X");
    }

    #[test]
    fn write_create_uses_content_arg_with_no_old() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new.txt");
        // file does NOT exist — this is a creation
        let args = json!({
            "path": path.to_str().unwrap(),
            "content": "fresh contents",
        });
        let diffs = synthesize_diffs("write_file", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].old_content.is_none());
        assert_eq!(diffs[0].new_content, "fresh contents");
    }

    #[test]
    fn write_overwrite_includes_old_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("existing.txt");
        std::fs::write(&path, "before").unwrap();
        let args = json!({
            "path": path.to_str().unwrap(),
            "content": "after",
        });
        let diffs = synthesize_diffs("write_file", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].old_content.as_deref(), Some("before"));
        assert_eq!(diffs[0].new_content, "after");
    }

    #[test]
    fn relative_path_resolves_against_workspace_root() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("rel.txt"), "old").unwrap();
        let args = json!({
            "path": "rel.txt",
            "old_string": "old",
            "new_string": "new",
        });
        let diffs = synthesize_diffs("edit_file", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].new_content, "new");
        // The path is preserved as the agent provided it, not resolved.
        assert_eq!(diffs[0].path, "rel.txt");
    }

    #[test]
    fn malformed_args_return_empty() {
        let tmp = TempDir::new().unwrap();
        // missing required fields
        assert!(synthesize_diffs("edit_file", &json!({}), tmp.path()).is_empty());
        assert!(synthesize_diffs("write_file", &json!({"path": 7}), tmp.path()).is_empty());
        assert!(synthesize_diffs("edit_file", &Value::Null, tmp.path()).is_empty());
    }

    #[test]
    fn anthropic_style_tool_names_recognized() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("file.txt");
        std::fs::write(&path, "old").unwrap();

        let edit_args = json!({
            "file_path": path.to_str().unwrap(),
            "old_string": "old",
            "new_string": "new",
        });
        assert_eq!(synthesize_diffs("Edit", &edit_args, tmp.path()).len(), 1);
        assert_eq!(synthesize_diffs("edit", &edit_args, tmp.path()).len(), 1);
        assert_eq!(
            synthesize_diffs("str_replace", &edit_args, tmp.path()).len(),
            1
        );

        let write_args = json!({
            "file_path": tmp.path().join("new.txt").to_str().unwrap(),
            "content": "x",
        });
        assert_eq!(synthesize_diffs("Write", &write_args, tmp.path()).len(), 1);
    }

    #[test]
    fn mcp_prefixed_names_recognized() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("file.txt");
        std::fs::write(&path, "old").unwrap();
        let args = json!({
            "path": path.to_str().unwrap(),
            "old_string": "old",
            "new_string": "new",
        });
        assert_eq!(
            synthesize_diffs("mcp_edit_file", &args, tmp.path()).len(),
            1,
            "bare mcp_ prefix"
        );
        assert_eq!(
            synthesize_diffs("mcp__fs__edit_file", &args, tmp.path()).len(),
            1,
            "mcp__server__name prefix"
        );
    }

    #[test]
    fn multi_edit_applies_edits_sequentially() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("multi.txt");
        std::fs::write(&path, "alpha beta").unwrap();
        let args = json!({
            "path": path.to_str().unwrap(),
            "edits": [
                { "old_string": "alpha", "new_string": "A" },
                { "old_string": "beta",  "new_string": "B" },
            ],
        });
        let diffs = synthesize_diffs("multi_edit", &args, tmp.path());
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].new_content, "A B");
    }

    #[test]
    fn oversized_content_is_dropped() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("big.txt");
        let big = "x".repeat(MAX_DIFF_BYTES + 1);
        let args = json!({
            "path": path.to_str().unwrap(),
            "content": big,
        });
        assert!(synthesize_diffs("write_file", &args, tmp.path()).is_empty());
    }
}
