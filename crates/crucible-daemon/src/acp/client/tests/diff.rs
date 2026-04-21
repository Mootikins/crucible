use std::io::Write;

use serde_json::json;
use tempfile::NamedTempFile;

use super::test_path;
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crucible_core::types::acp::ToolCallInfo;

#[test]
fn test_generate_diff_for_write_operation() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);

    // Create a temp file with initial content
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "line1").unwrap();
    writeln!(temp_file, "line2").unwrap();
    writeln!(temp_file, "line3").unwrap();
    let path = temp_file.path().to_string_lossy().to_string();

    // Simulate a write tool call that modifies content
    let tool_call = ToolCallInfo::new("update_note")
        .with_id("tool-1")
        .with_arguments(json!({
            "path": path,
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
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);

    // Create a temp file with initial content
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "fn main() {{").unwrap();
    writeln!(temp_file, "    println!(\"Hello\");").unwrap();
    writeln!(temp_file, "}}").unwrap();
    let path = temp_file.path().to_string_lossy().to_string();

    // Simulate an Edit tool call (Claude Code style: old_string/new_string)
    let tool_call = ToolCallInfo::new("Edit")
        .with_id("tool-1")
        .with_arguments(json!({
            "file_path": path,
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
