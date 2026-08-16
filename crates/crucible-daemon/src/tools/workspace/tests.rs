//! Tests for [`super`]: the workspace tool set.
//!
//! A sibling file rather than an inline `mod tests`, for the file-size gate —
//! same shape as `agent_factory/tests.rs`.

use super::*;

/// Spilled output is reachable by name.
///
/// The session dir is already an allowed root, so the only thing standing
/// between the model and its own spilled output was that
/// `$CRU_SESSION_DIR/tools/x.txt` is not an absolute path — it was joined
/// under the workspace and then rejected as outside the roots. `bash` got
/// the expansion for free from the shell; every other tool did not.
#[test]
fn a_session_dir_env_var_resolves_to_the_session_dir() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let session_dir = tmp.path().join("session");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(session_dir.join("tools")).unwrap();
    std::fs::write(session_dir.join("tools").join("bash-1.txt"), "spilled").unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_env("CRU_SESSION_DIR", session_dir.to_string_lossy().to_string())
        .with_allowed_roots(vec![session_dir.clone()]);

    let resolved = tools
        .resolve_path("$CRU_SESSION_DIR/tools/bash-1.txt")
        .expect("the session dir is an allowed root");
    assert_eq!(
        std::fs::read_to_string(resolved).unwrap(),
        "spilled",
        "the model must be able to read back what was spilled for it"
    );

    // Braced form too, since that is what a shell-literate model may write.
    assert!(tools
        .resolve_path("${CRU_SESSION_DIR}/tools/bash-1.txt")
        .is_ok());
}

/// Expansion must not become an escape hatch: an unset variable stays
/// literal and is then judged by containment like any other path.
#[test]
fn an_unknown_env_var_is_not_expanded_and_stays_contained() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    let tools = WorkspaceTools::new(&workspace).with_allowed_roots(vec![]);

    // `$HOME` is not in the tool's env map, so it stays literal and is
    // judged as a relative path under the workspace — never the real home.
    let resolved = tools.resolve_path("$HOME/.ssh/id_rsa");
    if let Ok(p) = &resolved {
        let home = dirs::home_dir().expect("a home dir on this box");
        assert!(
            !p.starts_with(&home),
            "process env must not be expanded: {} reached {}",
            "$HOME/.ssh/id_rsa",
            p.display()
        );
    }
    assert!(tools.resolve_path("/etc/passwd").is_err());
}

use tempfile::TempDir;

// =========================================================================
// Test fixtures
// =========================================================================

fn create_workspace() -> (TempDir, WorkspaceTools) {
    let temp = TempDir::new().unwrap();
    let tools = WorkspaceTools::new(temp.path());
    (temp, tools)
}

// =========================================================================
// read_file tests
// =========================================================================

#[tokio::test]
async fn test_read_file_returns_content_with_line_numbers() {
    let (temp, tools) = create_workspace();
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "line1\nline2\nline3")
        .await
        .unwrap();

    let result = tools.read_file("test.txt".to_string(), None, None).await;

    assert!(result.is_ok());
    let result = result.unwrap();
    assert!(!result
        .is_error
        .expect("is_error field should be present in tool result"));

    // Check content contains line numbers
    let content = format!("{:?}", result.content);
    assert!(content.contains("line1"));
    assert!(content.contains("line2"));
    assert!(content.contains("line3"));
}

#[tokio::test]
async fn test_read_file_with_offset_and_limit() {
    let (temp, tools) = create_workspace();
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "line1\nline2\nline3\nline4\nline5")
        .await
        .unwrap();

    // Read lines 2-3 only
    let result = tools
        .read_file("test.txt".to_string(), Some(2), Some(2))
        .await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("line2"));
    assert!(content.contains("line3"));
    assert!(!content.contains("line1")); // Should be skipped
    assert!(!content.contains("line4")); // Should be limited
}

#[tokio::test]
async fn test_read_file_nonexistent_returns_error() {
    let (_temp, tools) = create_workspace();

    let result = tools
        .read_file("nonexistent.txt".to_string(), None, None)
        .await;

    assert!(result.is_err());
}

// =========================================================================
// edit_file tests
// =========================================================================

#[tokio::test]
async fn test_edit_file_replaces_text() {
    let (temp, tools) = create_workspace();
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "hello world").await.unwrap();

    let result = tools
        .edit_file(
            "test.txt".to_string(),
            "world".to_string(),
            "rust".to_string(),
            None,
        )
        .await;

    assert!(result.is_ok());

    let content = tokio::fs::read_to_string(&file).await.unwrap();
    assert_eq!(content, "hello rust");
}

#[tokio::test]
async fn test_edit_file_replace_all() {
    let (temp, tools) = create_workspace();
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "foo bar foo baz foo")
        .await
        .unwrap();

    let result = tools
        .edit_file(
            "test.txt".to_string(),
            "foo".to_string(),
            "qux".to_string(),
            Some(true),
        )
        .await;

    assert!(result.is_ok());

    let content = tokio::fs::read_to_string(&file).await.unwrap();
    assert_eq!(content, "qux bar qux baz qux");
}

#[tokio::test]
async fn test_edit_file_not_found_returns_message() {
    let (temp, tools) = create_workspace();
    let file = temp.path().join("test.txt");
    tokio::fs::write(&file, "hello world").await.unwrap();

    let result = tools
        .edit_file(
            "test.txt".to_string(),
            "notfound".to_string(),
            "replacement".to_string(),
            None,
        )
        .await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("not found"));
}

// =========================================================================
// write_file tests
// =========================================================================

#[tokio::test]
async fn test_write_file_creates_file() {
    let (temp, tools) = create_workspace();

    let result = tools
        .write_file("new.txt".to_string(), "hello".to_string())
        .await;

    assert!(result.is_ok());

    let content = tokio::fs::read_to_string(temp.path().join("new.txt"))
        .await
        .unwrap();
    assert_eq!(content, "hello");
}

#[tokio::test]
async fn test_write_file_creates_parent_dirs() {
    let (temp, tools) = create_workspace();

    let result = tools
        .write_file("a/b/c/new.txt".to_string(), "nested".to_string())
        .await;

    assert!(result.is_ok());

    let content = tokio::fs::read_to_string(temp.path().join("a/b/c/new.txt"))
        .await
        .unwrap();
    assert_eq!(content, "nested");
}

// =========================================================================
// bash tests
// =========================================================================

#[tokio::test]
async fn test_bash_executes_command() {
    let (_temp, tools) = create_workspace();

    let result = tools.bash("echo hello".to_string(), None).await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("hello"));
}

#[tokio::test]
async fn test_bash_returns_exit_code_on_failure() {
    let (_temp, tools) = create_workspace();

    let result = tools.bash("exit 42".to_string(), None).await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("42"));
}

#[tokio::test]
async fn test_bash_timeout() {
    let (_temp, tools) = create_workspace();

    let result = tools.bash("sleep 10".to_string(), Some(100)).await;

    assert!(result.is_err());
}

// =========================================================================
// glob tests
// =========================================================================

#[tokio::test]
async fn test_glob_finds_files() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(temp.path().join("a.rs"), "")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("b.rs"), "")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("c.txt"), "")
        .await
        .unwrap();

    let result = tools.glob("*.rs".to_string(), None, None);

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("a.rs"));
    assert!(content.contains("b.rs"));
    assert!(!content.contains("c.txt"));
}

#[tokio::test]
async fn test_glob_respects_limit() {
    let (temp, tools) = create_workspace();
    for i in 0..10 {
        tokio::fs::write(temp.path().join(format!("{i}.rs")), "")
            .await
            .unwrap();
    }

    let result = tools.glob("*.rs".to_string(), None, Some(3));

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("3 files"));
    assert!(content.contains("truncated"));
}

// =========================================================================
// grep tests
// =========================================================================

#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn test_grep_finds_matches() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(temp.path().join("test.txt"), "hello\nworld\nhello again")
        .await
        .unwrap();

    let result = tools
        .grep(
            "hello".to_string(),
            Some("test.txt".to_string()),
            None,
            None,
        )
        .await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("hello"));
    assert!(content.contains("2 matches")); // Two lines with "hello"
}

#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn test_grep_with_glob_filter() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(temp.path().join("test.rs"), "fn main() {}")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("test.txt"), "fn in txt")
        .await
        .unwrap();

    let result = tools
        .grep("fn".to_string(), None, Some("*.rs".to_string()), None)
        .await;

    assert!(result.is_ok());
    let content = format!("{:?}", result.unwrap().content);
    assert!(content.contains("test.rs"));
    assert!(!content.contains("test.txt"));
}

// =========================================================================
// tool_definitions tests
// =========================================================================

#[test]
fn test_tool_definitions_returns_all_tools() {
    let defs = WorkspaceTools::tool_definitions();

    assert_eq!(defs.len(), 6);

    let names: Vec<&str> = defs.iter().map(|t| t.name.as_ref()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"edit_file"));
    assert!(names.contains(&"write_file"));
    assert!(names.contains(&"bash"));
    assert!(names.contains(&"glob"));
    assert!(names.contains(&"grep"));
}

#[test]
fn test_tool_definitions_have_descriptions() {
    let defs = WorkspaceTools::tool_definitions();

    for def in defs {
        assert!(
            def.description.is_some(),
            "Tool {} should have description",
            def.name
        );
    }
}

// =========================================================================
// Argument-injection containment
//
// `grep` and `glob` are in `is_safe` (see `agent_manager::is_safe`), so a
// model-chosen argument reaches them without ever passing the permission
// gate — no mode stance, no Lua hook, no prompt. Anything these two accept
// is accepted unconditionally, which is why their argument handling is
// load-bearing in a way the gated tools' is not.
// =========================================================================

/// A pattern is data, not argv.
///
/// `rg` parses any leading-dash argument as a flag, and `--pre=<cmd>` runs
/// `<cmd>` against every file walked — so a pattern that reached rg
/// unseparated was arbitrary command execution on the host.
#[tokio::test]
async fn grep_pattern_starting_with_dash_is_not_parsed_as_a_flag() {
    use std::os::unix::fs::PermissionsExt;

    let (temp, tools) = create_workspace();
    let marker = temp.path().join("executed.marker");
    let script = temp.path().join("pre.sh");

    tokio::fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\ncat \"$1\"\n", marker.display()),
    )
    .await
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    tokio::fs::write(temp.path().join("target.txt"), "hello")
        .await
        .unwrap();

    // Whether this errors or returns no matches is immaterial; what matters
    // is that the preprocessor never ran.
    let _ = tools
        .grep(format!("--pre={}", script.display()), None, None, None)
        .await;

    assert!(
        !marker.exists(),
        "a grep pattern beginning with '-' must reach rg as a pattern, not a flag"
    );
}

/// The separator must not be defeated by a later flag.
///
/// `--glob` is appended after the pattern, so a naive `--` placement would
/// turn the glob into a positional argument and silently change which files
/// are searched.
#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn grep_still_honors_a_glob_filter_alongside_a_literal_pattern() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(temp.path().join("a.rs"), "needle")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("b.txt"), "needle")
        .await
        .unwrap();

    let result = tools
        .grep("needle".to_string(), None, Some("*.rs".to_string()), None)
        .await
        .expect("grep with a glob filter should succeed");

    let content = format!("{:?}", result.content);
    assert!(content.contains("a.rs"), "the glob filter must still apply");
    assert!(
        !content.contains("b.txt"),
        "files excluded by the glob must not be searched"
    );
}

/// A leading-dash pattern must still be searchable as a pattern.
#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn grep_finds_a_literal_pattern_that_begins_with_a_dash() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(
        temp.path().join("sig.rs"),
        "fn f() -> Result<()> { Ok(()) }",
    )
    .await
    .unwrap();

    let result = tools
        .grep("-> Result".to_string(), None, None, None)
        .await
        .expect("a pattern beginning with '-' is legitimate and must still match");

    assert!(
        format!("{:?}", result.content).contains("sig.rs"),
        "the pattern must be searched, not swallowed by the separator"
    );
}

/// The `..` guard used to be gated on `allowed_roots.is_some()`, so on the
/// unconstrained instance — the one the daemon builds for plugin tool calls,
/// and the one an untrusted message reaches — `../../etc/*` walked straight
/// out. `create_workspace()` builds exactly that instance, which is why this
/// test is meaningful here.
#[tokio::test]
async fn glob_parent_traversal_is_rejected_even_with_no_allowed_roots() {
    let (temp, tools) = create_workspace();
    assert!(
        tools.allowed_roots.is_none(),
        "this test is only meaningful on an unconstrained instance"
    );
    tokio::fs::write(temp.path().join("inside.rs"), "")
        .await
        .unwrap();

    let result = tools.glob("../../../../../../etc/pas*".to_string(), None, None);

    match result {
        Err(_) => {}
        Ok(r) => panic!(
            "an upward-traversing glob pattern must be rejected, got: {:?}",
            r.content
        ),
    }
}

/// `PathBuf::join` with an absolute argument discards the base, so an
/// absolute glob pattern silently escaped the search path. The `..` guard
/// does not catch this, and the allowed-roots filter is `None` on the
/// instance the daemon builds for plugin tool calls.
#[tokio::test]
async fn glob_absolute_pattern_does_not_escape_the_search_path() {
    let (temp, tools) = create_workspace();
    tokio::fs::write(temp.path().join("inside.rs"), "")
        .await
        .unwrap();

    let result = tools.glob("/etc/*".to_string(), None, None);

    match result {
        Err(_) => {}
        Ok(r) => {
            let content = format!("{:?}", r.content);
            assert!(
                !content.contains("/etc/"),
                "an absolute glob pattern must not enumerate paths outside the search path: {content}"
            );
        }
    }
}

/// The transcript leak the flat-kiln work closes.
///
/// A kiln-less session's kiln is the daemon's data root, which *contains* the
/// sessions root — so granting the kiln root alone put every session ever
/// recorded inside containment. The deny root removes that subtree; the
/// session's own storage dir, granted as a deeper allowed root, survives it.
#[test]
fn a_session_cannot_read_another_sessions_transcript() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(data_root.join("Note.md"), "kiln note").unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "mine").unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_allowed_roots(vec![data_root.clone(), own_dir.clone()])
        .with_denied_roots(vec![sessions_root.clone()]);

    assert!(
        tools
            .resolve_path(&other_dir.join("session.jsonl").to_string_lossy())
            .is_err(),
        "another session's transcript must be outside containment"
    );
    assert!(
        tools
            .resolve_path(&sessions_root.to_string_lossy())
            .is_err(),
        "the sessions root itself must not be enumerable"
    );
    assert!(
        tools
            .resolve_path(&own_dir.join("session.jsonl").to_string_lossy())
            .is_ok(),
        "a session must still reach its own storage"
    );
    assert!(
        tools
            .resolve_path(&data_root.join("Note.md").to_string_lossy())
            .is_ok(),
        "denying the sessions root must not cost the kiln around it"
    );
}

/// An empty kiln set degrades capabilities, never containment.
///
/// A kiln-less session whose workspace was also detached had
/// `workspace_root == ""`, and `with_allowed_roots` seeds the root list with
/// it. `Path::starts_with("")` is true for every path and `"".components()`
/// counts zero, so the deepest-match rule answered "permitted, depth 0" for
/// the whole filesystem — containment was off, not merely narrow.
#[test]
fn an_empty_root_denies_everything_instead_of_permitting_it() {
    let tools = WorkspaceTools::new("").with_allowed_roots(vec![]);

    assert!(
        tools.resolve_path("/etc/shadow").is_err(),
        "an empty allowed root must deny, not permit the whole filesystem"
    );
    assert!(
        tools.resolve_path("/").is_err(),
        "an empty allowed root must not admit the filesystem root"
    );
}

/// Same rule, reached the other way: an empty string handed in as an *extra*
/// root must not widen a set that was otherwise contained.
#[test]
fn an_empty_extra_root_does_not_widen_containment() {
    let tmp = tempfile::TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();

    let tools = WorkspaceTools::new(&workspace).with_allowed_roots(vec![std::path::PathBuf::new()]);

    assert!(
        tools.resolve_path("/etc/shadow").is_err(),
        "an empty extra root must not disable containment"
    );
    assert!(
        tools
            .resolve_path(&workspace.join("f.txt").to_string_lossy())
            .is_ok(),
        "the real workspace root must still be reachable"
    );
}

/// `grep` containment-checked only the directory it was pointed at and then
/// let ripgrep recurse, so `grep(path = <data root>)` printed every other
/// session's transcript verbatim — the exact subtree `denied_roots` exists to
/// close. `glob` post-filters what it yields; `grep` must too.
#[tokio::test]
#[ignore = "requires: ripgrep"]
async fn grep_does_not_yield_matches_from_a_denied_subtree() {
    let tmp = tempfile::TempDir::new().unwrap();
    let data_root = tmp.path().join("crucible");
    let sessions_root = data_root.join("sessions");
    let workspace = tmp.path().join("workspace");
    let own_dir = sessions_root.join("chat-mine");
    let other_dir = sessions_root.join("chat-theirs");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&own_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(data_root.join("Note.md"), "password hunting notes").unwrap();
    std::fs::write(own_dir.join("session.jsonl"), "password mine").unwrap();
    std::fs::write(other_dir.join("session.jsonl"), "password theirs").unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_allowed_roots(vec![data_root.clone(), own_dir.clone()])
        .with_denied_roots(vec![sessions_root.clone()]);

    let result = tools
        .grep(
            "password".to_string(),
            Some(data_root.to_string_lossy().to_string()),
            None,
            None,
        )
        .await
        .expect("the data root is an allowed root, so the search itself is permitted");
    let content = format!("{:?}", result.content);

    assert!(
        !content.contains("theirs"),
        "another session's transcript must not be yielded by grep: {content}"
    );
    assert!(
        content.contains("hunting"),
        "the kiln around the denied subtree must still be searchable: {content}"
    );
    assert!(
        content.contains("mine"),
        "a session must still grep its own storage: {content}"
    );
}

/// Legacy transcripts left inside a kiln by an incomplete migration are the
/// same secret in a different place. A kiln appearing mid-run is never
/// scanned and `migrate_one` skips on failure, so `{kiln}/.crucible/sessions`
/// has to be denied rather than assumed empty.
#[test]
fn a_kilns_own_legacy_session_directory_is_not_readable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let legacy = kiln.join(".crucible").join("sessions").join("chat-old");
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::write(kiln.join("Note.md"), "kiln note").unwrap();
    std::fs::write(legacy.join("session.jsonl"), "legacy").unwrap();

    let tools = WorkspaceTools::new(&workspace)
        .with_allowed_roots(vec![kiln.clone()])
        .with_denied_roots(vec![kiln.join(".crucible").join("sessions")]);

    assert!(
        tools
            .resolve_path(&legacy.join("session.jsonl").to_string_lossy())
            .is_err(),
        "a legacy in-kiln transcript must be outside containment"
    );
    assert!(
        tools
            .resolve_path(&kiln.join("Note.md").to_string_lossy())
            .is_ok(),
        "denying the legacy sessions dir must not cost the kiln around it"
    );
}
