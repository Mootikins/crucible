use super::parse;
use crate::cli::*;

#[test]
fn chat_card_is_distinct_from_acp_and_cannot_replace_a_resumed_agent() {
    use clap::Parser;
    let Commands::Chat { card, acp, .. } = parse(&["cru", "chat", "--card", "researcher"]) else {
        panic!("chat")
    };
    assert_eq!(card.as_deref(), Some("researcher"));
    assert!(acp.is_none());
    for conflict in ["--acp", "--agent", "--resume", "--replay"] {
        assert!(
            Cli::try_parse_from(["cru", "chat", "--card", "researcher", conflict, "other"])
                .is_err()
        );
    }
}

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
/// in people's shells. Preserve that alias; cards use the distinct `--card` flag.
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
