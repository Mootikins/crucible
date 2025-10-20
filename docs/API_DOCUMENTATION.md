# Crucible API Documentation

> **Status**: Active API Documentation
> **Version**: 1.0.0
> **Date**: 2025-10-20
> **Purpose**: Complete API reference for the Crucible service architecture

## Table of Contents

- [Core APIs](#core-apis)
  - [crucible-core](#crucible-core)
  - [crucible-config](#crucible-config)
- [Service Layer APIs](#service-layer-apis)
  - [crucible-services](#crucible-services)
  - [crucible-tools](#crucible-tools)
  - [crucible-rune](#crucible-rune)
  - [crucible-rune-macros](#crucible-rune-macros)
- [Storage APIs](#storage-apis)
  - [crucible-surrealdb](#crucible-surrealdb)
  - [crucible-llm](#crucible-llm)
- [Interface APIs](#interface-apis)
  - [crucible-cli](#crucible-cli)
  - [crucible-tauri](#crucible-tauri)

## Core APIs

### crucible-core

Core business logic and domain models for the Crucible system.

#### Types

##### Document
```rust
/// Represents a document in the knowledge system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: String,
    pub content: String,
    pub path: PathBuf,
    pub metadata: DocumentMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub author: Option<String>,
    pub word_count: usize,
    pub reading_time_minutes: u32,
    pub frontmatter: serde_json::Value,
}
```

##### Agent
```rust
/// AI agent definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub config: AgentConfig,
    pub status: AgentStatus,
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub model: String,
    pub tools: Vec<String>,
    pub max_tokens: usize,
    pub temperature: f64,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    Idle,
    Processing,
    Error,
}
```

##### Result Types
```rust
/// Standardized result type
pub type Result<T> = std::result::Result<T, Error>;

/// Error type
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Invalid document format: {0}")]
    InvalidFormat(String),

    #[error("Database error: {0}")]
    Database(#[from] surrealdb::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),
}
```

#### Core Operations

##### Document Management
```rust
/// Document service
pub struct DocumentService {
    db: Surreal<Db>,
    index: SearchIndex,
}

impl DocumentService {
    /// Create a new document
    pub async fn create(&self, input: CreateDocumentInput) -> Result<Document> {
        let doc = Document {
            id: uuid::Uuid::new_v4().to_string(),
            title: input.title,
            content: input.content,
            path: input.path,
            metadata: DocumentMetadata::from_frontmatter(&input.frontmatter),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        // Store in database
        self.db.create(("document", &doc.id)).content(doc.clone()).await?;

        // Index for search
        self.index.index_document(&doc).await?;

        Ok(doc)
    }

    /// Update a document
    pub async fn update(&self, id: &str, input: UpdateDocumentInput) -> Result<Document> {
        let mut doc = self.get_document(id).await?;

        doc.title = input.title.unwrap_or(doc.title);
        doc.content = input.content.unwrap_or(doc.content);
        doc.metadata = input.metadata.unwrap_or(doc.metadata);
        doc.updated_at = Utc::now();

        // Update database and index
        self.db.update(("document", id)).content(&doc).await?;
        self.index.update_document(&doc).await?;

        Ok(doc)
    }

    /// Get a document by ID
    pub async fn get_document(&self, id: &str) -> Result<Document> {
        self.db.select(("document", id)).await
            .map_err(|_| Error::DocumentNotFound(id.to_string()))
    }

    /// Search documents
    pub async fn search(&self, query: &str) -> Result<Vec<Document>> {
        let results = self.index.search(query).await?;
        let mut documents = Vec::new();

        for result in results {
            let doc = self.get_document(&result.doc_id).await?;
            documents.push(doc);
        }

        Ok(documents)
    }
}
```

##### Agent System
```rust
/// Agent service
pub struct AgentService {
    agents: HashMap<String, Agent>,
    llm: Arc<dyn LLMProvider>,
}

impl AgentService {
    /// Execute an agent task
    pub async fn execute(&self, agent_id: &str, task: AgentTask) -> Result<AgentResult> {
        let agent = self.agents.get(agent_id)
            .ok_or_else(|| Error::AgentNotFound(agent_id.to_string()))?;

        match agent.status {
            AgentStatus::Idle => {
                // Start processing
                let result = self.process_task(agent, task).await?;

                // Update status
                self.update_agent_status(agent_id, AgentStatus::Idle);

                Ok(result)
            }
            AgentStatus::Processing => Err(Error::AgentBusy(agent_id.to_string())),
        }
    }

    /// Process a task with an agent
    async fn process_task(&self, agent: &Agent, task: AgentTask) -> Result<AgentResult> {
        // Use agent's tools to process task
        let mut results = Vec::new();

        for tool_id in &agent.config.tools {
            let tool_result = self.execute_tool(tool_id, &task).await?;
            results.push(tool_result);
        }

        // Generate response using LLM
        let response = self.llm.generate_response(&task, &results).await?;

        Ok(AgentResult {
            success: true,
            response,
            tools_used: agent.config.tools.clone(),
            duration: task.duration,
        })
    }
}
```

### crucible-config

Configuration management for the entire Crucible system.

#### Configuration Types

##### Application Config
```rust
/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub services: ServiceConfig,
    pub tools: ToolConfig,
    pub ui: UIConfig,
    pub logging: LoggingConfig,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub surrealdb: SurrealDBConfig,
    pub duckdb: DuckDBConfig,
    pub storage_path: PathBuf,
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub enabled_services: Vec<String>,
    pub search: SearchServiceConfig,
    pub agent: AgentServiceConfig,
    pub hot_reload: HotReloadConfig,
}
```

##### Configuration Sources
```rust
/// Configuration source trait
pub trait ConfigSource: Send + Sync {
    async fn load(&self) -> Result<serde_json::Value>;
    async fn watch(&self, callback: Box<dyn ConfigWatcher>) -> Result<()>;
}

/// File configuration source
pub struct FileConfigSource {
    path: PathBuf,
    watch: bool,
}

impl FileConfigSource {
    pub fn new(path: PathBuf, watch: bool) -> Self {
        Self { path, watch }
    }

    pub async fn load(&self) -> Result<serde_json::Value> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let config: serde_json::Value = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
```

#### Configuration Manager
```rust
/// Configuration manager
pub struct ConfigManager {
    sources: Vec<Box<dyn ConfigSource>>,
    cache: Arc<RwLock<AppConfig>>,
    watchers: Vec<Box<dyn ConfigWatcher>>,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            cache: Arc::new(RwLock::new(AppConfig::default())),
            watchers: Vec::new(),
        }
    }

    /// Add a configuration source
    pub fn add_source(&mut self, source: Box<dyn ConfigSource>) {
        self.sources.push(source);
    }

    /// Load configuration from all sources
    pub async fn load(&self) -> Result<AppConfig> {
        let mut merged = serde_json::Map::new();

        for source in &self.sources {
            let config = source.load().await?;
            merge_config(&mut merged, config);
        }

        let config: AppConfig = serde_json::from_value(serde_json::Value::Object(merged))?;
        *self.cache.write().await = config.clone();

        Ok(config)
    }

    /// Watch for configuration changes
    pub async fn watch(&self) -> Result<()> {
        for source in &self.sources {
            let callback = Box::new(self.clone());
            source.watch(callback).await?;
        }
        Ok(())
    }

    /// Get current configuration
    pub async fn get(&self) -> AppConfig {
        self.cache.read().await.clone()
    }
}
```

## Service Layer APIs

### crucible-services

Service abstraction layer providing search, indexing, and tool management.

#### Service Registry
```rust
/// Service registry
pub struct ServiceRegistry {
    services: HashMap<String, Box<dyn Service>>,
    config: ServiceConfig,
}

impl ServiceRegistry {
    /// Create a new service registry
    pub fn new(config: ServiceConfig) -> Self {
        Self {
            services: HashMap::new(),
            config,
        }
    }

    /// Register a service
    pub fn register<T>(&mut self, name: &str, service: T)
    where
        T: Service + 'static,
    {
        self.services.insert(name.to_string(), Box::new(service));
    }

    /// Get a service by name
    pub fn get<T>(&self, name: &str) -> Result<&T, ServiceError>
    where
        T: Service + 'static,
    {
        self.services.get(name)
            .and_then(|s| s.as_any().downcast_ref())
            .ok_or(ServiceError::NotFound(name.to_string()))
    }

    /// Start all services
    pub async fn start_all(&mut self) -> Result<()> {
        for (name, service) in &mut self.services {
            if service.start().await.is_err() {
                tracing::warn!("Failed to start service: {}", name);
            }
        }
        Ok(())
    }

    /// Stop all services
    pub async fn stop_all(&self) -> Result<()> {
        for (name, service) in &self.services {
            if service.stop().await.is_err() {
                tracing::warn!("Failed to stop service: {}", name);
            }
        }
        Ok(())
    }
}
```

#### Service Trait
```rust
/// Service trait
#[async_trait]
pub trait Service: Send + Sync + Any {
    /// Start the service
    async fn start(&mut self) -> Result<()>;

    /// Stop the service
    async fn stop(&self) -> Result<()>;

    /// Check if service is running
    fn is_running(&self) -> bool;

    /// Get service status
    fn status(&self) -> ServiceStatus;

    /// Get service info
    fn info(&self) -> ServiceInfo;

    /// Cast to Any
    fn as_any(&self) -> &dyn Any;
}

/// Service status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

/// Service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub metrics: ServiceMetrics,
}
```

#### Search Service
```rust
/// Search service
pub struct SearchService {
    index: Arc<SearchIndex>,
    config: SearchConfig,
}

impl SearchService {
    /// Search for documents
    pub async fn search(&self, query: &str, options: SearchOptions) -> Result<SearchResults> {
        let results = self.index.search_with_options(query, options).await?;

        Ok(SearchResults {
            total: results.total,
            results: results.hits,
            query: query.to_string(),
            options,
            duration: results.duration,
        })
    }

    /// Index a document
    pub async fn index_document(&self, doc: &Document) -> Result<()> {
        self.index.index(doc).await
    }

    /// Get search suggestions
    pub async fn suggest(&self, query: &str) -> Result<Vec<String>> {
        self.index.suggest(query).await
    }
}

/// Search options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub filters: Vec<SearchFilter>,
    pub sort: Option<SortOrder>,
    pub fields: Option<Vec<String>>,
}

/// Search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub total: usize,
    pub results: Vec<SearchHit>,
    pub query: String,
    pub options: SearchOptions,
    pub duration: Duration,
}
```

#### Tool Service
```rust
/// Tool service
pub struct ToolService {
    static_tools: ToolRegistry,
    dynamic_tools: DynamicToolRegistry,
    config: ToolConfig,
}

impl ToolService {
    /// Execute a tool
    pub async fn execute_tool(&self, name: &str, params: serde_json::Value) -> Result<ToolResult> {
        // Check static tools first
        if let Some(tool) = self.static_tools.get_tool(name) {
            return self.execute_static_tool(tool, &params).await;
        }

        // Check dynamic tools
        if let Some(tool) = self.dynamic_tools.get_tool(name).await? {
            return self.execute_dynamic_tool(tool, &params).await;
        }

        Err(ToolError::ToolNotFound(name.to_string()))
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let mut tools = Vec::new();

        // Add static tools
        tools.extend(self.static_tools.list_tools());

        // Add dynamic tools
        tools.extend(self.dynamic_tools.list_tools().await?);

        Ok(tools)
    }

    /// Register a new dynamic tool
    pub async fn register_tool(&self, tool: DynamicTool) -> Result<()> {
        self.dynamic_tools.register_tool(tool).await
    }
}
```

### crucible-tools

Static system tools for knowledge management operations.

#### Tool Registry
```rust
/// Tool registry for static tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new tool registry
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    pub fn register<T>(&mut self, name: &str, tool: T)
    where
        T: Tool + 'static,
    {
        self.tools.insert(name.to_string(), Box::new(tool));
    }

    /// Get a tool by name
    pub fn get_tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all tools
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools.values()
            .map(|t| t.info())
            .collect()
    }
}
```

#### Tool Trait
```rust
/// Tool trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get tool information
    fn info(&self) -> ToolInfo;

    /// Execute the tool
    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError>;

    /// Validate parameters
    fn validate_params(&self, params: &serde_json::Value) -> Result<(), ToolError> {
        let schema = self.info().schema;
        let validation = jsonschema::validator(&schema);

        if validation.validate(params).is_err() {
            Err(ToolError::InvalidParameters)
        } else {
            Ok(())
        }
    }
}

/// Tool information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub category: String,
    pub version: String,
    pub author: String,
    pub schema: serde_json::Value,
    pub tags: Vec<String>,
}
```

#### Built-in Tools

##### Search Tool
```rust
/// Search tool implementation
pub struct SearchTool {
    service: Arc<SearchService>,
}

#[async_trait]
impl Tool for SearchTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "search".to_string(),
            description: "Search documents by query".to_string(),
            category: "search".to_string(),
            version: "1.0.0".to_string(),
            author: "Crucible Team".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "limit": {"type": "integer", "description": "Maximum results"},
                    "filter": {"type": "object", "description": "Search filters"}
                },
                "required": ["query"]
            }),
            tags: vec!["search", "document".to_string()],
        }
    }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let query = params["query"].as_str()
            .ok_or_else(|| ToolError::InvalidParameters)?;

        let options = SearchOptions {
            limit: params["limit"].as_u64().map(|n| n as usize),
            offset: None,
            filters: vec![],
            sort: None,
            fields: None,
        };

        let results = self.service.search(query, options).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(serde_json::json!({
            "results": results.results,
            "total": results.total,
            "query": query
        }))
    }
}
```

##### Metadata Tool
```rust
/// Metadata extraction tool
pub struct MetadataTool {
    extractor: Arc<MetadataExtractor>,
}

#[async_trait]
impl Tool for MetadataTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "extract_metadata".to_string(),
            description: "Extract metadata from documents".to_string(),
            category: "metadata".to_string(),
            version: "1.0.0".to_string(),
            author: "Crucible Team".to_string(),
            schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Document path"}
                },
                "required": ["path"]
            }),
            tags: vec!["metadata", "extraction"],
        }
    }

    async fn execute(&self, params: serde_json::Value) -> Result<serde_json::Value, ToolError> {
        let path = params["path"].as_str()
            .ok_or_else(|| ToolError::InvalidParameters)?;

        let metadata = self.extractor.extract(path).await
            .map_err(|e| ToolError::Execution(e.to_string()))?;

        Ok(serde_json::json!({
            "metadata": metadata,
            "path": path
        }))
    }
}
```

### crucible-rune

Dynamic tool execution with hot-reload capabilities.

#### Rune Runtime
```rust
/// Rune runtime for dynamic tool execution
pub struct RuneRuntime {
    vm: Arc<RuneVm>,
    hot_reloader: HotReloader,
    config: RuneConfig,
}

impl RuneRuntime {
    /// Create a new Rune runtime
    pub fn new(config: RuneConfig) -> Self {
        let vm = Arc::new(RuneVm::new());

        Self {
            vm,
            hot_reloader: HotReloader::new(&config),
            config,
        }
    }

    /// Load a Rune script
    pub async fn load_script(&mut self, path: &Path) -> Result<ScriptHandle> {
        let source = tokio::fs::read_to_string(path).await?;
        let handle = self.vm.load_script(&source, path).await?;

        // Register for hot reload if enabled
        if self.config.hot_reload {
            self.hot_reloader.watch_script(path, handle.clone()).await?;
        }

        Ok(handle)
    }

    /// Execute a Rune script
    pub async fn execute_script(&self, handle: &ScriptHandle, args: Vec<Value>) -> Result<Value> {
        self.vm.execute_script(handle, args).await
    }

    /// Hot reload a script
    pub async fn reload_script(&mut self, path: &Path) -> Result<ScriptHandle> {
        let handle = self.hot_reloader.reload_script(path).await?;
        Ok(handle)
    }

    /// Get list of loaded scripts
    pub async fn list_scripts(&self) -> Vec<ScriptInfo> {
        self.vm.list_scripts()
    }
}
```

#### Hot Reloader
```rust
/// Hot reloader for Rune scripts
pub struct HotReloader {
    watcher: notify::RecommendedWatcher,
    scripts: HashMap<PathBuf, ScriptHandle>,
}

impl HotReloader {
    /// Watch a directory for script changes
    pub fn watch_directory(&mut self, path: &Path) -> Result<()> {
        let (tx, mut rx) = mpsc::channel();
        let watcher = notify::RecommendedWatcher::new(tx, Default::default())?;

        watcher.watch(path, RecursiveMode::Recursive)?;

        // Start watching task
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Ok(event) = event {
                    Self::handle_file_event(event).await;
                }
            }
        });

        Ok(())
    }

    /// Handle file change events
    async fn handle_file_event(event: notify::Event) {
        for path in event.paths {
            if path.extension().and_then(|s| s.to_str()) == Some("rn") {
                // Reload script
                if let Some(handle) = self.scripts.get(&path) {
                    match self.reload_script(&path).await {
                        Ok(new_handle) => {
                            self.scripts.insert(path, new_handle);
                            tracing::info!("Reloaded script: {:?}", path);
                        }
                        Err(e) => {
                            tracing::error!("Failed to reload script {:?}: {}", path, e);
                        }
                    }
                }
            }
        }
    }
}
```

#### Rune Integration
```rust
/// Integration services for Rune scripts
pub struct RuneIntegration {
    runtime: Arc<RuneRuntime>,
    services: Arc<crucible_services::ServiceRegistry>,
}

impl RuneIntegration {
    /// Create a new Rune integration
    pub fn new(runtime: Arc<RuneRuntime>, services: Arc<crucible_services::ServiceRegistry>) -> Self {
        Self { runtime, services }
    }

    /// Get a service from Rune script
    pub async fn get_service<T>(&self, name: &str) -> Result<T>
    where
        T: Service + 'static,
    {
        self.services.get(name)
    }

    /// Execute a tool from Rune script
    pub async fn execute_tool(&self, tool_name: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let tool_service = self.services.get::<dyn ToolService>("tools").unwrap();
        tool_service.execute_tool(tool_name, params).await
    }

    /// Database access from Rune script
    pub async fn get_database(&self) -> Result<Arc<Surreal<Db>>> {
        let db_service = self.services.get::<crucible_surrealdb::SurrealService>("database").unwrap();
        Ok(db_service.get_client())
    }
}
```

### crucible-rune-macros

Procedural macros for compile-time tool generation.

#### Rune Tool Macro
```rust
/// Rune tool attribute macro
#[proc_macro_attribute]
pub fn rune_tool(
    args: TokenStream,
    input: TokenStream,
) -> TokenStream {
    let tool_func = parse_macro_input!(input as ItemFn);
    let attrs = parse_macro_input!(args as AttributeArgs);

    // Extract tool attributes
    let mut description = None;
    let mut category = None;
    let mut tags = Vec::new();
    let mut async_tool = false;

    for attr in attrs {
        match attr {
            NestedMeta::Meta(meta) => {
                match meta {
                    Meta::NameValue(name_value) => {
                        match name_value.path.get_ident() {
                            Some(ident) if ident == "desc" || ident == "description" => {
                                description = Some(name_value.value.clone());
                            }
                            Some(ident) if ident == "category" => {
                                category = Some(name_value.value.clone());
                            }
                            _ => {}
                        }
                    }
                    Meta::List(list) => {
                        if list.path.get_ident() == Some(&Ident::new("tags", list.span())) {
                            tags = extract_tags(list);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Generate tool metadata
    let tool_info = generate_tool_info(&tool_func, description, category, tags);

    // Generate wrapper function
    let wrapper = generate_wrapper_function(&tool_func, async_tool, &tool_info);

    // Generate registration code
    let registration = generate_registration_code(&tool_info);

    // Combine all generated code
    quote! {
        #tool_func
        #wrapper
        #registration
    }
    .into()
}
```

#### Schema Generation
```rust
/// Generate JSON schema for tool parameters
pub fn generate_schema(input: &ItemFn) -> Result<serde_json::Value> {
    let mut schema = json!({
        "type": "object",
        "properties": {},
        "required": []
    });

    // Extract function parameters and generate schema
    for param in &input.sig.inputs {
        if let FnArg::Typed(pat_type) = param {
            if let Pat::Ident(ident) = &pat_type.pat {
                let param_name = &ident.ident;
                let param_type = extract_type(&pat_type.ty)?;

                schema["properties"][param_name] = param_type.to_schema();

                // Check if parameter is required
                if !is_optional_type(&pat_type.ty) {
                    schema["required"].push(param_name.to_string());
                }
            }
        }
    }

    Ok(schema)
}

/// Type to JSON schema conversion
trait ToSchema {
    fn to_schema(&self) -> serde_json::Value;
}

impl ToSchema for Type {
    fn to_schema(&self) -> serde_json::Value {
        match self {
            Type::Path(path) => {
                let ident = &path.path.segments.last().unwrap().ident;
                match ident.to_string().as_str() {
                    "String" => json!({"type": "string"}),
                    "i32" | "i64" | "u32" | "u64" | "f64" => json!({"type": "number"}),
                    "bool" => json!({"type": "boolean"}),
                    "Option" => json!({"type": ["null", "string"]}),
                    _ => json!({"type": "object"}),
                }
            }
            _ => json!({"type": "object"}),
        }
    }
}
```

## Storage APIs

### crucible-surrealdb

SurrealDB integration with query builders and migrations.

#### Database Service
```rust
/// SurrealDB service
pub struct SurrealService {
    client: Surreal<Db>,
    config: SurrealDBConfig,
}

impl SurrealService {
    /// Create a new SurrealDB service
    pub async fn new(config: SurrealDBConfig) -> Result<Self> {
        let client = Surreal::init();

        // Connect to database
        client.connect(config.connection_string()).await?;

        // Use namespace and database
        client.use_ns(&config.namespace).await?;
        client.use_db(&config.database).await?;

        // Run migrations
        Self::run_migrations(&client).await?;

        Ok(Self { client, config })
    }

    /// Query builder
    pub fn query(&self) -> QueryBuilder {
        QueryBuilder::new(self.client.clone())
    }

    /// Execute a query
    pub async fn execute<T>(&self, query: &str, vars: Value) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut query = self.client.query(query);

        if !vars.is_null() {
            query = query.vars(vars);
        }

        let result: QueryResult<T> = query.await?;
        Ok(result.take(0)?)
    }

    /// Create a record
    pub async fn create<T>(&self, id: &str, data: T) -> Result<T>
    where
        T: Serialize,
    {
        let result: T = self.client.create(id).content(data).await?;
        Ok(result)
    }

    /// Update a record
    pub async fn update<T>(&self, id: &str, data: T) -> Result<T>
    where
        T: Serialize,
    {
        let result: T = self.client.update(id).content(data).await?;
        Ok(result)
    }

    /// Select a record
    pub async fn select<T>(&self, id: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let result: T = self.client.select(id).await?;
        Ok(result)
    }

    /// Delete a record
    pub async fn delete<T>(&self, id: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let result: T = self.client.delete(id).await?;
        Ok(result)
    }
}
```

#### Query Builder
```rust
/// SQL-like query builder
pub struct QueryBuilder<'a> {
    client: &'a Surreal<Db>,
    query: String,
    variables: Value,
}

impl<'a> QueryBuilder<'a> {
    pub fn new(client: Surreal<Db>) -> Self {
        Self {
            client,
            query: String::new(),
            variables: Value::Object(serde_json::Map::new()),
        }
    }

    /// SELECT statement
    pub fn select(mut self, fields: &str) -> Self {
        self.query.push_str("SELECT ");
        self.query.push_str(fields);
        self.query.push_str(" ");
        self
    }

    /// FROM statement
    pub fn from(mut self, table: &str) -> Self {
        self.query.push_str("FROM ");
        self.query.push_str(table);
        self.query.push_str(" ");
        self
    }

    /// WHERE clause
    pub fn where_clause(mut self, condition: &str) -> Self {
        self.query.push_str("WHERE ");
        self.query.push_str(condition);
        self.query.push_str(" ");
        self
    }

    /// ORDER BY clause
    pub fn order_by(mut self, field: &str, direction: SortDirection) -> Self {
        self.query.push_str("ORDER BY ");
        self.query.push_str(field);
        self.query.push_str(" ");
        self.query.push_str(match direction {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        });
        self.query.push_str(" ");
        self
    }

    /// LIMIT clause
    pub fn limit(mut self, limit: usize) -> Self {
        self.query.push_str("LIMIT ");
        self.query.push_str(&limit.to_string());
        self.query.push_str(" ");
        self
    }

    /// Bind variables
    pub fn bind(mut self, key: &str, value: Value) -> Self {
        let mut vars = std::mem::replace(&mut self.variables, Value::Null);
        if let Value::Object(mut map) = vars {
            map.insert(key.to_string(), value);
            self.variables = Value::Object(map);
        }
        self
    }

    /// Execute the query
    pub async fn execute<T>(self) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let response: Response = self.client.query(&self.query)
            .vars(self.variables)
            .await?;

        let mut results = Vec::new();
        for i in 0..response.len() {
            let result: T = response.take(i)?;
            results.push(result);
        }

        Ok(results)
    }
}
```

### crucible-llm

LLM service integration with multiple providers.

#### LLM Service
```rust
/// LLM service
pub struct LLMService {
    config: LLMConfig,
    providers: HashMap<String, Box<dyn LLMProvider>>,
    default_provider: String,
}

impl LLMService {
    /// Create a new LLM service
    pub fn new(config: LLMConfig) -> Self {
        let mut providers = HashMap::new();

        // Register providers
        providers.insert("openai".to_string(), Box::new(OpenAIProvider::new()));
        providers.insert("ollama".to_string(), Box::new(OllamaProvider::new()));
        providers.insert("anthropic".to_string(), Box::new(AnthropicProvider::new()));

        Self {
            config,
            providers,
            default_provider: config.default_provider.clone(),
        }
    }

    /// Generate a completion
    pub async fn generate_completion(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let provider = self.providers.get(&self.default_provider)
            .ok_or_else(|| LLMError::ProviderNotFound(self.default_provider.clone()))?;

        provider.generate_completion(request).await
    }

    /// Generate embeddings
    pub async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        let provider = self.providers.get(&self.default_provider)
            .ok_or_else(|| LLMError::ProviderNotFound(self.default_provider.clone()))?;

        provider.generate_embeddings(text).await
    }

    /// Analyze content
    pub async fn analyze_content(&self, content: &str, analysis_type: AnalysisType) -> Result<AnalysisResult> {
        let provider = self.providers.get(&self.default_provider)
            .ok_or_else(|| LLMError::ProviderNotFound(self.default_provider.clone()))?;

        provider.analyze_content(content, analysis_type).await
    }
}
```

#### LLM Provider Trait
```rust
/// LLM provider trait
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Generate a completion
    async fn generate_completion(&self, request: CompletionRequest) -> Result<CompletionResponse>;

    /// Generate embeddings
    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>>;

    /// Analyze content
    async fn analyze_content(&self, content: &str, analysis_type: AnalysisType) -> Result<AnalysisResult>;

    /// Get provider info
    fn info(&self) -> ProviderInfo;
}

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub model: String,
    pub context: Option<String>,
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub usage: TokenUsage,
    pub model: String,
}

/// Token usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}
```

## Interface APIs

### crucible-cli

Command-line interface with REPL and command processing.

#### CLI Application
```rust
/// CLI application
pub struct CLIApp {
    config: AppConfig,
    repl: Option<Repl>,
    commands: HashMap<String, Box<dyn Command>>,
}

impl CLIApp {
    /// Create a new CLI application
    pub async fn new(config: AppConfig) -> Result<Self> {
        let mut commands = HashMap::new();

        // Register built-in commands
        commands.insert("help".to_string(), Box::new(HelpCommand::new()));
        commands.insert("stats".to_string(), Box::new(StatsCommand::new()));
        commands.insert("search".to_string(), Box::new(SearchCommand::new()));
        commands.insert("repl".to_string(), Box::new(ReplCommand::new()));

        Ok(Self {
            config,
            repl: None,
            commands,
        })
    }

    /// Run the CLI application
    pub async fn run(&mut self, args: Vec<String>) -> Result<()> {
        match args.len() {
            0 => self.start_repl().await,
            1 => {
                let cmd = args[0].clone();
                self.execute_command(&cmd, vec![]).await
            }
            _ => {
                let cmd = args[0].clone();
                let args = args[1..].to_vec();
                self.execute_command(&cmd, args).await
            }
        }
    }

    /// Execute a command
    pub async fn execute_command(&mut self, name: &str, args: Vec<String>) -> Result<()> {
        if let Some(cmd) = self.commands.get(name) {
            cmd.execute(&self.config, args).await
        } else {
            Err(CLIError::CommandNotFound(name.to_string()))
        }
    }

    /// Start the REPL
    pub async fn start_repl(&mut self) -> Result<()> {
        let repl = Repl::new(self.config.clone()).await?;
        repl.run().await?;
        Ok(())
    }
}
```

#### REPL Interface
```rust
/// REPL interface
pub struct Repl {
    editor: Reedline,
    services: Arc<crucible_services::ServiceRegistry>,
    tools: Arc<crucible_tools::ToolRegistry>,
    history: FileBackedHistory,
    db: Arc<SurrealService>,
}

impl Repl {
    /// Create a new REPL
    pub async fn new(config: AppConfig) -> Result<Self> {
        let history = FileBackedHistory::with_file(1000, "~/.crucible/history")?;

        let mut editor = Reedline::create()
            .with_history(history.clone())
            .with_completer(Box::new(ReplCompleter::new()));

        Ok(Self {
            editor,
            services: Self::setup_services(config).await?,
            tools: Self::setup_tools(config).await?,
            history,
            db: Self::setup_database(config).await?,
        })
    }

    /// Run the REPL
    pub async fn run(&mut self) -> Result<()> {
        loop {
            let input = self.editor.read_line("> ").await?;

            match self.process_input(&input).await {
                Ok(Some(output)) => println!("{}", output),
                Ok(None) => break,
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        Ok(())
    }

    /// Process input
    async fn process_input(&mut self, input: &str) -> Result<Option<String>> {
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Ok(None);
        }

        if trimmed.starts_with(':') {
            self.execute_command(trimmed).await
        } else {
            self.execute_query(trimmed).await
        }
    }

    /// Execute a command
    async fn execute_command(&self, input: &str) -> Result<Option<String>> {
        // Parse and execute built-in commands
        Ok(None)
    }

    /// Execute a query
    async fn execute_query(&self, query: &str) -> Result<Option<String>> {
        // Execute SurrealQL query
        let results: Vec<serde_json::Value> = self.db.query(query).await?;

        let output = serde_json::to_string_pretty(&results)
            .map_err(|e| Error::Serialization(e.to_string()))?;

        Ok(Some(output))
    }
}
```

### crucible-tauri

Tauri desktop application backend.

#### Tauri Application
```rust
/// Tauri application
pub struct TauriApp {
    app: tauri::App,
    services: Arc<crucible_services::ServiceRegistry>,
    config: AppConfig,
}

impl TauriApp {
    /// Create a new Tauri application
    pub async fn new() -> Result<Self> {
        let config = Self::load_config().await?;
        let services = Self::setup_services(&config).await?;

        let mut builder = tauri::Builder::default()
            .plugin(tauri_plugin_window::init())
            .plugin(tauri_plugin_notification::init());

        // Register commands
        builder = builder.invoke_handler(tauri::generate_handler![
            search_documents,
            get_document,
            create_document,
            update_document,
            delete_document,
            execute_tool,
            list_tools,
        ]);

        let app = builder.build(tauri::generate_context!())
            .expect("error while running tauri application");

        Ok(Self {
            app,
            services,
            config,
        })
    }

    /// Run the application
    pub fn run(self) {
        self.app.run(|_app_handle, event| {
            match event {
                tauri::RunEvent::Exit => {}
                tauri::RunEvent::WindowEvent { .. } => {}
            }
        });
    }
}
```

#### Tauri Commands
```rust
/// Search documents command
#[tauri::command]
async fn search_documents(
    app: tauri::AppHandle,
    query: String,
    options: Option<SearchOptions>,
) -> Result<serde_json::Value, String> {
    let services: &crucible_services::ServiceRegistry =
        &app.state::<crucible_services::ServiceRegistry>();

    let search_service = services.get::<SearchService>("search")
        .map_err(|e| e.to_string())?;

    let options = options.unwrap_or_default();
    let results = search_service.search(&query, options).await
        .map_err(|e| e.to_string())?;

    Ok(serde_json::to_value(results).unwrap())
}

/// Execute tool command
#[tauri::command]
async fn execute_tool(
    app: tauri::AppHandle,
    tool_name: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let services: &crucible_services::ServiceRegistry =
        &app.state::<crucible_services::ServiceRegistry>();

    let tool_service = services.get::<ToolService>("tools")
        .map_err(|e| e.to_string())?;

    let result = tool_service.execute_tool(&tool_name, params).await
        .map_err(|e| e.to_string())?;

    Ok(result)
}
```

## Error Handling

### Error Types

#### Core Errors
```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Document not found: {0}")]
    DocumentNotFound(String),

    #[error("Invalid document format: {0}")]
    InvalidFormat(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Service error: {0}")]
    Service(String),

    #[error("Tool execution error: {0}")]
    Tool(String),

    #[error("Database error: {0}")]
    Database(String),
}

impl From<surrealdb::Error> for Error {
    fn from(err: surrealdb::Error) -> Self {
        Error::Database(err.to_string())
    }
}
```

#### Service Errors
```rust
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service not found: {0}")]
    NotFound(String),

    #[error("Service already exists: {0}")]
    AlreadyExists(String),

    #[error("Service configuration error: {0}")]
    Configuration(String),

    #[error("Service execution error: {0}")]
    Execution(String),
}
```

#### Tool Errors
```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Schema validation error: {0}")]
    SchemaValidation(String),
}
```

## Configuration Reference

### Service Configuration
```yaml
services:
  search:
    enabled: true
    type: "crucible_services::SearchService"
    config:
      index_path: "./indexes"
      max_results: 100
      fuzzy_search: true

  agent:
    enabled: true
    type: "crucible_services::AgentService"
    config:
      default_model: "gpt-3.5-turbo"
      max_tokens: 2000
      temperature: 0.7

  tools:
    static:
      - name: "search"
        module: "crucible_tools::search"
        function: "search_notes"
      - name: "metadata"
        module: "crucible_tools::metadata"
        function: "extract_metadata"

    dynamic:
      - name: "custom_search"
        path: "./tools/custom_search.rn"
        hot_reload: true

  hot_reload:
    enabled: true
    watch_paths: ["./tools", "./scripts"]
    debounce_ms: 1000
```

### Database Configuration
```yaml
database:
  surrealdb:
    connection_string: "ws://localhost:8000"
    namespace: "crucible"
    database: "main"

  duckdb:
    path: "./data/duckdb"

  storage_path: "./data"
```

### LLM Configuration
```yaml
llm:
  default_provider: "openai"
  providers:
    openai:
      api_key: "${OPENAI_API_KEY}"
      base_url: "https://api.openai.com/v1"
      model: "gpt-3.5-turbo"

    ollama:
      base_url: "http://localhost:11434"
      model: "llama2"
```

---

*This API documentation will be updated as the project evolves. Check for the latest version in the documentation repository.*