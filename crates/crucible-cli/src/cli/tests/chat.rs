use super::parse;
use crate::cli::*;

#[test]
fn test_chat_with_env_flag_single() {
    // Should parse --env KEY=VALUE
    let Commands::Chat { agent, env, .. } = parse(&[
        "cru",
        "chat",
        "--agent",
        "opencode",
        "--env",
        "LOCAL_ENDPOINT=http://localhost:11434",
    ]) else {
        panic!("Expected Chat command");
    };
    assert_eq!(agent, Some("opencode".to_string()));
    assert_eq!(env.len(), 1);
    assert_eq!(env[0], "LOCAL_ENDPOINT=http://localhost:11434");
}

#[test]
fn test_chat_with_env_flag_multiple() {
    // Should parse multiple --env flags
    let Commands::Chat { agent, env, .. } = parse(&[
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
    assert_eq!(agent, Some("claude".to_string()));
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
