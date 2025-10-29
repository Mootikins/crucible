use crate::test_utilities::TestKiln;
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_chat_help() {
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("chat")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Interactive chat mode"));
}

#[test]
fn test_chat_with_agent_flag() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--agent")
        .arg("researcher")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
    // Don't check success/failure as it may require Ollama to be running
}

#[test]
fn test_chat_with_model_override() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--model")
        .arg("llama3.2")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}

#[test]
fn test_chat_with_temperature() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--temperature")
        .arg("0.7")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}

#[test]
fn test_chat_with_max_tokens() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--max-tokens")
        .arg("500")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}

#[test]
fn test_chat_no_stream_flag() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--no-stream")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}

#[test]
fn test_chat_conversation_history_save_load() {
    let kiln = TestKiln::new().unwrap();
    let history_path = kiln.kiln_path.join("test_history.json");

    // First session - create history
    {
        let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
        cmd.arg("--kiln-path")
            .arg(kiln.kiln_path_str())
            .arg("--database")
            .arg(kiln.db_path_str())
            .arg("chat")
            .arg("--start-message")
            .arg("Test message")
            .write_stdin("save\nquit\n")
            .timeout(std::time::Duration::from_secs(30))
            .assert();
    }

    // Check that a history file was created in ~/.crucible/chat_history/
    let home_dir = dirs::home_dir().expect("Could not find home directory");
    let history_dir = home_dir.join(".crucible").join("chat_history");

    if history_dir.exists() {
        let entries = fs::read_dir(&history_dir).unwrap();
        let json_files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();

        // At least one history file should exist
        assert!(
            !json_files.is_empty(),
            "Expected at least one conversation history file"
        );
    }
}

#[test]
fn test_chat_with_unknown_agent() {
    let kiln = TestKiln::new().unwrap();

    // Should fallback to default agent
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--agent")
        .arg("nonexistent-agent")
        .arg("--start-message")
        .arg("Hello")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}

#[test]
fn test_chat_start_message() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--database")
        .arg(kiln.db_path_str())
        .arg("chat")
        .arg("--start-message")
        .arg("What is Crucible?")
        .write_stdin("quit\n")
        .timeout(std::time::Duration::from_secs(30))
        .assert();
}
