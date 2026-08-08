use serde_json::json;
use tempfile::TempDir;

use super::test_path;
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crucible_core::types::acp::ToolCallInfo;

/// A client whose ACP session has `working_dir` as its workspace.
///
/// The workspace is load-bearing, not scenery: `generate_diff_for_write`
/// delegates to `tools::diff_synth`, which will only read a file it can
/// prove is inside the session's workspace — the diff preview is emitted
/// before the permission gate resolves, so an unbounded read there is
/// published whatever the user later decides. These tests used to pass
/// `working_dir: None` with a file in the system temp dir, which is the
/// exfiltration shape itself rather than a realistic session.
fn client_in(working_dir: &TempDir) -> CrucibleAcpClient {
    CrucibleAcpClient::new(ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: Some(working_dir.path().to_path_buf()),
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    })
}

#[test]
fn test_generate_diff_for_write_operation() {
    let workspace = TempDir::new().unwrap();
    let client = client_in(&workspace);

    std::fs::write(workspace.path().join("note.md"), "line1\nline2\nline3\n").unwrap();

    // Simulate a write tool call that modifies content
    let tool_call = ToolCallInfo::new("update_note")
        .with_id("tool-1")
        .with_arguments(json!({
            "path": "note.md",
            "content": "line1\nmodified\nline3\n"
        }));

    let diff = client.generate_diff_for_write(&tool_call);
    assert!(diff.is_some(), "Should generate diff for write operation");

    let diff_str = diff.unwrap();
    assert!(diff_str.contains("-line2"), "Should show deleted line");
    assert!(diff_str.contains("+modified"), "Should show inserted line");
}

#[test]
fn test_generate_diff_for_edit_tool_string_replacement() {
    let workspace = TempDir::new().unwrap();
    let client = client_in(&workspace);

    std::fs::write(
        workspace.path().join("main.rs"),
        "fn main() {\n    println!(\"Hello\");\n}\n",
    )
    .unwrap();

    // Simulate an Edit tool call (Claude Code style: old_string/new_string)
    let tool_call = ToolCallInfo::new("Edit")
        .with_id("tool-1")
        .with_arguments(json!({
            "file_path": "main.rs",
            "old_string": "println!(\"Hello\")",
            "new_string": "println!(\"Hello, World!\")"
        }));

    let diff = client.generate_diff_for_write(&tool_call);
    assert!(diff.is_some(), "Should generate diff for Edit tool");

    let diff_str = diff.unwrap();
    assert!(diff_str.contains("-"), "Should have deletion");
    assert!(diff_str.contains("+"), "Should have insertion");
    assert!(
        diff_str.contains("Hello, World!"),
        "Should show new content"
    );
}

/// The ACP leg of the exfiltration, at its own call site
/// (`acp/client/tools.rs:202`): an agent names a file outside the session's
/// workspace and the preview must not open it. Denying the tool call cannot
/// help — `generate_diff_for_write` runs while the call is still pending and
/// its output is what the user is shown.
#[test]
fn generate_diff_refuses_a_target_outside_the_workspace() {
    let workspace = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let client = client_in(&workspace);

    let secret = outside.path().join("id_rsa");
    std::fs::write(&secret, "SENTINEL-ACP-PRIVATE-KEY").unwrap();
    // The process CAN read it — so an empty result proves containment
    // refused the read, not that the fixture was unreadable.
    assert!(std::fs::read_to_string(&secret).is_ok());

    let tool_call = ToolCallInfo::new("Edit")
        .with_id("tool-1")
        .with_arguments(json!({
            "file_path": secret.to_string_lossy(),
            "old_string": "",
            "new_string": "x"
        }));

    let diff = client.generate_diff_for_write(&tool_call);
    assert!(
        !diff
            .unwrap_or_default()
            .contains("SENTINEL-ACP-PRIVATE-KEY"),
        "an out-of-workspace file body reached the ACP diff preview"
    );
}

#[test]
fn test_generate_diff_skips_read_operations() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);

    // Read operation should not generate diff
    let test_file = test_path("test.md");
    let tool_call = ToolCallInfo::new("read_note")
        .with_id("tool-1")
        .with_arguments(json!({"path": test_file.to_string_lossy()}));

    let diff = client.generate_diff_for_write(&tool_call);
    assert!(
        diff.is_none(),
        "Should not generate diff for read operation"
    );
}
