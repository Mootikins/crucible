//! Chat Command - ACP-based Natural Language Interface
//!
//! Provides an interactive chat interface using the Agent Client Protocol.
//! Supports toggleable plan (read-only) and act (write-enabled) modes.

use anyhow::Result;
use crucible_acp::humanize_tool_title;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, trace, warn};
use walkdir::WalkDir;

use crate::acp::{discover_agent, ContextEnricher, CrucibleAcpClient};
use crate::config::CliConfig;
use crate::core_facade::CrucibleCoreFacade;
use crate::factories;
use crate::formatting::render_markdown;
use crate::progress::{BackgroundProgress, LiveProgress, StatusLine};
use crucible_pipeline::NotePipeline;
use crucible_watch::traits::{DebounceConfig, HandlerConfig, WatchConfig};
use crucible_watch::{EventFilter, FileEvent, FileEventKind, WatchMode};
use tokio::sync::mpsc;

/// Chat mode - can be toggled during session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMode {
    /// Plan mode - read-only, agent cannot modify files
    Plan,
    /// Act mode - write-enabled, agent can modify files (with confirmation)
    Act,
    /// Auto-approve mode - write-enabled, agent can modify files without confirmation
    AutoApprove,
}

impl ChatMode {
    /// Get the display name for this mode
    pub fn display_name(&self) -> &'static str {
        match self {
            ChatMode::Plan => "plan",
            ChatMode::Act => "act",
            ChatMode::AutoApprove => "auto",
        }
    }

    /// Get the mode description for display
    pub fn description(&self) -> &'static str {
        match self {
            ChatMode::Plan => "read-only",
            ChatMode::Act => "write-enabled",
            ChatMode::AutoApprove => "auto-approve",
        }
    }

    /// Cycle to the next mode (Plan -> Act -> AutoApprove -> Plan)
    pub fn cycle_next(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::AutoApprove,
            ChatMode::AutoApprove => ChatMode::Plan,
        }
    }

    /// Toggle to the other mode (legacy, Plan <-> Act only)
    pub fn toggle(&self) -> Self {
        match self {
            ChatMode::Plan => ChatMode::Act,
            ChatMode::Act => ChatMode::Plan,
            ChatMode::AutoApprove => ChatMode::Plan,
        }
    }

    /// Check if this mode allows writes
    pub fn is_read_only(&self) -> bool {
        matches!(self, ChatMode::Plan)
    }

    /// Check if this mode auto-approves operations
    pub fn is_auto_approve(&self) -> bool {
        matches!(self, ChatMode::AutoApprove)
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

    

    info!("Starting chat command");
    info!("Initial mode: {}", initial_mode.display_name());

    // Single-line status display for clean startup UX
    let mut status = StatusLine::new();

    // Get default agent from config before moving config
    let default_agent_from_config = config.acp.default_agent.clone();

    // PARALLEL INITIALIZATION: Run storage init and agent discovery concurrently
    status.update("Initializing storage and discovering agent...");

    let preferred_agent = agent_name.or(default_agent_from_config);
    let config_for_storage = config.clone();

    let (storage_result, agent_result) = tokio::join!(
        async {
            let client = factories::create_surrealdb_storage(&config_for_storage).await?;
            factories::initialize_surrealdb_schema(&client).await?;
            Ok::<_, anyhow::Error>(client)
        },
        discover_agent(preferred_agent.as_deref())
    );

    let storage_client = storage_result?;
    let agent = agent_result?;

    // Quick sync check + background processing (unless --no-process or --no-context)
    let bg_progress: Option<BackgroundProgress> = if !no_process && !no_context {
        use crate::sync::quick_sync_check;

        status.update("Checking for file changes...");

        let kiln_path = &config.kiln_path;
        let sync_status = quick_sync_check(&storage_client, kiln_path).await?;

        if sync_status.needs_processing() {
            let pending = sync_status.pending_count();
            status.update(&format!("Starting background indexing ({} files)...", pending));

            // Create pipeline for background processing
            let pipeline = factories::create_pipeline(
                storage_client.clone(),
                &config,
                false,
            )
            .await?;

            let files_to_process = sync_status.files_to_process();
            let bg_pipeline = Arc::new(pipeline);
            let progress = BackgroundProgress::new(pending);

            // Spawn background processing with progress tracking
            let bg_pipeline_clone = bg_pipeline.clone();
            let progress_clone = progress.clone();
            tokio::spawn(async move {
                for file in files_to_process {
                    match bg_pipeline_clone.process(&file).await {
                        Ok(_) => progress_clone.inc_completed(),
                        Err(e) => {
                            tracing::warn!("Background process failed for {}: {}", file.display(), e);
                            progress_clone.inc_failed();
                        }
                    }
                }
                tracing::debug!("Background file processing completed");
            });

            // Spawn background watch task
            let watch_config = config.clone();
            let watch_pipeline = bg_pipeline;
            tokio::spawn(async move {
                if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                    tracing::error!("Background watch failed: {}", e);
                }
            });

            info!("Background processing spawned for {} files", pending);
            Some(progress)
        } else {
            // All files up to date, still spawn watch
            let pipeline = factories::create_pipeline(
                storage_client.clone(),
                &config,
                false,
            )
            .await?;
            let watch_config = config.clone();
            let watch_pipeline = Arc::new(pipeline);
            tokio::spawn(async move {
                if let Err(e) = spawn_background_watch(watch_config, watch_pipeline).await {
                    tracing::error!("Background watch failed: {}", e);
                }
            });
            None
        }
    } else {
        None
    };

    // Initialize core facade
    status.update("Initializing core...");
    let core = Arc::new(CrucibleCoreFacade::from_storage(
        storage_client.clone(),
        config,
    ));

    // Get cached embedding provider
    status.update("Initializing embedding provider...");
    let embedding_provider = factories::get_or_create_embedding_provider(core.config()).await?;

    // Get knowledge repository from storage
    let knowledge_repo = core.storage().as_knowledge_repository();

    // Create ACP client with kiln path and MCP dependencies for in-process tool execution
    let kiln_path = core.config().kiln_path.clone();
    let acp_config = core.config().acp.clone();
    let mut client =
        CrucibleAcpClient::with_acp_config(agent, initial_mode.is_read_only(), acp_config)
            .with_kiln_path(kiln_path)
            .with_mcp_dependencies(knowledge_repo, embedding_provider);

    // Spawn agent (tools will be initialized via in-process SSE MCP server)
    status.update("Connecting to agent...");
    client.spawn().await?;

    // Finalize startup status
    status.success("Ready");

    // Start live progress display if we have background processing
    let live_progress = bg_progress.map(LiveProgress::start);

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
        run_interactive_session(core, &mut client, initial_mode, no_context, context_size, live_progress).await?;

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
    _live_progress: Option<LiveProgress>,
) -> Result<()> {
    use colored::Colorize;
    use reedline::{
        default_emacs_keybindings, DefaultPrompt, EditCommand, Emacs, KeyCode, KeyModifiers,
        Reedline, ReedlineEvent, Signal,
    };
    use std::time::Instant;

    let mut current_mode = initial_mode;
    let mut last_ctrl_c: Option<Instant> = None;

    // Configure keybindings:
    // - Shift+Tab: silent mode cycle
    // - Ctrl+J: insert newline (multiline input)
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::SHIFT,
        KeyCode::BackTab,
        ReedlineEvent::ExecuteHostCommand("\x00mode".to_string()),
    );
    keybindings.add_binding(
        KeyModifiers::CONTROL,
        KeyCode::Char('j'),
        ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
    );
    let edit_mode = Box::new(Emacs::new(keybindings));
    let mut line_editor = Reedline::create().with_edit_mode(edit_mode);

    let enricher = ContextEnricher::new(core.clone(), context_size);

    println!("\n{}", "🤖 Crucible Chat".bright_blue().bold());
    println!("{}", "=================".bright_blue());
    println!(
        "Mode: {} {}",
        current_mode.display_name().bright_cyan().bold(),
        format!("({})", current_mode.description()).dimmed()
    );
    println!();
    println!("{}", "Commands:".bold());
    println!("  {} - Switch to plan mode (read-only)", "/plan".green());
    println!("  {} - Switch to act mode (write-enabled)", "/act".green());
    println!("  {} - Switch to auto-approve mode", "/auto".green());
    println!("  {} - Cycle modes (or Shift+Tab)", "/mode".green());
    println!(
        "  {} - Search knowledge base",
        "/search <query>".green()
    );
    println!();
    println!(
        "{} | {}",
        "Ctrl+J for newline".dimmed(),
        "Ctrl+C twice to exit".dimmed()
    );

    loop {
        // Create simple prompt based on current mode
        let mode_icon = match current_mode {
            ChatMode::Plan => "📖",
            ChatMode::Act => "✏️",
            ChatMode::AutoApprove => "⚡",
        };

        let prompt_indicator = format!("{} {} ", current_mode.display_name(), mode_icon);
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

                // Handle silent mode switch (from Shift+Tab keybinding)
                // This cycles mode without any visual output - prompt updates on next iteration
                if input == "\x00mode" {
                    current_mode = current_mode.cycle_next();
                    continue;
                }

                // Handle commands
                if input == "/exit" || input == "/quit" {
                    println!("{}", "👋 Goodbye!".bright_blue());
                    break;
                } else if input == "/plan" {
                    current_mode = ChatMode::Plan;
                    println!(
                        "{} Mode switched to: {} (read-only)",
                        "→".bright_cyan(),
                        "plan".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input == "/act" {
                    current_mode = ChatMode::Act;
                    println!(
                        "{} Mode switched to: {} (write-enabled)",
                        "→".bright_cyan(),
                        "act".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input == "/auto" {
                    current_mode = ChatMode::AutoApprove;
                    println!(
                        "{} Mode switched to: {} (auto-approve)",
                        "→".bright_cyan(),
                        "auto".bright_cyan().bold()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input == "/mode" {
                    current_mode = current_mode.cycle_next();
                    println!(
                        "{} Mode: {} ({})",
                        "→".bright_cyan(),
                        current_mode.display_name().bright_cyan().bold(),
                        current_mode.description()
                    );
                    // Note: In full implementation, would update client permissions here
                    continue;
                } else if input.starts_with("/search ") || input == "/search" {
                    let query = if input == "/search" {
                        ""
                    } else {
                        input[8..].trim()
                    };

                    if query.is_empty() {
                        println!(
                            "{} Usage: /search <query>",
                            "!".yellow()
                        );
                        continue;
                    }

                    // Show searching indicator
                    print!("{} ", "🔍 Searching...".bright_cyan().dimmed());
                    use std::io::{self, Write};
                    io::stdout().flush().unwrap();

                    match core.semantic_search(query, 10).await {
                        Ok(results) => {
                            // Clear searching indicator
                            print!("\r{}\r", " ".repeat(20));
                            io::stdout().flush().unwrap();

                            if results.is_empty() {
                                println!(
                                    "{} No results found for: {}",
                                    "○".dimmed(),
                                    query.italic()
                                );
                            } else {
                                println!(
                                    "{} Found {} results:\n",
                                    "●".bright_blue(),
                                    results.len()
                                );
                                for (i, result) in results.iter().enumerate() {
                                    println!(
                                        "  {} {} {}",
                                        format!("{}.", i + 1).dimmed(),
                                        result.title.bright_white(),
                                        format!("({:.0}%)", result.similarity * 100.0).dimmed()
                                    );
                                    // Show snippet preview (first non-empty line)
                                    if !result.snippet.is_empty() {
                                        let preview = result
                                            .snippet
                                            .lines()
                                            .find(|l| !l.trim().is_empty())
                                            .unwrap_or("");
                                        if !preview.is_empty() {
                                            let truncated = if preview.len() > 80 {
                                                format!("{}...", &preview[..77])
                                            } else {
                                                preview.to_string()
                                            };
                                            println!("     {}", truncated.dimmed());
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            // Clear searching indicator
                            print!("\r{}\r", " ".repeat(20));
                            io::stdout().flush().unwrap();

                            println!("{} Search failed: {}", "✗".red(), e);
                        }
                    }
                    println!();
                    continue;
                }

                // Prepare the message (with or without context enrichment)
                let message = if no_context {
                    input.to_string()
                } else {
                    // Show context enrichment indicator
                    print!(
                        "{} ",
                        "🔍 Finding relevant context...".bright_cyan().dimmed()
                    );
                    io::stdout().flush().unwrap();

                    let enriched_result = enricher.enrich_with_results(input).await;

                    // Clear the enrichment indicator
                    print!("\r{}\r", " ".repeat(35));
                    io::stdout().flush().unwrap();

                    match enriched_result {
                        Ok(result) => {
                            // Display the notes found to the user
                            if !result.notes_found.is_empty() {
                                println!(
                                    "{} Found {} relevant notes:",
                                    "📚".dimmed(),
                                    result.notes_found.len()
                                );
                                for note in &result.notes_found {
                                    println!(
                                        "   {} {} {}",
                                        "•".dimmed(),
                                        note.title.bright_white(),
                                        format!("({:.0}%)", note.similarity * 100.0).dimmed()
                                    );
                                }
                            }
                            result.prompt
                        }
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
                    Ok((response, tool_calls)) => {
                        // Clear the "thinking" indicator
                        print!("\r{}\r", " ".repeat(20));
                        io::stdout().flush().unwrap();

                        // Print agent response with markdown rendering
                        let rendered = render_markdown(&response);
                        // Print with indicator on first line, rest indented
                        let mut lines = rendered.lines();
                        if let Some(first) = lines.next() {
                            println!("{} {}", "●".bright_blue(), first);
                            for line in lines {
                                println!("  {}", line);
                            }
                        }

                        // Show tool calls that are missing from the inline stream (fallback)
                        let has_inline_tools = response.contains('▷');
                        if !tool_calls.is_empty()
                            && (response.trim().is_empty() || !has_inline_tools)
                        {
                            for tool in &tool_calls {
                                let args_str = format_tool_args(&tool.arguments);
                                let display_title = humanize_tool_title(&tool.title);
                                println!(
                                    "  {} {}({})",
                                    "▷".cyan(),
                                    display_title,
                                    args_str.dimmed()
                                );
                            }
                        }
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
                use std::time::Duration;
                if let Some(last) = last_ctrl_c {
                    if last.elapsed() < Duration::from_secs(2) {
                        println!("\n{}", "👋 Goodbye!".bright_blue());
                        break;
                    }
                }
                last_ctrl_c = Some(Instant::now());
                println!("\n{}", "Press Ctrl+C again to exit".yellow());
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

    println!(
        "\n{}",
        "╭─ Enriched Prompt ─────────────────────────╮".blue()
    );
    for line in prompt.lines() {
        if line.starts_with("# ") {
            println!("{}", line.bright_blue().bold());
        } else if line.starts_with("## ") {
            println!("{}", line.cyan().bold());
        } else {
            println!("{}", line);
        }
    }
    println!(
        "{}",
        "╰────────────────────────────────────────────╯".blue()
    );
}

/// Discover markdown files in a directory
///
/// This function recursively searches for .md files in the given path.
/// If the path is a file, returns it if it's a markdown file.
/// If the path is a directory, walks the tree to find all markdown files.
///
/// Excludes common system directories: .crucible, .git, .obsidian, node_modules
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
            .filter_entry(|e| !is_excluded_dir(e.path()))
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

/// Check if a directory should be excluded from file discovery
fn is_excluded_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            name == ".crucible"
                || name == ".git"
                || name == ".obsidian"
                || name == "node_modules"
                || name == ".trash"
        })
        .unwrap_or(false)
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
async fn spawn_background_watch(config: CliConfig, pipeline: Arc<NotePipeline>) -> Result<()> {
    let kiln_path = config.kiln_path.clone();

    // Create watcher via factory (DIP pattern - depends only on FileWatcher trait)
    let mut watcher_arc = factories::create_file_watcher(&config)?;

    // Get mutable access to configure the watcher
    let watcher = Arc::get_mut(&mut watcher_arc)
        .ok_or_else(|| anyhow::anyhow!("Failed to get mutable watcher reference"))?;

    // Create event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Configure watch with markdown file filter and debouncing
    // Exclude .crucible directory (contains SurrealDB database files)
    let crucible_dir = kiln_path.join(".crucible");
    let filter = EventFilter::new()
        .with_extension("md")
        .exclude_dir(crucible_dir);
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
    info!(
        "Background watch started for chat mode on: {}",
        kiln_path.display()
    );

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
                    Ok(crucible_core::processing::ProcessingResult::Skipped)
                    | Ok(crucible_core::processing::ProcessingResult::NoChanges) => {
                        trace!("Background skipped (unchanged): {}", event.path.display());
                    }
                    Err(e) => {
                        warn!(
                            "Background reprocess failed for {}: {}",
                            event.path.display(),
                            e
                        );
                    }
                }
            }
            FileEventKind::Deleted => {
                debug!("File deleted: {}", event.path.display());
                // Could mark as deleted in DB in future
            }
            _ => {
                trace!(
                    "Ignoring event: {:?} for {}",
                    event.kind,
                    event.path.display()
                );
            }
        }
    }

    info!("Background watch stopped");
    Ok(())
}

/// Format tool call arguments for display
fn format_tool_args(args: &Option<serde_json::Value>) -> String {
    match args {
        Some(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| format!("{}={}", k, format_arg_value(v)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Format a single argument value, truncating if too long
fn format_arg_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => {
            let truncated = if s.len() > 30 {
                format!("{}...", &s[..27])
            } else {
                s.clone()
            };
            format!("\"{}\"", truncated)
        }
        other => {
            let s = other.to_string();
            if s.len() > 30 {
                format!("{}...", &s[..27])
            } else {
                s
            }
        }
    }
}
