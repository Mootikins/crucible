use anyhow::Result;
use clap::Parser;

use crucible_cli::{cli::{Cli, Commands}, commands, config};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    let env_filter = format!("crucible_cli={},crucible_mcp={}", log_level, log_level);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(env_filter))
        .init();

    // Load configuration
    let config = config::CliConfig::load(
        cli.config,
        cli.vault_path,
        cli.embedding_url,
        cli.embedding_model,
    )?;

    // Execute command (default to REPL if no command provided)
    match cli.command {
        Some(Commands::Search {
            query,
            limit,
            format,
            show_content,
        }) => commands::search::execute(config, query, limit, format, show_content).await?,

        Some(Commands::Fuzzy {
            query,
            content,
            tags,
            paths,
            limit,
        }) => commands::fuzzy::execute(config, query.unwrap_or_default(), content, tags, paths, limit).await?,

        Some(Commands::Semantic {
            query,
            top_k,
            format,
            show_scores,
        }) => commands::semantic::execute(config, query, top_k, format, show_scores).await?,

        Some(Commands::Note(cmd)) => commands::note::execute(config, cmd).await?,

        Some(Commands::Index { path, force, glob }) => commands::index::execute(config, path, force, glob).await?,

        Some(Commands::Stats) => commands::stats::execute(config).await?,

        Some(Commands::Run { script, args }) => commands::rune::execute(config, script, args).await?,

        Some(Commands::Commands) => commands::rune::list_commands(config).await?,

        Some(Commands::Config(cmd)) => commands::config::execute(cmd).await?,

        Some(Commands::Chat {
            agent,
            model,
            temperature,
            max_tokens,
            no_stream,
            start_message,
            history,
        }) => commands::chat::execute(
            config,
            agent,
            model,
            temperature,
            max_tokens,
            !no_stream,
            start_message,
            history,
        ).await?,

        // Commands::EnhancedChat { // Temporarily disabled
        //     agent,
        //     model,
        //     temperature,
        //     max_tokens,
        //     performance_tracking,
        //     start_message,
        //     history,
        // } => commands::enhanced_chat_session::execute(
        //     config,
        //     agent,
        //     model,
        //     temperature,
        //     max_tokens,
        //     performance_tracking,
        //     start_message,
        //     history,
        // ).await?,

        // Commands::Agent(cmd) => commands::agent_management::execute(config, cmd).await?, // Temporarily disabled

        None => {
            // Default to REPL when no command is provided
            commands::repl::execute(config, cli.db_path, cli.tool_dir, cli.format).await?
        }
    }

    Ok(())
}