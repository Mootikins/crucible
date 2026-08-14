use super::parse;
use crate::cli::*;

#[test]
fn test_chat_with_env_flag_single() {
    // Should parse --env KEY=VALUE
    let Commands::Chat { acp, env, .. } = parse(&[
        "cru",
        "chat",
        "--agent",
        "opencode",
        "--env",
        "LOCAL_ENDPOINT=http://localhost:11434",
    ]) else {
        panic!("Expected Chat command");
    };
    assert_eq!(acp, Some("opencode".to_string()));
    assert_eq!(env.len(), 1);
    assert_eq!(env[0], "LOCAL_ENDPOINT=http://localhost:11434");
}

#[test]
fn test_chat_with_env_flag_multiple() {
    // Should parse multiple --env flags
    let Commands::Chat { acp, env, .. } = parse(&[
        "cru",
        "chat",
        "--agent",
        "claude",
        "--env",
        "ANTHROPIC_BASE_URL=http://localhost:4000",
        "--env",
        "ANTHROPIC_MODEL=claude-sonnet",
    ]) else {
        panic!("Expected Chat command");
    };
    assert_eq!(acp, Some("claude".to_string()));
    assert_eq!(env.len(), 2);
    assert!(env.contains(&"ANTHROPIC_BASE_URL=http://localhost:4000".to_string()));
    assert!(env.contains(&"ANTHROPIC_MODEL=claude-sonnet".to_string()));
}

#[test]
fn test_chat_without_env_flag_has_empty_vec() {
    // Default should be empty vec
    let Commands::Chat { env, .. } = parse(&["cru", "chat", "--agent", "opencode"]) else {
        panic!("Expected Chat command");
    };
    assert!(env.is_empty());
}

/// `--agent` was the ACP spelling before agent cards took the name, and it is
/// in people's shells. `cru chat` still cannot take a card — it resolves its
/// agent CLI-side — so here the old spelling keeps working rather than
/// becoming an error that has no card path to point at.
#[test]
fn chat_accepts_both_spellings_of_the_acp_flag() {
    let Commands::Chat { acp, .. } = parse(&["cru", "chat", "--acp", "claude"]) else {
        panic!("Expected Chat command");
    };
    assert_eq!(acp, Some("claude".to_string()));

    let Commands::Chat { acp, .. } = parse(&["cru", "chat", "--agent", "claude"]) else {
        panic!("Expected Chat command");
    };
    assert_eq!(acp, Some("claude".to_string()));
}
