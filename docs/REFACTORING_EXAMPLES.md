# Detailed Refactoring Examples & Code Patterns

## Issue 1.1: REPL Module - Detailed Analysis

### Current Problems

**File**: `/crates/crucible-cli/src/commands/repl/mod.rs` (1065 lines)

The module has these main responsibilities mixed together:
1. **REPL Session Loop** (lines ~150-400): Interactive read-eval-print loop
2. **Command Parsing** (lines ~400-500): Parsing `:command` syntax  
3. **Formatter Selection** (lines ~500-600): Choosing output format
4. **Tool Execution** (lines ~600-700): Running registered tools
5. **History Management** (lines ~700-800): Storing/retrieving command history
6. **UI Coordination** (lines ~800-900): Layout, rendering
7. **Statistics Tracking** (lines ~900-1065): Metrics and stats

### Why This is a Problem

```rust
// Current: Everything in one struct (1065 lines)
pub struct Repl {
    core: Arc<CrucibleCore>,
    editor: Reedline,
    tools: Arc<UnifiedToolRegistry>,     // Tool responsibility
    config: ReplConfig,
    formatter: Box<dyn OutputFormatter>,  // Formatting responsibility
    history: CommandHistory,              // History responsibility
    shutdown_tx: watch::Sender<bool>,     // Shutdown responsibility
    current_query_cancel: Option<oneshot::Sender<()>>,
    stats: ReplStats,                    // Stats responsibility
}

// Single method handles everything:
pub async fn run(&mut self) -> Result<()> {
    loop {
        // Read input
        let signal = self.editor.read_line(&prompt)?;
        
        // Parse command
        let input = Input::parse(&signal)?;
        
        // Dispatch to handler
        match input.command_type {
            CommandType::SurrealQL => {
                // Execute query, format, print
            },
            CommandType::BuiltIn => {
                // Execute tool, format, print  
            },
            CommandType::Exit => break,
        }
        
        // Update history
        self.history.add(&signal)?;
        
        // Update stats
        self.stats.record_command();
    }
}
```

### Solution: Decompose into Focused Components

```rust
// New structure: Separate concerns

// 1. InputHandler - responsibility: reading from user
pub struct InputHandler {
    editor: Reedline,
    history: CommandHistory,
}

impl InputHandler {
    pub async fn read_command(&mut self) -> Result<Signal> {
        // Only handles input reading
    }
}

// 2. CommandDispatcher - responsibility: routing commands
pub struct CommandDispatcher {
    tools: Arc<UnifiedToolRegistry>,
    core: Arc<CrucibleCore>,
}

impl CommandDispatcher {
    pub async fn dispatch(&self, input: Input) -> Result<CommandResult> {
        // Only handles routing and execution
    }
}

// 3. OutputRenderer - responsibility: formatting and printing
pub struct OutputRenderer {
    formatter: Box<dyn OutputFormatter>,
}

impl OutputRenderer {
    pub fn render(&self, result: CommandResult) -> Result<String> {
        // Only handles output formatting
    }
}

// 4. ReplSession - responsibility: orchestrating components
pub struct ReplSession {
    input: InputHandler,
    dispatcher: CommandDispatcher,
    renderer: OutputRenderer,
    stats: ReplStats,
}

impl ReplSession {
    pub async fn run(&mut self) -> Result<()> {
        loop {
            let signal = self.input.read_command().await?;
            let result = self.dispatcher.dispatch(Input::parse(signal)?).await?;
            println!("{}", self.renderer.render(result)?);
            self.stats.record_command();
        }
    }
}
```

**Benefits**:
- Each component has single responsibility
- Can test each component independently
- Easy to extend (e.g., add new formatter, new command type)
- Smaller files, easier to understand

---

## Issue 1.4: EAV Graph Ingest - Decomposition Strategy

### Current Problem

**File**: `/crates/crucible-surrealdb/src/eav_graph/ingest.rs` (6849 lines!)

Contains huge switch statement for embed processing:

```rust
fn classify_content(target: &str) -> ContentCategory {
    fn is_url(s: &str) -> bool { /* ... */ }
    fn get_extension(s: &str) -> Option<&str> { /* ... */ }
    
    if !is_url(target) {
        return match get_extension(target) {
            Some("md") => ContentCategory::Note,
            Some("png") | Some("jpg") => ContentCategory::Image,
            Some("mp4") => ContentCategory::Video,
            Some("mp3") => ContentCategory::Audio,
            Some("pdf") => ContentCategory::PDF,
            // ... 50+ more types
            _ => ContentCategory::Other,
        };
    }
    
    // Then 200+ lines for URL classification
    let target_lower = target.to_lowercase();
    if target_lower.contains("youtube.com") { /* ... */ }
    else if target_lower.contains("github.com") { /* ... */ }
    // ... many more if-else chains
}

// Plus 100+ lines of embed processing per embed type
// Total: 6849 lines in single file!
```

### Solution: Strategy Pattern

```rust
// Define trait for each concern
pub trait ContentClassifier: Send + Sync {
    fn classify(&self, target: &str) -> ContentCategory;
}

pub trait EmbedProcessor: Send + Sync {
    fn process(&self, embed: &EmbedRef, context: &ProcessContext) -> Result<EmbedData>;
    fn can_handle(&self, embed_type: &ContentCategory) -> bool;
}

pub trait WikilinkResolver: Send + Sync {
    fn resolve(&self, target: &str) -> Result<ResolvedLink>;
}

pub trait ContentValidator: Send + Sync {
    fn validate(&self, content: &str) -> Result<ValidationReport>;
}

// Implement specific strategies
pub struct FileExtensionClassifier;

impl ContentClassifier for FileExtensionClassifier {
    fn classify(&self, target: &str) -> ContentCategory {
        // Only handles file extension classification
        // ~50 lines instead of 6849
    }
}

pub struct YouTubeEmbedProcessor;

impl EmbedProcessor for YouTubeEmbedProcessor {
    fn process(&self, embed: &EmbedRef, context: &ProcessContext) -> Result<EmbedData> {
        // Only handles YouTube embeds
        // Can focus on specific logic
    }
    
    fn can_handle(&self, embed_type: &ContentCategory) -> bool {
        matches!(embed_type, ContentCategory::YouTube)
    }
}

// Main ingestor uses composition
pub struct NoteIngestor {
    classifiers: Vec<Box<dyn ContentClassifier>>,
    processors: Vec<Box<dyn EmbedProcessor>>,
    resolver: Box<dyn WikilinkResolver>,
    validator: Box<dyn ContentValidator>,
}

impl NoteIngestor {
    pub async fn ingest(&self, note: ParsedNote) -> Result<IngestionResult> {
        // Now just orchestrates using injected components
        // Can add new embed types without modifying this
        for embed in note.embeds {
            let category = self.classify(&embed)?;
            let processor = self.processors
                .iter()
                .find(|p| p.can_handle(&category))?;
            let processed = processor.process(&embed, context)?;
        }
    }
    
    fn classify(&self, embed: &EmbedRef) -> Result<ContentCategory> {
        // Try each classifier
        for classifier in &self.classifiers {
            if let Some(category) = classifier.classify(&embed.target) {
                return Ok(category);
            }
        }
        Ok(ContentCategory::Other)
    }
}
```

**Benefits**:
- Each embed type handled by dedicated processor
- New embed types added via plugins, not modifying core
- Can test each processor independently
- Classifiers are isolated from processors
- File size reduced from 6849 to <1000 lines

---

## Issue 2.1: Command Router - Extensibility Pattern

### Current Problem

**File**: `/crates/crucible-cli/src/main.rs` (lines 49-253)

```rust
// Must modify this massive match statement to add new commands
match cli.command {
    Some(Commands::Chat { query, agent, ... }) => {
        commands::chat::execute(config, agent, query, ...).await?
    }
    Some(Commands::Process { path, force, watch }) => {
        commands::process::execute(config, path, force, watch).await?
    }
    Some(Commands::Search { query, limit, ... }) => {
        commands::search::execute(config, query, limit, ...).await?
    }
    // ... 10+ more variants
    // MUST MODIFY THIS TO ADD ANY NEW COMMAND ❌
    None => {
        // REPL mode
    }
}
```

### Solution: Registry Pattern

```rust
// Create command trait (abstraction)
#[async_trait]
pub trait Command: Send + Sync {
    async fn execute(&self, config: &CliConfig, args: &[String]) -> Result<()>;
    fn name(&self) -> &str;
    fn help(&self) -> &str;
}

// Each command implements this
pub struct ChatCommand;

#[async_trait]
impl Command for ChatCommand {
    async fn execute(&self, config: &CliConfig, args: &[String]) -> Result<()> {
        let query = args.get(0).unwrap_or(&String::new());
        let agent = args.get(1);
        commands::chat::execute(config, agent.cloned(), Some(query.clone()), ...).await
    }
    
    fn name(&self) -> &str { "chat" }
    fn help(&self) -> &str { "Start chat interface" }
}

// Command registry
pub struct CommandRegistry {
    commands: HashMap<String, Arc<dyn Command>>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            commands: HashMap::new(),
        };
        
        // Register built-in commands
        registry.register("chat", Arc::new(ChatCommand));
        registry.register("search", Arc::new(SearchCommand));
        registry.register("parse", Arc::new(ParseCommand));
        // ... etc
        
        registry
    }
    
    pub fn register(&mut self, name: &str, cmd: Arc<dyn Command>) {
        self.commands.insert(name.to_string(), cmd);
    }
    
    pub async fn execute(&self, name: &str, config: &CliConfig, args: &[String]) -> Result<()> {
        let cmd = self.commands.get(name)
            .ok_or_else(|| anyhow!("Unknown command: {}", name))?;
        cmd.execute(config, args).await
    }
    
    pub fn list(&self) -> Vec<&str> {
        self.commands.keys().map(|s| s.as_str()).collect()
    }
}

// Main now uses registry (no modification needed for new commands!)
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = CliConfig::load(...)?;
    
    let registry = CommandRegistry::new();
    
    if let Some(command_name) = &cli.command_name {
        registry.execute(command_name, &config, &cli.args).await?;
    } else {
        // Default: REPL mode
        commands::repl::execute(&config).await?;
    }
    
    Ok(())
}
```

**Benefits**:
- Add new commands without modifying main.rs
- Can load commands from plugins
- Can load commands from config
- Extensible without recompilation
- Follows Open/Closed principle

---

## Issue 5.1: Output Formatting - Code Duplication

### Current Duplication

**Commands duplicating output rendering logic**:

```rust
// File 1: parse.rs (lines ~200-300)
pub async fn execute(...) -> Result<()> {
    for result in parse_results {
        match format.as_str() {
            "json" => {
                let json = serde_json::to_string(&result)?;
                println!("{}", json);
            },
            "table" => {
                let table = format_as_table(&result);
                println!("{}", table);
            },
            _ => {
                let plain = format_as_plain(&result);
                println!("{}", plain);
            }
        }
    }
}

// File 2: search.rs (lines ~300-400)
pub async fn execute(...) -> Result<()> {
    for result in search_results {
        match format.as_str() {
            "json" => {
                let json = serde_json::to_string(&result)?;
                println!("{}", json);
            },
            "table" => {
                let table = format_as_table(&result);
                println!("{}", table);
            },
            _ => {
                let plain = format_as_plain(&result);
                println!("{}", plain);
            }
        }
    }
}

// File 3: storage.rs (similar pattern)
// ... duplicated 5+ more times
```

### Solution: Centralized Formatter Registry

```rust
// output.rs - Single source of truth
pub trait OutputFormatter: Send + Sync {
    fn format(&self, data: &dyn std::any::Any) -> Result<String>;
}

pub struct JsonFormatter;
impl OutputFormatter for JsonFormatter {
    fn format(&self, data: &dyn std::any::Any) -> Result<String> {
        // ... json formatting logic
    }
}

pub struct TableFormatter;
impl OutputFormatter for TableFormatter {
    fn format(&self, data: &dyn std::any::Any) -> Result<String> {
        // ... table formatting logic
    }
}

pub struct PlainFormatter;
impl OutputFormatter for PlainFormatter {
    fn format(&self, data: &dyn std::any::Any) -> Result<String> {
        // ... plain formatting logic
    }
}

// Registry
pub struct OutputRegistry {
    formatters: HashMap<String, Arc<dyn OutputFormatter>>,
}

impl OutputRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            formatters: HashMap::new(),
        };
        registry.register("json", Arc::new(JsonFormatter));
        registry.register("table", Arc::new(TableFormatter));
        registry.register("plain", Arc::new(PlainFormatter));
        registry
    }
    
    pub fn format(&self, format: &str, data: &dyn std::any::Any) -> Result<String> {
        self.formatters
            .get(format)
            .ok_or_else(|| anyhow!("Unknown format: {}", format))?
            .format(data)
    }
}

// Now all commands use the same formatter!
pub async fn execute(...) -> Result<()> {
    let formatter = OutputRegistry::new();
    
    for result in results {
        let formatted = formatter.format(&format_string, &result)?;
        println!("{}", formatted);
    }
}
```

**Benefits**:
- Single definition of each format
- Consistent formatting across all commands
- Easy to add new formats
- Easy to test formatters independently

---

## Issue 3.2: Fat Config - Interface Segregation

### Current Problem

```rust
pub struct ApplicationConfig {
    pub enable_strict_validation: bool,         // For validators
    pub enable_auto_conflict_resolution: bool,  // For conflict resolver
    pub max_batch_size: usize,                  // For optimizer
    pub enable_change_optimization: bool,       // For optimizer
    pub enable_rollback: bool,                  // For recovery
    pub verify_after_each_change: bool,         // For validators
    pub stop_on_first_error: bool,              // For error handler
}

// Every component gets ALL of these!
pub struct ChangeApplicationSystem {
    config: ApplicationConfig,
    // Can only use specific fields, but forced to know about all
}
```

### Solution: Segregated Configs

```rust
// Each component gets only what it needs

pub struct ValidationConfig {
    pub enable_strict_validation: bool,
    pub verify_after_each_change: bool,
}

pub struct OptimizationConfig {
    pub enable_change_optimization: bool,
    pub max_batch_size: usize,
}

pub struct ConflictResolutionConfig {
    pub auto_resolve_conflicts: bool,
}

pub struct RecoveryConfig {
    pub enable_rollback: bool,
    pub stop_on_first_error: bool,
}

// Validators only know about ValidationConfig
pub trait ChangeValidator {
    fn validate(&self, changes: &[EnhancedTreeChange], config: &ValidationConfig) -> Result<()>;
}

// Optimizers only know about OptimizationConfig
pub trait ChangeOptimizer {
    fn optimize(&self, changes: Vec<EnhancedTreeChange>, config: &OptimizationConfig) 
        -> Result<Vec<EnhancedTreeChange>>;
}

// Main system composes them
pub struct ChangeApplicationSystem {
    validator: Box<dyn ChangeValidator>,
    optimizer: Box<dyn ChangeOptimizer>,
    resolver: Box<dyn ConflictResolver>,
    recovery: Box<dyn RecoveryManager>,
}
```

**Benefits**:
- Components only depend on what they need
- Clear what each component requires
- Easier to test with mocks
- Easier to understand component contracts

---

## Testing Impact

### Before (Hard to Test)
```rust
#[tokio::test]
async fn test_repl() {
    // Must set up entire REPL with all components
    let mut repl = Repl::new(core, config).await.unwrap();
    
    // Can't test input handling independently
    // Can't test command dispatch independently
    // Can't test formatting independently
    // Must test everything together!
    repl.run().await.unwrap();
}
```

### After (Easy to Test)
```rust
#[tokio::test]
async fn test_input_handler() {
    let handler = InputHandler::new(/* minimal setup */);
    let signal = handler.read_command().await.unwrap();
    assert_eq!(signal.command, "test");
}

#[tokio::test]
async fn test_command_dispatch() {
    let dispatcher = CommandDispatcher::new(core);
    let result = dispatcher.dispatch(Input::parse("SELECT * FROM notes")).await.unwrap();
    assert!(!result.is_empty());
}

#[tokio::test]
fn test_output_formatting() {
    let renderer = OutputRenderer::new();
    let output = renderer.render(result).unwrap();
    assert!(output.contains("expected content"));
}

#[tokio::test]
async fn test_repl_integration() {
    // Full integration test - tests all components together
    let session = ReplSession::new(...);
    session.run().await.unwrap();
}
```

---

## Summary: Refactoring Impact by Priority

| Issue | Impact | Effort | Files Affected |
|-------|--------|--------|-----------------|
| 1.1 REPL decomposition | High | Medium | 5 files |
| 1.4 Embed processor extraction | Very High | High | 8+ files |
| 2.1 Command registry | High | Medium | main.rs + commands |
| 5.1 Consolidated formatting | Medium | Low | 5+ command files |
| 3.2 Segregated configs | Medium | Low | 10+ files using config |
| 1.3 Change application split | Medium | High | change_application.rs |
| 3.1 Storage trait split | Low | Medium | traits/storage.rs |
| 1.2 Parser bridge decomposition | Medium | High | parser modules |

**Recommendation**: Start with 1.1, 5.1, then 2.1 for maximum quick wins.
