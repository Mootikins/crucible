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
#[ignore = "requires ripgrep"]
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
#[ignore = "requires ripgrep"]
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
