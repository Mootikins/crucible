//! Chat Command - ACP-based Natural Language Interface
//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{error, info, warn, debug, trace};
use walkdir::WalkDir;

use crate::acp::{discover_agent, ContextEnricher, CrucibleAcpClient};
use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;
use crate::factories;
use crucible_pipeline::NotePipeline;
use crucible_watch::{EventFilter, FileEvent, FileEventKind, WatchMode};
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use tokio::sync::mpsc;

/// Chat mode - can be toggled during session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    /// Plan mode - read-only, agent cannot modify files
    Plan,
    /// Act mode - write-enabled, agent can modify files
    Act,
}

impl ChatMode {
    /// Get the display name for this mode
    pub fn display_name(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
        }
    }

    /// Toggle to the other mode
    pub fn toggle(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::Plan,
        }
    }

    /// Check if this mode allows writes
    pub fn is_read_only(&self) -> bool {
        matches!(self, ChatMode::Plan)
    }
}

/// Execute the chat command
///
/// # Arguments
/// * `config` - CLI configuration
/// * `agent_name` - Optional preferred agent name
/// * `query` - Optional one-shot query (if None, starts interactive mode)
/// * `read_only` - Initial mode: if true, starts in plan mode; if false, starts in act mode
/// * `no_context` - If true, skip context enrichment
/// * `no_process` - If true, skip auto-processing of files before context enrichment
/// * `context_size` - Number of context results to include
pub async fn execute(
    config: CliConfig,
    agent_name: Option<String>,
    query: Option<String>,
    read_only: bool,
    no_context: bool,
    no_process: bool,
    context_size: Option<usize>,
) -> Result<()> {
    // Determine initial mode
    let initial_mode = if read_only {
        ChatMode::Plan
    } else {
        ChatMode::Act
    };

    use crate::output;
    use colored::Colorize;

    println!("{}", "╔═══════════════════════════════════════════╗".bright_blue().bold());
    println!("{}", "║       🤖 Initializing Crucible Chat       ║".bright_blue().bold());
    println!("{}", "╚═══════════════════════════════════════════╝".bright_blue().bold());

    info!("Starting chat command");
    info!("Initial mode: {}", initial_mode.display_name());

    // Get default agent from config before moving config
    let default_agent_from_config = config.llm.default_agent.clone();

    // Initialize storage using factory pattern
    output::info("Initializing storage...");
    let storage_client = factories::create_surrealdb_storage(&config).await?;
    factories::initialize_surrealdb_schema(&storage_client).await?;
    output::success("Storage initialized");

    // Auto-process files to generate embeddings (unless --no-process or --no-context)
    if !no_process && !no_context {
        use indicatif::{ProgressBar, ProgressStyle};

        output::info("Running pipeline to ensure embeddings are up-to-date...");

        let pipeline = factories::create_pipeline(
            storage_client.clone(),
            &config,
            false, // force=false for incremental processing
        ).await?;

        // Discover markdown files in kiln
        let kiln_path = &config.kiln.path;
        let files = discover_markdown_files(kiln_path)?;

        if files.is_empty() {
            warn!("No markdown files found in kiln at {}", kiln_path.display());
        } else {
            let pb = ProgressBar::new(files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} files ({eta})")
                    .unwrap()
                    .progress_chars("=>-")
            );

            output::info(&format!("Processing {} markdown files", files.len()));

            // Process each file (best effort - don't fail if one file errors)
            for file in files {
                pb.inc(1);
                if let Err(e) = pipeline.process(&file).await {
                    warn!("Failed to process {}: {}", file.display(), e);
                }
            }

            pb.finish_with_message("Processing complete");
            output::success("Pipeline processing complete");
        }

        // Spawn background watch task for auto-reindexing during chat
        // This runs silently in the background, reprocessing changed files
        let watch_config = config.clone();
        let watch_pipeline = Arc::new(pipeline);

        tokio::spawn(async move {
            if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                tracing::error!("Background watch failed: {}", e);
            }
        });

        info!("Background watch spawned for chat mode");
    } else if no_process {
        output::info("File processing skipped due to --no-process flag");
    }

    // Initialize core facade (still needed for semantic search in ContextEnricher)
    // Reuse the storage client we created earlier (line 80) instead of creating a new one
    output::info("Initializing core...");
    let core = Arc::new(CrucibleCoreFacade::from_storage(
        storage_client.clone(),
        config
    ));
    output::success("Core initialized");

    // Discover agent (use --agent flag, or fall back to config default)
    output::info("Discovering ACP agent...");
    let preferred_agent = agent_name.or(default_agent_from_config);
    let agent = discover_agent(preferred_agent.as_deref()).await?;
    output::success(&format!("Using agent: {} ({})", agent.name, agent.command));

    // Create ACP client with kiln path for tool initialization
    let kiln_path = core.config().kiln.path.clone();
    let mut client = CrucibleAcpClient::new(agent, initial_mode.is_read_only())
        .with_kiln_path(kiln_path);

    // Spawn agent (tools will be initialized automatically)
    client.spawn().await?;

    // Handle query
    if let Some(query_text) = query {
        // One-shot mode
        info!("One-shot query mode");

        let prompt = if no_context {
            info!("Context enrichment disabled");
            query_text
        } else {
            // Enrich with context
            info!("Enriching query with context...");
            let enricher = ContextEnricher::new(core.clone(), context_size);
            enricher.enrich(&query_text).await?
        };

        // Start chat with enriched prompt
        client.start_chat(&prompt).await?;

        // Cleanup
        client.shutdown().await?;
    } else {
        // Interactive mode
        info!("Interactive chat mode");
        run_interactive_session(core, &mut client, initial_mode, no_context, context_size).await?;

        // Cleanup
        client.shutdown().await?;
    }

    Ok(())
}

/// Run an interactive chat session with mode toggling support
async fn run_interactive_session(
    core: Arc<CrucibleCoreFacade>,
    client: &mut CrucibleAcpClient,
    initial_mode: ChatMode,
    no_context: bool,
    context_size: Option<usize>,
) -> Result<()> {
    use colored::Colorize;
    use reedline::{DefaultPrompt, Reedline, Signal};

    let mut current_mode = initial_mode;
    let mut line_editor = Reedline::create();
    let enricher = ContextEnricher::new(core.clone(), context_size);

    println!("\n{}", "🤖 Crucible Chat".bright_blue().bold());
    println!("{}", "=================".bright_blue());
    println!("Mode: {} {}",
        current_mode.display_name().bright_cyan().bold(),
        if current_mode == ChatMode::Plan { "(read-only)" } else { "(write-enabled)" }.dimmed()
    );
    println!();
    println!("{}", "Commands:".bold());
    println!("  {} - Switch to plan mode (read-only)", "/plan".green());
    println!("  {} - Switch to act mode (write-enabled)", "/act".green());
    println!("  {} - Exit chat", "/exit".green());
    println!();

    loop {
        // Create simple prompt based on current mode
        let prompt_indicator = format!(
            "{} {} ",
            current_mode.display_name(),
            if current_mode == ChatMode::Plan { "📖" } else { "✏️" }
        );
        let prompt = DefaultPrompt::new(
            reedline::DefaultPromptSegment::Basic(prompt_indicator),
            reedline::DefaultPromptSegment::Empty,
        );

        // Read user input
        let sig = line_editor.read_line(&prompt);

        match sig {
            Ok(Signal::Success(buffer)) => {
                let input = buffer.trim();

                // Handle empty input
                if input.is_empty() {
                    continue;
                }

                // Handle commands
                if input == "/exit" || input == "/quit" {
                    println!("{}", "👋 Goodbye!".bright_blue());
                    break;
                } else if input == "/plan" {
                    current_mode = ChatMode::Plan;
                    println!("{} Mode switched to: {} (read-only)",
                        "→".bright_cyan(),
                        "plan".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input == "/act" {
                    current_mode = ChatMode::Act;
                    println!("{} Mode switched to: {} (write-enabled)",
                        "→".bright_cyan(),
                        "act".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                }

                // Prepare the message (with or without context enrichment)
                let message = if no_context {
                    input.to_string()
                } else {
                    // Show context enrichment indicator
                    print!("{} ", "🔍 Finding relevant context...".bright_cyan().dimmed());
                    io::stdout().flush().unwrap();

                    let enriched_result = enricher.enrich(input).await;

                    // Clear the enrichment indicator
                    print!("\r{}\r", " ".repeat(35));
                    io::stdout().flush().unwrap();

                    match enriched_result {
                        Ok(enriched) => enriched,
                        Err(e) => {
                            error!("Context enrichment failed: {}", e);
                            info!("Falling back to original message");
                            input.to_string()
                        }
                    }
                };

                // Send to agent
                println!(); // Blank line before response

                // Show "thinking" indicator
                print!("{} ", "🤔 Thinking...".bright_blue().dimmed());
                use std::io::{self, Write};
                io::stdout().flush().unwrap();

                match client.send_message(&message).await {
                    Ok(response) => {
                        // Clear the "thinking" indicator
                        print!("\r{}\r", " ".repeat(20));
                        io::stdout().flush().unwrap();

                        // Print agent response with nice border
                        println!("{}", "╭─ Agent Response ──────────────────────────╮".bright_blue());
                        for line in response.lines() {
                            println!("│ {}", line);
                        }
                        println!("{}", "╰────────────────────────────────────────────╯".bright_blue());
                        println!(); // Blank line after response
                    }
                    Err(e) => {
                        // Clear the "thinking" indicator
                        print!("\r{}\r", " ".repeat(20));
                        io::stdout().flush().unwrap();

                        error!("Failed to send message: {}", e);
                        println!("{} Error: {}", "✗".red(), e);
                    }
                }
            }
            Ok(Signal::CtrlC) => {
                println!("\n{}", "Interrupted. Type /exit to quit.".yellow());
                continue;
            }
            Ok(Signal::CtrlD) => {
                println!("\n{}", "👋 Goodbye!".bright_blue());
                break;
            }
            Err(err) => {
                error!("Error reading input: {}", err);
                break;
            }
        }
    }

    Ok(())
}

/// Helper function to print a prompt with context highlighting
fn print_enriched_prompt(prompt: &str) {
    use colored::Colorize;

    println!("\n{}", "╭─ Enriched Prompt ─────────────────────────╮".blue());
    for line in prompt.lines() {
        if line.starts_with("# ") {
            println!("{}", line.bright_blue().bold());
        } else if line.starts_with("## ") {
            println!("{}", line.cyan().bold());
        } else {
            println!("{}", line);
        }
    }
    println!("{}", "╰────────────────────────────────────────────╯".blue());
}

/// Discover markdown files in a directory
///
/// This function recursively searches for .md files in the given path.
/// If the path is a file, returns it if it's a markdown file.
/// If the path is a directory, walks the tree to find all markdown files.
fn discover_markdown_files(path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if path.is_file() {
        if is_markdown_file(path) {
            files.push(path.to_path_buf());
        }
    } else if path.is_dir() {
        for entry in WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path();
            if entry_path.is_file() && is_markdown_file(entry_path) {
                files.push(entry_path.to_path_buf());
            }
        }
    }

    Ok(files)
}

/// Check if a path is a markdown file
fn is_markdown_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

/// Spawn background watch task for chat mode
///
/// This function runs silently in the background, watching for file changes
/// and reprocessing them automatically. All output goes through tracing
/// (logged to file) to avoid polluting stdio used for JSON-RPC.
///
/// The background task will be automatically cancelled when the chat
/// command exits (tokio runtime cleanup).
async fn spawn_background_watch(
    config: CliConfig,
    pipeline: Arc<NotePipeline>,
) -> Result<()> {
    let kiln_path = config.kiln.path.clone();

    // Create watcher via factory (DIP pattern - depends only on FileWatcher trait)
    let mut watcher_arc = factories::create_file_watcher(&config)?;

    // Get mutable access to configure the watcher
    let watcher = Arc::get_mut(&mut watcher_arc)
        .ok_or_else(|| anyhow::anyhow!("Failed to get mutable watcher reference"))?;

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Configure watch with markdown file filter and debouncing
    let filter = EventFilter::new().with_extension("md");
    let watch_config = WatchConfig {
        id: "chat-background-watch".to_string(),
        recursive: true,
        filter: Some(filter),
        debounce: DebounceConfig::default(),
        handler_config: HandlerConfig::default(),
        mode: WatchMode::Standard, // Standard mode for immediate event processing
        backend_options: Default::default(),
    };

    // Start watching the kiln directory
    let _handle = watcher.watch(kiln_path.clone(), watch_config).await?;
    info!("Background watch started for chat mode on: {}", kiln_path.display());

    // Event processing loop (runs until chat exits)
    while let Some(event) = rx.recv().await {
        match event.kind {
            FileEventKind::Created | FileEventKind::Modified => {
                debug!("Background watch detected change: {}", event.path.display());

                // Silently reprocess changed file
                match pipeline.process(&event.path).await {
                    Ok(crucible_core::processing::ProcessingResult::Success { .. }) => {
                        debug!("Background reprocessed: {}", event.path.display());
                    }
                    Ok(crucible_core::processing::ProcessingResult::Skipped) |
                    Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                        trace!("Background skipped (unchanged): {}", event.path.display());
                    }
                    Err(e) => {
                        warn!("Background reprocess failed for {}: {}", event.path.display(), e);
                    }
                }
            }
            FileEventKind::Deleted => {
                debug!("File deleted: {}", event.path.display());
                // Could mark as deleted in DB in future
            }
            _ => {
                trace!("Ignoring event: {:?} for {}", event.kind, event.path.display());
            }
        }
    }

    info!("Background watch stopped");
    Ok(())
}
