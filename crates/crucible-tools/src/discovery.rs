//! Enhanced tool discovery for flexible organization patterns
//!
//! This module implements comprehensive tool discovery with support for:
//! - Simple direct tools (backwards compatibility)
//! - Organized tools in modules
//! - Flexible naming conventions
//! - Consumer awareness without restrictions
//! - Hot-reload support
//! - AST-based analysis

use crate::{tool::RuneTool, tool::ToolMetadata, analyzer::RuneAstAnalyzer, types::ValidationResult};
use anyhow::{Context, Result};
use chrono::Utc;
use std::hash::{Hash, Hasher};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Discovery configuration
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// File extensions to consider
    pub extensions: Vec<String>,
    /// Directories to exclude
    pub exclude_dirs: Vec<String>,
    /// Files to exclude
    pub exclude_files: Vec<String>,
    /// Whether to enable hot-reload
    pub hot_reload: bool,
    /// Whether to validate tools during discovery
    pub validate_tools: bool,
    /// Maximum file size to process (bytes)
    pub max_file_size: usize,
    /// Whether to follow symbolic links
    pub follow_symlinks: bool,
    /// Discovery patterns to use
    pub patterns: DiscoveryPatterns,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            extensions: vec!["rn".to_string(), "rune".to_string()],
            exclude_dirs: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                ".crucible".to_string(),
            ],
            exclude_files: vec![
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
            ],
            hot_reload: true,
            validate_tools: true,
            max_file_size: 10 * 1024 * 1024, // 10MB
            follow_symlinks: false,
            patterns: DiscoveryPatterns::default(),
        }
    }
}

/// Discovery patterns configuration
#[derive(Debug, Clone)]
pub struct DiscoveryPatterns {
    /// Enable simple direct tool discovery
    pub direct_tools: bool,
    /// Enable module-based tool discovery
    pub module_tools: bool,
    /// Enable semantic naming convention
    pub semantic_naming: bool,
    /// Enable topic-module-function pattern
    pub topic_module_function: bool,
    /// Custom patterns
    pub custom_patterns: HashMap<String, CustomPattern>,
}

impl Default for DiscoveryPatterns {
    fn default() -> Self {
        Self {
            direct_tools: true,
            module_tools: true,
            semantic_naming: false,
            topic_module_function: false,
            custom_patterns: HashMap::new(),
        }
    }
}

/// Custom discovery pattern
#[derive(Debug, Clone)]
pub struct CustomPattern {
    /// Pattern name
    pub name: String,
    /// Regex pattern to match
    pub regex: String,
    /// Extraction groups
    pub groups: Vec<String>,
    /// Tool name template
    pub name_template: String,
}

/// Discovery result containing all tools found in a file
#[derive(Debug, Clone)]
pub struct DiscoveredTools {
    /// Unique discovery ID
    pub id: String,
    /// File path
    pub file_path: PathBuf,
    /// Tools discovered in this file
    pub tools: Vec<DiscoveredTool>,
    /// File metadata
    pub metadata: FileMetadata,
    /// Discovery timestamp
    pub discovered_at: chrono::DateTime<Utc>,
    /// Discovery status
    pub status: DiscoveryStatus,
}

/// Discovery status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryStatus {
    /// Discovery completed successfully
    Success,
    /// Discovery completed with warnings
    Warning(String),
    /// Discovery failed
    Error(String),
}

/// Individual discovered tool
#[derive(Debug, Clone)]
pub struct DiscoveredTool {
    /// Unique tool identifier
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema
    pub input_schema: Value,
    /// Output schema (optional)
    pub output_schema: Option<Value>,
    /// Entry point information
    pub entry_point: ToolEntryPoint,
    /// Tool metadata
    pub metadata: ToolMetadata,
    /// Consumer information
    pub consumer_info: ConsumerInfo,
    /// Tool category
    pub category: String,
    /// Tool tags
    pub tags: Vec<String>,
    /// Source location in file
    pub location: SourceLocation,
}

/// How to invoke the tool
#[derive(Debug, Clone)]
pub enum ToolEntryPoint {
    /// Direct function call (simple tools)
    Direct {
        function_name: String,
        module_path: Vec<String>,
    },
    /// Module-based function call (organized tools)
    Module {
        module_path: Vec<String>,
        function_name: String,
    },
    /// Struct method call
    Method {
        struct_name: String,
        method_name: String,
    },
    /// Macro-generated tool
    Macro {
        macro_name: String,
        generated_function: String,
    },
}

/// File-level metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// Naming convention used
    pub naming_convention: Option<String>,
    /// File information
    pub file_info: HashMap<String, Value>,
    /// File size in bytes
    pub file_size: u64,
    /// File modification time
    pub modified_time: chrono::DateTime<Utc>,
    /// File hash for change detection
    pub file_hash: Option<String>,
}

/// Consumer information for tool
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsumerInfo {
    /// Primary consumers
    pub primary_consumers: Vec<String>,
    /// Secondary consumers
    pub secondary_consumers: Vec<String>,
    /// UI hints
    pub ui_hints: HashMap<String, Value>,
    /// Agent hints
    pub agent_hints: HashMap<String, Value>,
    /// Integration requirements
    pub integration_requirements: Vec<String>,
}

/// Source location information
#[derive(Debug, Clone)]
pub struct SourceLocation {
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Byte offset
    pub offset: usize,
    /// Length of the item
    pub length: usize,
}

/// Enhanced tool discovery engine
pub struct ToolDiscovery {
    /// Rune context for compilation
    context: Arc<rune::Context>,
    /// Discovery configuration
    config: DiscoveryConfig,
    /// Cache for discovered tools
    cache: Arc<RwLock<HashMap<PathBuf, DiscoveredTools>>>,
    /// File watcher for hot-reload
    #[cfg(feature = "hot-reload")]
    file_watcher: Option<Arc<notify::RecommendedWatcher>>,
    /// AST analyzer
    analyzer: Arc<RuneAstAnalyzer>,
}

impl ToolDiscovery {
    /// Create a new discovery engine
    pub fn new(config: DiscoveryConfig) -> Result<Self> {
        let context = Arc::new(rune::Context::with_default_modules()?);
        let analyzer = Arc::new(RuneAstAnalyzer::new()?);

        Ok(Self {
            context,
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "hot-reload")]
            file_watcher: None,
            analyzer,
        })
    }

    /// Create discovery engine with custom context
    pub fn with_context(config: DiscoveryConfig, context: Arc<rune::Context>) -> Result<Self> {
        let analyzer = Arc::new(RuneAstAnalyzer::new()?);

        Ok(Self {
            context,
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "hot-reload")]
            file_watcher: None,
            analyzer,
        })
    }

    /// Discover tools in a directory
    pub async fn discover_from_directory<P: AsRef<Path>>(&self, dir_path: P) -> Result<Vec<DiscoveredTools>> {
        let dir_path = dir_path.as_ref();
        info!("Starting tool discovery in directory: {:?}", dir_path);

        if !dir_path.exists() {
            warn!("Tool directory does not exist: {:?}", dir_path);
            return Ok(Vec::new());
        }

        if !dir_path.is_dir() {
            return Err(anyhow::anyhow!("Path is not a directory: {:?}", dir_path));
        }

        let mut all_discoveries = Vec::new();
        let mut entries = tokio::fs::read_dir(dir_path).await
            .with_context(|| format!("Failed to read tool directory: {:?}", dir_path))?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            if self.should_skip_path(&path) {
                continue;
            }

            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if self.config.extensions.contains(&ext.to_string()) {
                        match self.discover_from_file(&path).await {
                            Ok(discovery) => {
                                info!("Discovered {} tools in {:?}", discovery.tools.len(), path);
                                all_discoveries.push(discovery);
                            }
                            Err(e) => {
                                warn!("Failed to discover tools in {:?}: {}", path, e);
                            }
                        }
                    }
                }
            } else if path.is_dir() && self.config.follow_symlinks || !path.is_symlink() {
                // Recursively discover in subdirectories
                match Box::pin(self.discover_from_directory(&path)).await {
                    Ok(mut discoveries) => {
                        all_discoveries.append(&mut discoveries);
                    }
                    Err(e) => {
                        warn!("Failed to discover tools in subdirectory {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Discovery complete. Found {} files with tools", all_discoveries.len());
        Ok(all_discoveries)
    }

    /// Discover tools in a single file
    pub async fn discover_from_file<P: AsRef<Path>>(&self, file_path: P) -> Result<DiscoveredTools> {
        let file_path = file_path.as_ref();
        debug!("Discovering tools in file: {:?}", file_path);

        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(file_path) {
                // Check if file has been modified
                if let Ok(metadata) = tokio::fs::metadata(file_path).await {
                    if let Ok(modified) = metadata.modified() {
                        let modified_time: chrono::DateTime<chrono::Utc> = chrono::DateTime::from(modified);
                        if modified_time <= cached.metadata.modified_time {
                            debug!("Using cached discovery for {:?}", file_path);
                            return Ok(cached.clone());
                        }
                    }
                }
            }
        }

        // Read file content
        let source_code = tokio::fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        // Check file size
        if source_code.len() > self.config.max_file_size {
            return Err(anyhow::anyhow!(
                "File too large: {} bytes (max: {} bytes)",
                source_code.len(),
                self.config.max_file_size
            ));
        }

        // Get file metadata
        let file_metadata = tokio::fs::metadata(file_path).await?;
        let modified_time = chrono::DateTime::from(file_metadata.modified()?);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source_code.hash(&mut hasher);
        let file_hash = Some(format!("{:x}", hasher.finish()));

        // Compile the file to analyze its structure
        let unit = self.compile_file(&source_code, file_path)?;

        // Discover tools using enabled patterns
        let mut tools = Vec::new();

        if self.config.patterns.direct_tools {
            tools.extend(self.discover_direct_tools(&unit, file_path, &source_code)?);
        }

        if self.config.patterns.module_tools {
            tools.extend(self.discover_module_tools(&unit, file_path, &source_code)?);
        }

        // Apply custom patterns
        for (name, pattern) in &self.config.patterns.custom_patterns {
            tools.extend(self.discover_with_custom_pattern(
                &unit,
                file_path,
                &source_code,
                pattern,
                name,
            )?);
        }

        // Validate tools if enabled
        if self.config.validate_tools {
            tools = self.validate_discovered_tools(tools).await?;
        }

        // Create file metadata
        let metadata = FileMetadata {
            naming_convention: self.detect_naming_convention(&source_code),
            file_info: self.extract_file_info(&unit, &source_code)?,
            file_size: source_code.len() as u64,
            modified_time,
            file_hash,
        };

        let discovery = DiscoveredTools {
            id: Uuid::new_v4().to_string(),
            file_path: file_path.to_path_buf(),
            tools,
            metadata,
            discovered_at: Utc::now(),
            status: DiscoveryStatus::Success,
        };

        // Cache the result
        {
            let mut cache = self.cache.write().await;
            cache.insert(file_path.to_path_buf(), discovery.clone());
        }

        Ok(discovery)
    }

    /// Validate a Rune file without loading it
    pub async fn validate_file<P: AsRef<Path>>(&self, file_path: P) -> Result<ValidationResult> {
        let file_path = file_path.as_ref();
        let source_code = tokio::fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {:?}", file_path))?;

        let unit = self.compile_file(&source_code, file_path)?;

        // Basic validation
        let mut validation = ValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            metadata: HashMap::new(),
        };

        // Try to extract required exports
        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit);

        // Check for NAME
        if vm.call(["NAME"], ()).is_err() {
            validation.valid = false;
            validation.errors.push("Missing required NAME function".to_string());
        }

        // Check for DESCRIPTION
        if vm.call(["DESCRIPTION"], ()).is_err() {
            validation.valid = false;
            validation.errors.push("Missing required DESCRIPTION function".to_string());
        }

        // Check for INPUT_SCHEMA
        if vm.call(["INPUT_SCHEMA"], ()).is_err() {
            validation.valid = false;
            validation.errors.push("Missing required INPUT_SCHEMA function".to_string());
        }

        // Check for call function
        if vm.call(["call"], ()).is_err() {
            validation.valid = false;
            validation.errors.push("Missing required call function".to_string());
        }

        Ok(validation)
    }

    /// Clear the discovery cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        info!("Discovery cache cleared");
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> CacheStats {
        let cache = self.cache.read().await;
        CacheStats {
            entries: cache.len(),
            total_tools: cache.values().map(|d| d.tools.len()).sum(),
            memory_usage: std::mem::size_of_val(&*cache) as u64,
        }
    }

    /// Check if a path should be skipped during discovery
    fn should_skip_path(&self, path: &Path) -> bool {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            if self.config.exclude_files.contains(&file_name.to_string()) {
                return true;
            }
        }

        if let Some(dir_name) = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
            if self.config.exclude_dirs.contains(&dir_name.to_string()) {
                return true;
            }
        }

        false
    }

    /// Compile a Rune file for analysis
    fn compile_file(&self, source_code: &str, file_path: &Path) -> Result<Arc<rune::Unit>> {
        let source = rune::Source::memory(source_code)?;

        let mut sources = rune::Sources::new();
        sources.insert(source)?;

        let mut diagnostics = rune::Diagnostics::new();

        let result = rune::prepare(&mut sources)
            .with_context(&self.context)
            .with_diagnostics(&mut diagnostics)
            .build();

        // Report warnings/errors but don't fail on warnings
        if !diagnostics.is_empty() {
            debug!("Compilation diagnostics for {:?}:", file_path);
            for diagnostic in diagnostics.diagnostics() {
                debug!("  {:?}", diagnostic);
            }
        }

        let unit = result.with_context(|| format!("Failed to compile Rune unit: {:?}", file_path))?;
        Ok(Arc::new(unit))
    }

    /// Discover simple direct tools (backwards compatibility)
    fn discover_direct_tools(
        &self,
        unit: &Arc<rune::Unit>,
        file_path: &Path,
        source_code: &str,
    ) -> Result<Vec<DiscoveredTool>> {
        let mut tools = Vec::new();

        let runtime = Arc::new(self.context.runtime()?);
        let mut vm = rune::runtime::Vm::new(runtime, unit.clone());

        // Extract tool metadata
        let tool_name = match vm.call(["NAME"], ()) {
            Ok(value) => {
                let name: String = rune::from_value(value)?;
                debug!("Found direct tool name: {}", name);
                name
            }
            Err(_) => {
                debug!("No NAME function found, using filename");
                file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown_tool")
                    .to_string()
            }
        };

        let description = match vm.call(["DESCRIPTION"], ()) {
            Ok(value) => {
                let desc: String = rune::from_value(value)?;
                desc
            }
            Err(_) => format!("Tool from {:?}", file_path),
        };

        let input_schema = match vm.call(["INPUT_SCHEMA"], ()) {
            Ok(value) => super::tool::rune_value_to_json(&value)?,
            Err(_) => serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        };

        // Try to extract optional OUTPUT_SCHEMA
        let output_schema = match vm.call(["OUTPUT_SCHEMA"], ()) {
            Ok(value) => Some(super::tool::rune_value_to_json(&value)?),
            Err(_) => None,
        };

        // Find source location
        let location = self.find_function_location(source_code, "call")?;

        let tool = DiscoveredTool {
            id: Uuid::new_v4().to_string(),
            name: tool_name.clone(),
            description: description.clone(),
            input_schema: input_schema.clone(),
            output_schema: output_schema.clone(),
            entry_point: ToolEntryPoint::Direct {
                function_name: "call".to_string(),
                module_path: Vec::new(),
            },
            metadata: ToolMetadata::default(),
            consumer_info: ConsumerInfo::default(),
            category: "general".to_string(),
            tags: vec!["direct".to_string()],
            location,
        };

        tools.push(tool);
        Ok(tools)
    }

    /// Discover organized tools in modules
    fn discover_module_tools(
        &self,
        unit: &Arc<rune::Unit>,
        file_path: &Path,
        source_code: &str,
    ) -> Result<Vec<DiscoveredTool>> {
        let mut tools = Vec::new();

        let discovered_modules = self.analyzer.analyze_modules(unit)?;

        for module in discovered_modules {
            for function in module.functions {
                // Generate input schema
                let input_schema = self.generate_function_schema(&function)?;

                // Extract description from doc comments
                let description = if !function.doc_comments.is_empty() {
                    function.doc_comments.join(" ").trim().to_string()
                } else {
                    format!("{} function in {} module", function.name, module.name)
                };

                // Infer consumer information
                let consumer_info = self.infer_consumer_info(&module.name, &function.name);

                // Find source location
                let location = SourceLocation {
                    line: function.location.line,
                    column: function.location.column,
                    offset: function.location.offset,
                    length: function.name.len(),
                };

                let tool = DiscoveredTool {
                    id: Uuid::new_v4().to_string(),
                    name: format!("{}.{}", module.name, function.name),
                    description: description.clone(),
                    input_schema: input_schema.clone(),
                    output_schema: self.infer_output_schema(&function),
                    entry_point: ToolEntryPoint::Module {
                        module_path: module.path.clone(),
                        function_name: function.name.clone(),
                    },
                    metadata: ToolMetadata::default(),
                    consumer_info,
                    category: module.name.clone(),
                    tags: vec!["module".to_string()],
                    location,
                };

                tools.push(tool);
            }
        }

        Ok(tools)
    }

    /// Discover tools using custom pattern
    fn discover_with_custom_pattern(
        &self,
        _unit: &Arc<rune::Unit>,
        _file_path: &Path,
        _source_code: &str,
        _pattern: &CustomPattern,
        _pattern_name: &str,
    ) -> Result<Vec<DiscoveredTool>> {
        // TODO: Implement custom pattern discovery
        // This would use regex matching and template substitution
        Ok(Vec::new())
    }

    /// Generate schema for a function
    fn generate_function_schema(&self, function: &crate::types::AsyncFunctionInfo) -> Result<Value> {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &function.parameters {
            let param_schema = match param.type_name.to_lowercase().as_str() {
                "string" => serde_json::json!({"type": "string"}),
                "number" | "int" | "float" => serde_json::json!({"type": "number"}),
                "bool" | "boolean" => serde_json::json!({"type": "boolean"}),
                "array" | "vec" => serde_json::json!({
                    "type": "array",
                    "items": {"type": "string"}
                }),
                "object" | "map" => serde_json::json!({
                    "type": "object",
                    "additionalProperties": true
                }),
                _ => serde_json::json!({"type": "string"}),
            };

            properties.insert(param.name.clone(), param_schema);

            if !param.is_optional {
                required.push(param.name.clone());
            }
        }

        Ok(serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required
        }))
    }

    /// Infer consumer information
    fn infer_consumer_info(&self, module_name: &str, function_name: &str) -> ConsumerInfo {
        let mut consumer_info = ConsumerInfo::default();

        match module_name {
            "ui" | "ui_helpers" | "display" | "format" => {
                consumer_info.primary_consumers = vec!["ui".to_string()];
                consumer_info.ui_hints.insert("widget_type".to_string(), serde_json::Value::String("utility".to_string()));
            }
            "agent" | "ai" | "analysis" | "recommend" => {
                consumer_info.primary_consumers = vec!["agents".to_string()];
                consumer_info.agent_hints.insert("capability".to_string(), serde_json::Value::String("analysis".to_string()));
            }
            "file" | "search" | "data" | "storage" => {
                consumer_info.primary_consumers = vec!["agents".to_string(), "ui".to_string()];
                consumer_info.secondary_consumers = vec!["system".to_string()];
            }
            _ => {
                consumer_info.primary_consumers = vec!["agents".to_string(), "ui".to_string()];
            }
        }

        if function_name.contains("search") || function_name.contains("find") {
            consumer_info.ui_hints.insert("input_type".to_string(), serde_json::Value::String("search_query".to_string()));
        }

        consumer_info
    }

    /// Infer output schema
    fn infer_output_schema(&self, function: &crate::types::AsyncFunctionInfo) -> Option<Value> {
        if let Some(ref return_type) = function.return_type {
            match return_type.to_lowercase().as_str() {
                "string" => Some(serde_json::json!({
                    "type": "string",
                    "description": format!("String result from {}", function.name)
                })),
                "vec" | "array" => Some(serde_json::json!({
                    "type": "array",
                    "items": {"type": "string"},
                    "description": format!("Array result from {}", function.name)
                })),
                _ => Some(serde_json::json!({
                    "type": "object",
                    "description": format!("Result from {} function", function.name)
                })),
            }
        } else {
            None
        }
    }

    /// Detect naming convention used in the file
    fn detect_naming_convention(&self, source_code: &str) -> Option<String> {
        if source_code.contains("TOOL_NAMING_CONVENTION") {
            if source_code.contains("\"semantic\"") {
                return Some("semantic".to_string());
            } else if source_code.contains("\"topic_module_function\"") {
                return Some("topic_module_function".to_string());
            }
        }
        None
    }

    /// Extract file information
    fn extract_file_info(&self, _unit: &Arc<rune::Unit>, source_code: &str) -> Result<HashMap<String, Value>> {
        let mut info = HashMap::new();

        if source_code.contains("get_tool_metadata") {
            info.insert("has_metadata".to_string(), serde_json::Value::Bool(true));
        }

        if source_code.contains("#[tool]") {
            info.insert("has_macro_tools".to_string(), serde_json::Value::Bool(true));
        }

        // Count lines
        let line_count = source_code.lines().count();
        info.insert("line_count".to_string(), serde_json::Value::Number(line_count.into()));

        Ok(info)
    }

    /// Find function location in source code
    fn find_function_location(&self, source_code: &str, function_name: &str) -> Result<SourceLocation> {
        let pattern = format!("pub (async )?fn {}", function_name);
        let regex = regex::Regex::new(&pattern)?;

        if let Some(mat) = regex.find(source_code) {
            let line_num = source_code[..mat.start()].lines().count() + 1;
            let line_start = source_code[..mat.start()].rfind('\n').unwrap_or(0);
            let column = mat.start() - line_start + 1;

            Ok(SourceLocation {
                line: line_num,
                column,
                offset: mat.start(),
                length: mat.len(),
            })
        } else {
            // Default location if not found
            Ok(SourceLocation {
                line: 1,
                column: 1,
                offset: 0,
                length: function_name.len(),
            })
        }
    }

    /// Validate discovered tools
    async fn validate_discovered_tools(&self, tools: Vec<DiscoveredTool>) -> Result<Vec<DiscoveredTool>> {
        let mut valid_tools = Vec::new();

        for tool in tools {
            // Basic validation
            if !tool.name.is_empty() && !tool.description.is_empty() {
                valid_tools.push(tool);
            } else {
                warn!("Skipping invalid tool: {}", tool.name);
            }
        }

        Ok(valid_tools)
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached entries
    pub entries: usize,
    /// Total number of tools in cache
    pub total_tools: usize,
    /// Estimated memory usage
    pub memory_usage: u64,
}

/// Convert discovered tools to RuneTool instances
pub async fn convert_to_rune_tools(
    discoveries: Vec<DiscoveredTools>,
    context: &rune::Context
) -> Result<Vec<RuneTool>> {
    let mut rune_tools = Vec::new();

    for discovery in discoveries {
        for discovered_tool in discovery.tools {
            debug!("Converting discovered tool: {}", discovered_tool.name);

            // Read the source file to create the RuneTool
            let source_code = tokio::fs::read_to_string(&discovery.file_path).await
                .with_context(|| format!("Failed to read source file: {:?}", discovery.file_path))?;

            let metadata = Some(discovered_tool.metadata);
            let mut rune_tool = RuneTool::from_source(&source_code, context, metadata)?;

            // Apply additional information from discovery
            rune_tool.category = discovered_tool.category;
            rune_tool.tags = discovered_tool.tags;
            rune_tool.file_path = Some(discovery.file_path.clone());

            rune_tools.push(rune_tool);
        }
    }

    Ok(rune_tools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    #[tokio::test]
    async fn test_discovery_config_default() {
        let config = DiscoveryConfig::default();
        assert!(config.extensions.contains(&"rn".to_string()));
        assert!(config.exclude_dirs.contains(&".git".to_string()));
        assert!(config.hot_reload);
        assert!(config.validate_tools);
    }

    #[tokio::test]
    async fn test_tool_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        let tool_source = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool" }
            pub fn INPUT_SCHEMA() {
                #{ type: "object", properties: #{ name: #{ type: "string" } } }
            }
            pub async fn call(args) {
                #{ success: true, message: `Hello ${args.name}` }
            }
        "#;

        let tool_path = tool_dir.join("test.rn");
        fs::write(&tool_path, tool_source).await.unwrap();

        let config = DiscoveryConfig::default();
        let discovery = ToolDiscovery::new(config).unwrap();
        let discoveries = discovery.discover_from_directory(&tool_dir).await.unwrap();

        assert_eq!(discoveries.len(), 1);
        let discovered = &discoveries[0];
        assert_eq!(discovered.tools.len(), 1);
        assert_eq!(discovered.tools[0].name, "test_tool");
    }

    #[tokio::test]
    async fn test_cache_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        let tool_source = r#"
            pub fn NAME() { "cached_tool" }
            pub fn DESCRIPTION() { "A cached tool" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ success: true } }
        "#;

        let tool_path = tool_dir.join("cached.rn");
        fs::write(&tool_path, tool_source).await.unwrap();

        let config = DiscoveryConfig::default();
        let discovery = ToolDiscovery::new(config).unwrap();

        // First discovery
        let discoveries1 = discovery.discover_from_file(&tool_path).await.unwrap();

        // Second discovery (should use cache)
        let discoveries2 = discovery.discover_from_file(&tool_path).await.unwrap();

        assert_eq!(discoveries1.tools.len(), discoveries2.tools.len());
        assert_eq!(discoveries1.tools[0].name, discoveries2.tools[0].name);

        let stats = discovery.cache_stats().await;
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.total_tools, 1);
    }

    #[tokio::test]
    async fn test_validation() {
        let temp_dir = TempDir::new().unwrap();
        let tool_dir = temp_dir.path().to_path_buf();

        // Valid tool
        let valid_source = r#"
            pub fn NAME() { "valid_tool" }
            pub fn DESCRIPTION() { "A valid tool" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ success: true } }
        "#;

        // Invalid tool (missing DESCRIPTION)
        let invalid_source = r#"
            pub fn NAME() { "invalid_tool" }
            pub fn INPUT_SCHEMA() { #{ type: "object", properties: #{} } }
            pub async fn call(args) { #{ success: true } }
        "#;

        let valid_path = tool_dir.join("valid.rn");
        let invalid_path = tool_dir.join("invalid.rn");

        fs::write(&valid_path, valid_source).await.unwrap();
        fs::write(&invalid_path, invalid_source).await.unwrap();

        let config = DiscoveryConfig::default();
        let discovery = ToolDiscovery::new(config).unwrap();

        let valid_result = discovery.validate_file(&valid_path).await.unwrap();
        assert!(valid_result.valid);

        let invalid_result = discovery.validate_file(&invalid_path).await.unwrap();
        assert!(!invalid_result.valid);
        assert!(!invalid_result.errors.is_empty());
    }
}