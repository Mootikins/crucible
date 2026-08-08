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
//!
//! # Containment
//!
//! This module opens files named in the model's **raw, unapproved tool
//! arguments**, and it does so *before* the permission gate resolves — its
//! output is attached to the `PermRequest` and broadcast on the session
//! event stream. A read performed here is therefore already published by
//! the time a human clicks Deny; denying cannot un-read it. `synth_edit`
//! with `old_string: ""` matches any file and returns its entire body as
//! `old_content`, so an unbounded read here is a full-file exfiltration
//! primitive reachable from any ordinary agent turn.
//!
//! The preview is consequently confined to `workspace_root` via the shared
//! [`crucible_core::path_containment::resolve_within`] chokepoint. A target
//! that cannot be proven inside it yields **no diff at all**, for reads and
//! creates alike; see [`readable_target`] for why refusal rather than a
//! redacted placeholder.
//!
//! ## `workspace_root` is NARROWER than the executing tool's own scope
//!
//! Stated here rather than discovered later. The tools this module recognises
//! are not workspace-scoped: `agent_manager/mod.rs` contains `WorkspaceTools`
//! to `[session.workspace, session.kiln, ..session.connected_kilns,
//! session.storage_path()]` (`with_allowed_roots`), and `create_note` /
//! `update_note` address the kiln exclusively
//! (`tools/notes/mod.rs`, `validate_path_within_kiln`). So an `edit_file`
//! naming an absolute path under the kiln, a connected kiln or the session
//! dir will *execute* while previewing as nothing — and since a session
//! started without an explicit workspace gets a private scratch directory
//! (`session_manager.rs`, `create_session`), the kiln is routinely outside
//! `workspace_root`.
//!
//! Losing a preview is a usability regression, never a disclosure one, so the
//! narrow root stays until the wider set can be supplied: closing it means
//! passing the executor's real root list to
//! [`crucible_core::path_containment::resolve_within_any`], which requires
//! the kiln, connected kilns and session dir to reach all four call sites —
//! none of `StreamContext`, `GenAiHandle` or the ACP `ClientConfig` carries
//! them today.

use std::path::{Path, PathBuf};

use crucible_core::types::acp::{FileDiff, MAX_DIFF_BYTES};
use serde_json::Value;

/// Synthesize `FileDiff`s for a permission request, given the tool name
/// and the raw JSON arguments the agent passed.
///
/// `workspace_root` is the session scope: relative `path`s resolve against
/// it, and it also **bounds** what the preview may read — a target not
/// provably inside it is refused outright (see [`readable_target`]). An
/// empty or non-existent root therefore denies everything.
///
/// Returns an empty `Vec` for non-file-mutating tools, malformed args,
/// oversized content, or an out-of-scope target. Never panics.
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

/// The contained path this preview may read, or `None` (with a debug log)
/// when it may not touch the target at all.
///
/// Containment itself is **not implemented here** — it delegates to the
/// shared [`crucible_core::path_containment::resolve_within`], which is the
/// one chokepoint the daemon, web and Lua layers all answer to. Writing a
/// local variant is how the previous rounds of this work shipped divergent
/// semantics: nine near-identical validators, each with a different hole.
/// The one thing this wrapper owns is the *policy* (refuse ⇒ no diff) and
/// the diagnostic; the walk, the symlink handling and the errno rules
/// belong to the shared function.
///
/// `workspace_root` is the one root every caller already passes — which is
/// why the fix lands at the sink and covers all four call sites without any
/// of them changing (see the module docs for why that root is narrower than
/// the executing tool's own scope, and what closing the gap would cost):
/// `agent_manager/messaging/permission.rs:219` (ACP gate),
/// `agent_manager/messaging/permission.rs:935` (internal tool loop),
/// `provider/genai_handle.rs:183` (tool-call emission, which has no
/// permission gate at all), and `acp/client/tools.rs:202` (tool-info
/// fallback, whose `Path::new("")` default now denies rather than resolving
/// against the daemon's process CWD).
///
/// # Why refuse rather than emit a redacted placeholder
///
/// The permission popup already renders its header without a diff body (the
/// module docs' "graceful degradation"), so refusal reuses a display path
/// that exists and needs no wire change; a placeholder would mean a new
/// `FileDiff` shape rippling into the TUI and web renderers. More
/// importantly, a partial preview *misinforms* the approver: emitting
/// `old_content: None` for an out-of-scope overwrite reads as "this creates
/// a new file" for an operation that destroys an existing one, and it turns
/// the popup into a filesystem existence oracle (diff appeared ⇒ target
/// absent; no diff ⇒ target present). The path string still reaches the
/// human — it came from the agent, so showing it discloses nothing the
/// agent did not already know.
fn readable_target(path: &str, workspace_root: &Path) -> Option<PathBuf> {
    match crucible_core::path_containment::resolve_within(workspace_root, Path::new(path)) {
        Ok(resolved) => Some(resolved),
        // Every `Refusal` variant lands here and means exactly one thing to
        // this sink: do not read, do not emit. They stay distinct in the log
        // because `RootUnavailable` (a misconfigured session) and
        // `SymlinkEscape` (an agent probing outside its scope) want very
        // different operator responses — but neither may soften the verdict.
        Err(refusal) => {
            tracing::debug!(
                path = %path,
                workspace_root = %workspace_root.display(),
                refusal = %refusal,
                "diff preview refused: target is not provably inside the session workspace"
            );
            None
        }
    }
}

fn read_old_content(resolved: &Path) -> Option<String> {
    std::fs::read_to_string(resolved).ok()
}

fn within_size_cap(old: Option<&str>, new: &str) -> bool {
    new.len() <= MAX_DIFF_BYTES && old.is_none_or(|s| s.len() <= MAX_DIFF_BYTES)
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

    let Some(resolved) = readable_target(path, workspace_root) else {
        return Vec::new();
    };
    // For an edit, the file must already exist; if it doesn't or the
    // disk read fails, we can't synthesize the diff sensibly.
    let Some(old_content) = read_old_content(&resolved) else {
        return Vec::new();
    };

    if !old_content.contains(old_string) {
        tracing::debug!(
            path = %resolved.display(),
            old_string_len = old_string.len(),
            "synth_edit: old_string not found on disk; skipping diff emission"
        );
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

    // Refused even though a create needs no read: emitting a create-shaped
    // diff for an out-of-scope target would misdescribe an overwrite and
    // would leak the target's existence. See `resolve_readable`.
    let Some(resolved) = readable_target(path, workspace_root) else {
        return Vec::new();
    };
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

    let Some(resolved) = readable_target(path, workspace_root) else {
        return Vec::new();
    };
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

    /// Native synthesis reads on-disk content and applies the edit in
    /// memory; an ACP agent would ship the pre/post strings directly. The
    /// real assertion this test makes is that native synthesis honors the
    /// disk state (so `old_content` matches what was on disk) and applies
    /// the agent-supplied `old_string`→`new_string` substitution to
    /// produce `new_content`. If those derivations drift, the user sees
    /// inconsistent diffs depending on which agent path produced them.
    #[test]
    fn synthesized_edit_derives_from_disk_and_agent_args() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("foo.rs");
        let on_disk = "fn main() {\n    println!(\"hi\");\n}\n";
        std::fs::write(&path, on_disk).unwrap();

        let native = synthesize_diffs(
            "edit_file",
            &json!({
                "path": path.to_str().unwrap(),
                "old_string": "println!(\"hi\");",
                "new_string": "println!(\"hello\");",
            }),
            tmp.path(),
        );
        assert_eq!(native.len(), 1);

        // old_content comes from disk verbatim (not synthesized).
        assert_eq!(native[0].old_content.as_deref(), Some(on_disk));
        // new_content reflects the substitution applied to the disk state.
        assert_eq!(
            native[0].new_content,
            "fn main() {\n    println!(\"hello\");\n}\n"
        );
    }

    // ── containment ────────────────────────────────────────────────────
    //
    // The synthesizer runs BEFORE the permission gate resolves and its
    // output is broadcast on the session event stream, so a read it
    // performs is already published by the time a human clicks Deny.
    // Every test below is about what it is allowed to read, never about
    // what the gate later decides.

    /// A file outside the workspace, plus proof this process can read it.
    ///
    /// The proof matters: if the fixture were unreadable (restrictive
    /// umask, odd tmpdir mode) an empty diff would prove nothing, because
    /// `read_old_content` would have returned `None` on its own. Asserting
    /// the read succeeds pins that containment — not file permissions — is
    /// the only thing that can produce an empty result.
    fn readable_secret(outside: &TempDir, body: &str) -> PathBuf {
        let secret = outside.path().join("id_rsa");
        std::fs::write(&secret, body).unwrap();
        assert_eq!(
            std::fs::read_to_string(&secret).unwrap(),
            body,
            "fixture must be readable by this process, or the assertions below \
             would pass for the wrong reason"
        );
        secret
    }

    /// The exfiltration primitive itself: `old_string: ""` matches every
    /// file, so `synth_edit` returns the WHOLE file body as `old_content`.
    /// This control leg runs the attack against an in-workspace file and
    /// asserts it succeeds — without it, an empty result on the
    /// out-of-workspace leg could mean "the empty `old_string` trick does
    /// not work" rather than "containment refused the read".
    #[test]
    fn empty_old_string_returns_the_whole_file_for_an_in_scope_path() {
        let ws = TempDir::new().unwrap();
        let target = ws.path().join("notes.md");
        std::fs::write(&target, "SENTINEL-IN-SCOPE-BODY").unwrap();

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": target.to_str().unwrap(),
                "old_string": "",
                "new_string": "x",
            }),
            ws.path(),
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs[0].old_content.as_deref(),
            Some("SENTINEL-IN-SCOPE-BODY"),
            "the empty-old_string full-file read must work in scope, or the \
             out-of-scope leg proves nothing"
        );
    }

    #[test]
    fn edit_outside_the_workspace_never_reads_the_file() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let secret = readable_secret(&outside, "SENTINEL-PRIVATE-KEY-BODY");

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": secret.to_str().unwrap(),
                "old_string": "",
                "new_string": "x",
            }),
            ws.path(),
        );

        assert!(
            diffs.is_empty(),
            "an out-of-workspace edit produced a diff: {diffs:?}"
        );
        // The event stream carries these serialized; assert on the wire form
        // so a leak through any field (not just `old_content`) is caught.
        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-PRIVATE-KEY-BODY"),
            "file body reached the emitted diff: {wire}"
        );
    }

    #[test]
    fn write_outside_the_workspace_never_reads_the_file() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let secret = readable_secret(&outside, "SENTINEL-OVERWRITTEN-BODY");

        let diffs = synthesize_diffs(
            "write_file",
            &json!({
                "path": secret.to_str().unwrap(),
                "content": "pwned",
            }),
            ws.path(),
        );

        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-OVERWRITTEN-BODY"),
            "file body reached the emitted diff: {wire}"
        );
        assert!(
            diffs.is_empty(),
            "an out-of-workspace write produced a diff: {diffs:?}"
        );
    }

    #[test]
    fn multi_edit_outside_the_workspace_never_reads_the_file() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let secret = readable_secret(&outside, "alpha SENTINEL-MULTI-BODY");

        let diffs = synthesize_diffs(
            "multi_edit",
            &json!({
                "path": secret.to_str().unwrap(),
                "edits": [{ "old_string": "alpha", "new_string": "A" }],
            }),
            ws.path(),
        );

        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-MULTI-BODY"),
            "file body reached the emitted diff: {wire}"
        );
        assert!(diffs.is_empty(), "out-of-workspace multi_edit: {diffs:?}");
    }

    #[test]
    fn relative_parent_traversal_never_reads_the_file() {
        let ws_parent = TempDir::new().unwrap();
        let ws = ws_parent.path().join("workspace");
        std::fs::create_dir(&ws).unwrap();
        std::fs::write(ws_parent.path().join("sibling.txt"), "SENTINEL-SIBLING").unwrap();

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": "../sibling.txt",
                "old_string": "",
                "new_string": "x",
            }),
            &ws,
        );
        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(!wire.contains("SENTINEL-SIBLING"), "`..` escaped: {wire}");
        assert!(diffs.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaf_escaping_the_workspace_never_reads_the_file() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let secret = readable_secret(&outside, "SENTINEL-VIA-SYMLINK");
        std::os::unix::fs::symlink(&secret, ws.path().join("innocent.md")).unwrap();

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": "innocent.md",
                "old_string": "",
                "new_string": "x",
            }),
            ws.path(),
        );
        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-VIA-SYMLINK"),
            "a symlink leaf inside the workspace read an outside file: {wire}"
        );
        assert!(diffs.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directory_escaping_the_workspace_never_reads_the_file() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        readable_secret(&outside, "SENTINEL-VIA-DIR-SYMLINK");
        std::os::unix::fs::symlink(outside.path(), ws.path().join("sub")).unwrap();

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": "sub/id_rsa",
                "old_string": "",
                "new_string": "x",
            }),
            ws.path(),
        );
        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-VIA-DIR-SYMLINK"),
            "a symlinked directory component escaped the workspace: {wire}"
        );
        assert!(diffs.is_empty());
    }

    /// An intra-workspace symlink is legitimate and must keep previewing —
    /// the fix must refuse escapes, not all links.
    #[cfg(unix)]
    #[test]
    fn symlink_staying_inside_the_workspace_still_previews() {
        let ws = TempDir::new().unwrap();
        std::fs::write(ws.path().join("real.md"), "inside body").unwrap();
        std::os::unix::fs::symlink(ws.path().join("real.md"), ws.path().join("link.md")).unwrap();

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": "link.md",
                "old_string": "inside",
                "new_string": "still inside",
            }),
            ws.path(),
        );
        assert_eq!(diffs.len(), 1, "an in-workspace symlink lost its preview");
        assert_eq!(diffs[0].new_content, "still inside body");
    }

    /// Out-of-scope creates are refused too. Emitting a create-shaped diff
    /// for them would be an existence oracle: "diff appeared" would mean
    /// the file does not exist, "no diff" would mean it does.
    #[test]
    fn write_create_outside_the_workspace_is_refused() {
        let ws = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let diffs = synthesize_diffs(
            "write_file",
            &json!({
                "path": outside.path().join("brand-new.txt").to_str().unwrap(),
                "content": "x",
            }),
            ws.path(),
        );
        assert!(diffs.is_empty(), "out-of-workspace create previewed");
    }

    /// `acp/client/tools.rs` falls back to an empty workspace root when the
    /// ACP session has no `working_dir`. An empty root cannot contain
    /// anything, so it must deny everything rather than resolve against the
    /// daemon's CWD.
    #[test]
    fn an_empty_workspace_root_denies_everything() {
        let outside = TempDir::new().unwrap();
        let secret = readable_secret(&outside, "SENTINEL-NO-WORKSPACE");

        let diffs = synthesize_diffs(
            "edit_file",
            &json!({
                "path": secret.to_str().unwrap(),
                "old_string": "",
                "new_string": "x",
            }),
            Path::new(""),
        );
        let wire = serde_json::to_string(&diffs).unwrap();
        assert!(
            !wire.contains("SENTINEL-NO-WORKSPACE"),
            "an empty workspace root read an absolute path: {wire}"
        );
        assert!(diffs.is_empty());
    }

    /// For create-style writes (file doesn't exist), the diff has
    /// `old_content == None` and `new_content` is exactly the agent's
    /// `content` argument — matching the shape an ACP agent emits.
    #[test]
    fn synthesized_write_create_has_no_old_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new.md");
        let new_content = "# New File\n\nFresh content.\n";

        let native = synthesize_diffs(
            "write_file",
            &json!({
                "path": path.to_str().unwrap(),
                "content": new_content,
            }),
            tmp.path(),
        );
        assert_eq!(native.len(), 1);
        assert!(native[0].old_content.is_none());
        assert_eq!(native[0].new_content, new_content);
    }
}
