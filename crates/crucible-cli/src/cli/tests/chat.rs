use crate::cli::*;
use clap::Parser;

#[test]
fn test_chat_with_env_flag_single() {
    // Should parse --env KEY=VALUE
    let cli = Cli::try_parse_from([
        "cru",
        "chat",
        "--agent",
        "opencode",
        "--env",
        "LOCAL_ENDPOINT=http://localhost:11434",
    ])
    .unwrap();

    if let Some(Commands::Chat { agent, env, .. }) = cli.command {
        assert_eq!(agent, Some("opencode".to_string()));
        assert_eq!(env.len(), 1);
        assert_eq!(env[0], "LOCAL_ENDPOINT=http://localhost:11434");
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn test_chat_with_env_flag_multiple() {
    // Should parse multiple --env flags
    let cli = Cli::try_parse_from([
        "cru",
        "chat",
        "--agent",
        "claude",
        "--env",
        "ANTHROPIC_BASE_URL=http://localhost:4000",
        "--env",
        "ANTHROPIC_MODEL=claude-sonnet",
    ])
    .unwrap();

    if let Some(Commands::Chat { agent, env, .. }) = cli.command {
        assert_eq!(agent, Some("claude".to_string()));
        assert_eq!(env.len(), 2);
        assert!(env.contains(&"ANTHROPIC_BASE_URL=http://localhost:4000".to_string()));
        assert!(env.contains(&"ANTHROPIC_MODEL=claude-sonnet".to_string()));
    } else {
        panic!("Expected Chat command");
    }
}

#[test]
fn test_chat_without_env_flag_has_empty_vec() {
    // Default should be empty vec
    let cli = Cli::try_parse_from(["cru", "chat", "--agent", "opencode"]).unwrap();

    if let Some(Commands::Chat { env, .. }) = cli.command {
        assert!(env.is_empty());
    } else {
        panic!("Expected Chat command");
    }
}
