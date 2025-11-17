# CLI Rework Proposal: ACP-Based Chat Interface

**Date:** 2025-11-17
**Status:** Proposal
**Goal:** Rework CLI to use new pipeline architecture + Agent Client Protocol for natural language interaction

---

## Executive Summary

The current CLI needs a complete rework to:
1. **Use the new `NotePipeline`** orchestrator instead of old scattered processing code
2. **Implement ACP chat interface** instead of SurrealQL REPL
3. **Follow the refined architecture** (plaintext-first, trait-based, editor-agnostic)

**Estimated Effort:** 1-2 weeks for minimal viable chat interface

---

## Architecture Vision

### High-Level Flow

```
┌─────────────────────────────────────────────────────────┐
│         Crucible CLI (Binary: cru)                       │
│                                                          │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Main Modes                                      │   │
│  │  1. Chat (default) - ACP-based natural language │   │
│  │  2. Process - Run pipeline on kiln               │   │
│  │  3. Status - Show pipeline status                │   │
│  │  4. Config - Manage configuration                │   │
│  └─────────────────────────────────────────────────┘   │
│                                                          │
│  ┌─────────────────────────────────────────────────┐   │
│  │  Chat Mode (ACP Client)                         │   │
│  │  - Spawns external agent (claude-code, etc.)    │   │
│  │  - Enriches queries with kiln context           │   │
│  │  - Streams responses back to user               │   │
│  └──────────────────┬──────────────────────────────┘   │
│                     │                                    │
└─────────────────────┼────────────────────────────────────┘
                      │
                      ▼
            ┌──────────────────────┐
            │  External Agent      │
            │  (claude-code, etc.) │
            └──────────────────────┘

┌─────────────────────────────────────────────────────────┐
│  Core Layer (accessed by CLI for context)               │
│                                                          │
│  ┌─────────────────┐  ┌─────────────────┐              │
│  │  NotePipeline   │  │  Storage Traits │              │
│  │  (5-phase proc) │  │  (query kiln)   │              │
│  └─────────────────┘  └─────────────────┘              │
└─────────────────────────────────────────────────────────┘
```

---

## Core Principles

### 1. Chat-First Interface
**Replace:** SurrealQL REPL
**With:** Natural language chat powered by ACP

```bash
# OLD (current - being deprecated)
$ cru
> SELECT * FROM notes WHERE tags CONTAINS 'rust';

# NEW (proposed)
$ cru chat
> What notes do I have about Rust?
🔍 Searching kiln... found 5 notes
💬 Claude: Based on your notes, you have extensive documentation about...
```

### 2. Pipeline-Driven Processing
**Replace:** Old scattered processing logic
**With:** Single `NotePipeline` orchestrator

```bash
# Explicit processing command
$ cru process
📁 Scanning kiln: /Users/me/kiln
✓ Phase 1: Filtered 100 files → 3 changed
✓ Phase 2: Parsed 3 files
✓ Phase 3: Merkle diff → 12 changed blocks
✓ Phase 4: Enriched 12 blocks (embeddings + metadata)
✓ Phase 5: Stored to database
⏱  Completed in 847ms
```

### 3. Background Processing
**Pattern:** Process on startup, then stay responsive

```rust
// Main flow
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize core
    let core = initialize_core(&cli).await?;

    // Background processing (unless --no-process)
    if !cli.no_process {
        tokio::spawn(async move {
            run_pipeline_scan(core.clone()).await
        });
    }

    // Main command (don't wait for processing)
    match cli.command {
        Commands::Chat => chat_mode(core).await,
        Commands::Process => explicit_process(core).await,
        Commands::Status => show_status(core).await,
        _ => {}
    }
}
```

### 4. Trait-Based Abstractions
**All core access through traits** - no direct database coupling

```rust
// CLI depends on traits, not concrete types
pub struct ChatInterface {
    pipeline: Arc<dyn NotePipelineOrchestrator>,
    storage: Arc<dyn EnrichedNoteStore>,
    semantic_search: Arc<dyn SemanticSearchService>,
}
```

---

## Proposed CLI Commands

### Simplified Command Structure

```bash
cru [OPTIONS] [COMMAND]

Commands:
  chat      Start natural language chat (default)
  process   Run pipeline on kiln
  status    Show processing status and stats
  search    Quick semantic search
  config    Manage configuration

Options:
  --kiln <PATH>         Override kiln path
  --no-process          Skip background processing
  --agent <TYPE>        Agent to use (claude-code, gemini, codex)
  -v, --verbose         Verbose logging
```

### Command Details

#### 1. `chat` (Default Command)

**Purpose:** Natural language interface to your knowledge base

```bash
# Start chat session
$ cru chat
🔍 Loading kiln... done
✨ Ready! Ask me anything about your notes.

> What did I write about Merkle trees?
🔍 Searching... found 3 notes
💬 Claude: You have detailed notes about Merkle trees including:
   1. Architecture doc explaining the 5-phase pipeline...
   2. Research notes on Oxen AI's implementation...
   ...

> Create a summary of those notes
💭 Thinking...
📝 Creating note: summaries/merkle-trees.md
✓ Done! Created new note with summary.

> exit
Goodbye!
```

**Implementation:**
- Uses `agent-client-protocol` crate
- Spawns external agent subprocess
- Enriches prompts with semantic search results
- Handles file operations (read/write notes)

#### 2. `process`

**Purpose:** Explicitly run pipeline on kiln

```bash
# Full scan
$ cru process
📁 Scanning kiln: ~/kiln (1,243 files)
⏱  Phase 1: Quick filter... 3 changed files
⏱  Phase 2: Parsing... done
⏱  Phase 3: Merkle diff... 12 blocks changed
⏱  Phase 4: Enriching... generating embeddings
⏱  Phase 5: Storing... done
✓ Processed 3 files, 12 blocks in 1.2s

# Force reprocess all
$ cru process --force
⚠️  This will reprocess all 1,243 files. Continue? (y/N)

# Process specific file
$ cru process notes/rust.md
✓ Processed notes/rust.md in 124ms
```

#### 3. `status`

**Purpose:** Show kiln status and processing metrics

```bash
$ cru status
📊 Kiln Status

📁 Files: 1,243 markdown files
📝 Notes: 1,189 notes indexed
🧱 Blocks: 14,567 total blocks
🔮 Embeddings: 13,891 blocks embedded
🔗 Links: 3,456 wikilinks, 234 backlinks

⏱  Last processed: 2 minutes ago
✓ All files up to date

🗂  Recent activity:
  • notes/architecture.md - modified 2 min ago (processing...)
  • notes/acp-mvp.md - created 5 min ago (✓ processed)
  • notes/philosophy.md - modified 1 hour ago (✓ processed)
```

#### 4. `search`

**Purpose:** Quick semantic search without chat

```bash
$ cru search "CRDT synchronization"
🔍 Searching... found 5 results

1. notes/architecture.md
   Score: 0.89
   ...discusses CRDT sync for multiplayer editing...

2. notes/research/merkle-vs-crdt.md
   Score: 0.76
   ...comparison of Merkle trees and CRDTs...

# With snippet extraction
$ cru search "pipeline phases" --show-content
🔍 Searching... found 3 results

1. notes/architecture.md (score: 0.92)
   > The pipeline coordinates five phases:
   > 1. Quick Filter: Check file state...
   > 2. Parse: Transform markdown to AST...
```

#### 5. `config`

**Purpose:** Configuration management

```bash
# Show current config
$ cru config show
kiln.path = /Users/me/kiln
embedding.provider = fastembed
embedding.model = BAAI/bge-small-en-v1.5
agent.default = claude-code

# Initialize new config
$ cru config init
✓ Created ~/.config/crucible/config.toml

# Set values
$ cru config set kiln.path ~/Documents/my-notes
✓ Updated configuration
```

---

## Detailed Implementation Plan

### Phase 1: Core Refactoring (Week 1)

#### 1.1 Create New CLI Module Structure

```
crates/crucible-cli/src/
├── main.rs                    # Entry point
├── cli.rs                     # clap argument parsing
├── commands/
│   ├── mod.rs
│   ├── chat.rs               # NEW - ACP chat interface
│   ├── process.rs            # NEW - Pipeline processing
│   ├── status.rs             # REFACTOR - Use new traits
│   ├── search.rs             # REFACTOR - Use semantic search
│   └── config.rs             # KEEP - Already good
├── acp/
│   ├── mod.rs
│   ├── client.rs             # NEW - ACP Client implementation
│   ├── context.rs            # NEW - Context enrichment
│   └── agent.rs              # NEW - Agent spawning
├── pipeline/
│   ├── mod.rs
│   ├── processor.rs          # NEW - Background processing
│   └── watcher.rs            # NEW - File watching integration
└── core_facade.rs            # NEW - Clean interface to core

# DELETE (old, pre-pipeline code)
├── commands/repl/            # Replaced by chat
├── commands/fuzzy.rs         # Replaced by semantic search
├── commands/diff.rs          # Not needed for MVP
├── common/*.disabled         # Already disabled
```

#### 1.2 Core Facade Pattern

**Purpose:** Clean, testable interface between CLI and core

```rust
// src/core_facade.rs
use crucible_core::*;
use crucible_pipeline::*;
use std::sync::Arc;

/// Unified facade for CLI to access core functionality
pub struct CrucibleCore {
    pipeline: Arc<dyn NotePipelineOrchestrator>,
    storage: Arc<dyn EnrichedNoteStore>,
    semantic_search: Arc<dyn SemanticSearchService>,
    config: Arc<CliConfig>,
}

impl CrucibleCore {
    /// Initialize from config
    pub async fn from_config(config: CliConfig) -> Result<Self> {
        // Initialize storage
        let storage_config = SurrealDbConfig {
            path: config.database_path_str()?,
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let storage_client = SurrealClient::new(storage_config).await?;
        let storage = Arc::new(EAVGraphStore::new(storage_client.clone()));

        // Initialize pipeline dependencies
        let change_detector = Arc::new(/* ... */);
        let merkle_store = Arc::new(MerklePersistence::new(storage_client.clone()));
        let enrichment = Arc::new(DefaultEnrichmentService::new(/* ... */));

        // Create pipeline
        let pipeline = Arc::new(NotePipeline::new(
            change_detector,
            merkle_store,
            enrichment,
            storage.clone(),
        ));

        // Create semantic search service
        let semantic_search = Arc::new(/* ... */);

        Ok(Self {
            pipeline,
            storage,
            semantic_search,
            config: Arc::new(config),
        })
    }

    /// Process a single file through pipeline
    pub async fn process_file(&self, path: &Path) -> Result<ProcessingResult> {
        self.pipeline.process(path).await
    }

    /// Process entire kiln
    pub async fn process_kiln(&self) -> Result<PipelineMetrics> {
        let kiln_path = &self.config.kiln.path;
        let mut metrics = PipelineMetrics::default();

        // Walk kiln directory
        for entry in walkdir::WalkDir::new(kiln_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension() == Some("md".as_ref()))
        {
            let result = self.pipeline.process(entry.path()).await?;
            metrics.merge(result);
        }

        Ok(metrics)
    }

    /// Semantic search
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        self.semantic_search.search(query, limit).await
    }

    /// Get storage statistics
    pub async fn get_stats(&self) -> Result<KilnStats> {
        self.storage.get_statistics().await
    }
}
```

#### 1.3 Update clap CLI Structure

```rust
// src/cli.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cru")]
#[command(about = "Crucible - AI-powered knowledge management")]
#[command(version)]
pub struct Cli {
    /// Subcommand (defaults to chat if not provided)
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Kiln directory path
    #[arg(long, global = true, env = "CRUCIBLE_KILN_PATH")]
    pub kiln: Option<PathBuf>,

    /// Skip background processing
    #[arg(long, global = true)]
    pub no_process: bool,

    /// Config file path
    #[arg(short = 'C', long, global = true)]
    pub config: Option<PathBuf>,

    /// Verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start natural language chat (default)
    Chat {
        /// Agent to use (claude-code, gemini, codex)
        #[arg(long, default_value = "claude-code")]
        agent: String,

        /// Start with a query (skip interactive mode)
        query: Option<String>,
    },

    /// Process files through pipeline
    Process {
        /// Specific file or directory to process
        path: Option<PathBuf>,

        /// Force reprocess all files
        #[arg(long)]
        force: bool,

        /// Watch for changes and auto-process
        #[arg(long)]
        watch: bool,
    },

    /// Show kiln status and statistics
    Status {
        /// Show detailed metrics
        #[arg(long)]
        detailed: bool,
    },

    /// Semantic search
    Search {
        /// Search query
        query: String,

        /// Number of results
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,

        /// Show content snippets
        #[arg(long)]
        show_content: bool,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Show current configuration
    Show,

    /// Initialize new config file
    Init {
        /// Force overwrite existing config
        #[arg(short = 'F', long)]
        force: bool,
    },

    /// Set configuration value
    Set {
        /// Key to set (e.g., 'kiln.path')
        key: String,

        /// Value to set
        value: String,
    },
}
```

### Phase 2: ACP Implementation (Week 1-2)

#### 2.1 ACP Client Implementation

```rust
// src/acp/client.rs
use agent_client_protocol::*;
use std::process::Stdio;
use tokio::process::Command;

pub struct CrucibleAcpClient {
    core: Arc<CrucibleCore>,
}

#[async_trait]
impl Client for CrucibleAcpClient {
    /// Agent requests to read a file
    async fn fs_read_text_file(&mut self, path: &str) -> Result<String> {
        // Read from kiln via storage
        let note_path = self.core.config.kiln.path.join(path);
        tokio::fs::read_to_string(note_path).await
            .context("Failed to read file")
    }

    /// Agent requests to write a file (optional for MVP)
    async fn fs_write_text_file(&mut self, path: &str, content: &str) -> Result<()> {
        let note_path = self.core.config.kiln.path.join(path);
        tokio::fs::write(note_path, content).await?;

        // Trigger pipeline processing
        self.core.process_file(&note_path).await?;

        Ok(())
    }

    /// Agent sends updates (streaming)
    async fn session_update(&mut self, update: SessionUpdate) -> Result<()> {
        match update {
            SessionUpdate::MessageChunk { content } => {
                print!("{}", content);
                std::io::stdout().flush()?;
            }
            SessionUpdate::Thought { content } => {
                println!("💭 {}", content);
            }
            SessionUpdate::ToolCall { tool, args } => {
                println!("🔧 {} ({})", tool, args);
            }
            SessionUpdate::Done => {
                println!("\n");
            }
            _ => {}
        }
        Ok(())
    }

    /// Permission request (auto-approve for chat-only MVP)
    async fn request_permission(&mut self, _action: &str) -> Result<bool> {
        // For MVP: auto-approve read operations
        // Later: show UI prompt for write operations
        Ok(true)
    }

    /// Terminal operations (not needed for chat MVP)
    async fn terminal_create(&mut self, _cmd: &str) -> Result<TerminalId> {
        Err(anyhow::anyhow!("Terminal not supported in chat mode"))
    }
}

pub struct AcpConnection {
    connection: ClientSideConnection<CrucibleAcpClient>,
    session_id: Option<String>,
}

impl AcpConnection {
    /// Spawn agent and create connection
    pub async fn new(agent_type: &str, core: Arc<CrucibleCore>) -> Result<Self> {
        // Spawn agent subprocess
        let mut child = Command::new(agent_type)
            .arg("--acp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        // Create client
        let client = CrucibleAcpClient { core };

        // Create connection
        let connection = ClientSideConnection::new(
            child.stdin.take().unwrap(),
            child.stdout.take().unwrap(),
            client,
        );

        // Initialize
        connection.initialize(InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client_info: ClientInfo {
                name: "Crucible".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: ClientCapabilities {
                file_system: Some(FileSystemCapabilities {
                    read_text_file: true,
                    write_text_file: true,
                }),
                terminal: None,
            },
        }).await?;

        Ok(Self {
            connection,
            session_id: None,
        })
    }

    /// Start new session
    pub async fn start_session(&mut self) -> Result<()> {
        let response = self.connection.new_session(NewSessionParams {
            cwd: std::env::current_dir()?,
            mcp_servers: vec![],
        }).await?;

        self.session_id = Some(response.session_id);
        Ok(())
    }

    /// Send message with context enrichment
    pub async fn send_message(&mut self, query: &str) -> Result<()> {
        let enriched = self.enrich_with_context(query).await?;

        self.connection.prompt(PromptParams {
            session_id: self.session_id.clone().unwrap(),
            prompt: vec![Content::Text { text: enriched }],
        }).await?;

        Ok(())
    }

    /// Enrich query with semantic search results
    async fn enrich_with_context(&self, query: &str) -> Result<String> {
        // Get reference to core through client
        let client = self.connection.client();

        // Semantic search
        let results = client.core.search(query, 5).await?;

        // Format context
        let context = results.iter()
            .map(|r| format!("## {}\n\n{}\n", r.title, r.snippet))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r#"# Context from Knowledge Base

{}

---

# User Query

{}"#,
            context, query
        ))
    }
}
```

#### 2.2 Chat Command Implementation

```rust
// src/commands/chat.rs
use crate::acp::*;
use crate::core_facade::*;

pub async fn execute(core: Arc<CrucibleCore>, agent: String, query: Option<String>) -> Result<()> {
    println!("🚀 Starting chat with {}...", agent);

    // Create ACP connection
    let mut connection = AcpConnection::new(&agent, core.clone()).await?;
    connection.start_session().await?;

    println!("✨ Ready! Type your questions (or 'exit' to quit)\n");

    // Handle one-shot query
    if let Some(q) = query {
        println!("> {}", q);
        connection.send_message(&q).await?;
        return Ok(());
    }

    // Interactive loop
    let mut rl = rustyline::Editor::<()>::new()?;
    loop {
        match rl.readline("> ") {
            Ok(line) => {
                let input = line.trim();

                if input.is_empty() {
                    continue;
                }

                if input == "exit" || input == "quit" {
                    println!("Goodbye!");
                    break;
                }

                rl.add_history_entry(input);

                // Send to agent with enrichment
                if let Err(e) = connection.send_message(input).await {
                    eprintln!("❌ Error: {}", e);
                }
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("\nUse 'exit' to quit");
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
```

### Phase 3: Pipeline Integration (Week 2)

#### 3.1 Process Command

```rust
// src/commands/process.rs
use crate::core_facade::*;
use indicatif::{ProgressBar, ProgressStyle};

pub async fn execute(
    core: Arc<CrucibleCore>,
    path: Option<PathBuf>,
    force: bool,
    watch: bool,
) -> Result<()> {
    if watch {
        return execute_watch_mode(core).await;
    }

    if let Some(file_path) = path {
        // Process single file
        execute_single_file(core, &file_path, force).await
    } else {
        // Process entire kiln
        execute_full_scan(core, force).await
    }
}

async fn execute_single_file(
    core: Arc<CrucibleCore>,
    path: &Path,
    force: bool,
) -> Result<()> {
    println!("📄 Processing {}...", path.display());

    let start = Instant::now();
    let result = core.process_file(path).await?;
    let elapsed = start.elapsed();

    match result {
        ProcessingResult::Success { changed_blocks, .. } => {
            println!("✓ Processed {} blocks in {:?}", changed_blocks, elapsed);
        }
        ProcessingResult::Skipped => {
            println!("⏭  Skipped (no changes)");
        }
        ProcessingResult::NoChanges => {
            println!("⏭  No content changes detected");
        }
    }

    Ok(())
}

async fn execute_full_scan(core: Arc<CrucibleCore>, force: bool) -> Result<()> {
    let kiln_path = &core.config.kiln.path;
    println!("📁 Scanning kiln: {}", kiln_path.display());

    // Count files first
    let files: Vec<_> = walkdir::WalkDir::new(kiln_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some("md".as_ref()))
        .collect();

    println!("   Found {} markdown files\n", files.len());

    // Progress bar
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {pos}/{len} files ({eta})")
            .unwrap()
            .progress_chars("=>-")
    );

    // Process files
    let mut metrics = PipelineMetrics::default();
    for entry in files {
        let result = core.process_file(entry.path()).await?;
        metrics.merge(result);
        pb.inc(1);
    }

    pb.finish_with_message("Done!");

    // Summary
    println!("\n✓ Pipeline Complete\n");
    println!("📊 Summary:");
    println!("   Files processed: {}", metrics.files_processed);
    println!("   Blocks changed: {}", metrics.blocks_changed);
    println!("   Embeddings generated: {}", metrics.embeddings_generated);
    println!("   Time elapsed: {:?}", metrics.total_time);

    Ok(())
}

async fn execute_watch_mode(core: Arc<CrucibleCore>) -> Result<()> {
    use notify::{Watcher, RecursiveMode, Event};

    println!("👀 Watching for file changes... (Ctrl+C to stop)");

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;

    watcher.watch(&core.config.kiln.path, RecursiveMode::Recursive)?;

    for event in rx {
        match event {
            Ok(Event { paths, .. }) => {
                for path in paths {
                    if path.extension() == Some("md".as_ref()) {
                        println!("\n📝 Change detected: {}", path.display());
                        if let Err(e) = core.process_file(&path).await {
                            eprintln!("❌ Error processing: {}", e);
                        }
                    }
                }
            }
            Err(e) => eprintln!("Watch error: {}", e),
        }
    }

    Ok(())
}
```

---

## Dependencies to Add

```toml
# crates/crucible-cli/Cargo.toml

[dependencies]
# ACP integration
agent-client-protocol = "0.6"  # Check crates.io for latest

# Interactive input
rustyline = "13.0"

# Progress indicators
indicatif = "0.17"

# File watching
notify = { workspace = true }
walkdir = "2.4"

# Keep existing
crucible-core = { path = "../crucible-core" }
crucible-pipeline = { path = "../crucible-pipeline" }
crucible-surrealdb = { path = "../crucible-surrealdb" }
clap = { workspace = true, features = ["derive", "env"] }
tokio = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
```

---

## Migration Strategy

### Week 1: Foundation
1. ✅ Create new module structure
2. ✅ Implement `CrucibleCore` facade
3. ✅ Update clap CLI structure
4. ✅ Add ACP client implementation
5. ✅ Implement chat command (basic)

### Week 2: Complete Features
6. ✅ Implement process command
7. ✅ Implement status command (refactored)
8. ✅ Implement search command (semantic)
9. ✅ Add context enrichment to chat
10. ✅ Testing and refinement

### Week 3: Cleanup (Optional)
11. ✅ Remove old REPL code
12. ✅ Remove disabled features
13. ✅ Update documentation
14. ✅ Add integration tests

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_core_facade_initialization() {
        let config = CliConfig::default();
        let core = CrucibleCore::from_config(config).await.unwrap();
        assert!(core.pipeline.is_ok());
    }

    #[tokio::test]
    async fn test_semantic_search() {
        let core = create_test_core().await;
        let results = core.search("test query", 5).await.unwrap();
        assert!(results.len() <= 5);
    }
}
```

### Integration Tests

```rust
// tests/cli_integration.rs
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_status_command() {
    Command::cargo_bin("cru")
        .unwrap()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("Kiln Status"));
}

#[test]
fn test_process_command() {
    let temp_kiln = create_test_kiln();

    Command::cargo_bin("cru")
        .unwrap()
        .arg("--kiln")
        .arg(temp_kiln.path())
        .arg("process")
        .assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));
}
```

---

## Success Criteria

### MVP (End of Week 2)
- ✅ `cru chat` works with claude-code
- ✅ Context enrichment from semantic search
- ✅ `cru process` runs full pipeline
- ✅ `cru status` shows accurate metrics
- ✅ `cru search` returns semantic results
- ✅ All commands use `NotePipeline`
- ✅ No database lock errors
- ✅ Background processing works

### Quality Gates
- ✅ No panics in normal operation
- ✅ Helpful error messages
- ✅ <2s startup time
- ✅ <1s semantic search
- ✅ Responsive during processing

---

## Open Questions

1. **Agent Discovery:** How does user install/configure agents? (Document: "install claude-code first")
2. **Context Size:** How many search results to include? (Start with 5, make configurable)
3. **Streaming UX:** How to show thinking vs response? (Use emoji prefixes: 💭 vs 💬)
4. **Permissions:** Auto-approve everything or prompt for writes? (Auto-approve for MVP)
5. **Session Persistence:** Save chat history? (No for MVP, add later)

---

## References

- **ACP-MVP.md** - ACP integration examples and patterns
- **ARCHITECTURE.md** - Overall system architecture
- **PHILOSOPHY.md** - Design principles and user stories
- **crucible-pipeline** - New 5-phase pipeline implementation
- **agent-client-protocol** crate - Official ACP Rust implementation

---

**Next Steps:**
1. Review and approve this proposal
2. Start Week 1 implementation
3. Daily check-ins on progress
4. Demo at end of Week 1
5. Iterate based on feedback
