//! Dynamic tool loading and hot-reload functionality
//!
//! This module provides dynamic loading of Rune tools from files,
//! hot-reload support for development, and caching mechanisms.

use crate::context::ContextManager;
use crate::discovery::{ToolDiscovery, DiscoveryConfig};
use crate::errors::{RuneError, ContextualError, ErrorContext};
use crate::rune_registry::RuneToolRegistry;
use crate::tool::RuneTool;
use crate::types::{LoadingStatus, ToolLoadingResult};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use tracing::{debug, info, warn, error};
use uuid::Uuid;

/// Configuration for tool loading
#[derive(Debug, Clone)]
pub struct LoaderConfig {
    /// Directories to scan for tools
    pub tool_directories: Vec<PathBuf>,
    /// File patterns to watch
    pub file_patterns: Vec<String>,
    /// Whether to enable hot-reload
    pub enable_hot_reload: bool,
    /// Hot-reload debounce time in milliseconds
    pub hot_reload_debounce_ms: u64,
    /// Whether to load tools recursively
    pub recursive_loading: bool,
    /// Whether to validate tools before loading
    pub validate_before_loading: bool,
    /// Maximum number of concurrent loading operations
    pub max_concurrent_loads: usize,
    /// Loading timeout in seconds
    pub loading_timeout_secs: u64,
}

impl Default for LoaderConfig {
    fn default() -> Self {
        Self {
            tool_directories: vec![
                std::path::PathBuf::from("./tools"),
                std::path::PathBuf::from("./rune-tools"),
                std::path::PathBuf::from("./scripts"),
            ],
            file_patterns: vec!["*.rn".to_string(), "*.rune".to_string()],
            enable_hot_reload: true,
            hot_reload_debounce_ms: 500,
            recursive_loading: true,
            validate_before_loading: true,
            max_concurrent_loads: 10,
            loading_timeout_secs: 30,
        }
    }
}

/// Dynamic tool loader with hot-reload support
pub struct ToolLoader {
    /// Configuration
    config: LoaderConfig,
    /// Tool registry
    registry: Arc<RuneToolRegistry>,
    /// Context manager
    context_manager: Arc<Mutex<ContextManager>>,
    /// Tool discovery
    discovery: Arc<ToolDiscovery>,
    /// Hot-reload watcher
    #[cfg(feature = "hot-reload")]
    file_watcher: Option<Arc<notify::RecommendedWatcher>>,
    /// Loaded tools by file path
    loaded_tools: Arc<RwLock<HashMap<PathBuf, String>>>,
    /// Loading state
    loading_state: Arc<RwLock<LoadingState>>,
    /// Event handlers
    event_handlers: Arc<Mutex<Vec<Box<dyn LoaderEventHandler>>>>,
}

/// Loading state for tracking concurrent operations
#[derive(Debug, Default)]
struct LoadingState {
    active_loads: HashMap<String, LoadingOperation>,
    pending_reloads: Vec<PathBuf>,
    loading_history: Vec<ToolLoadingResult>,
}

/// Active loading operation
#[derive(Debug)]
struct LoadingOperation {
    id: String,
    file_path: PathBuf,
    start_time: chrono::DateTime<chrono::Utc>,
    status: LoadingStatus,
}

/// Event handler for loader events
pub trait LoaderEventHandler: Send + Sync {
    /// Called when a tool is loaded
    fn on_tool_loaded(&self, tool_name: &str, file_path: &PathBuf);
    /// Called when a tool fails to load
    fn on_tool_load_failed(&self, tool_name: &str, file_path: &PathBuf, error: &str);
    /// Called when a tool is reloaded
    fn on_tool_reloaded(&self, tool_name: &str, file_path: &PathBuf);
    /// Called when hot-reload detects changes
    fn on_file_changed(&self, file_path: &PathBuf);
}

/// Default event handler that logs events
pub struct DefaultEventHandler;

impl LoaderEventHandler for DefaultEventHandler {
    fn on_tool_loaded(&self, tool_name: &str, file_path: &PathBuf) {
        info!("Tool '{}' loaded from {:?}", tool_name, file_path);
    }

    fn on_tool_load_failed(&self, tool_name: &str, file_path: &PathBuf, error: &str) {
        error!("Failed to load tool '{}' from {:?}: {}", tool_name, file_path, error);
    }

    fn on_tool_reloaded(&self, tool_name: &str, file_path: &PathBuf) {
        info!("Tool '{}' reloaded from {:?}", tool_name, file_path);
    }

    fn on_file_changed(&self, file_path: &PathBuf) {
        debug!("File changed: {:?}", file_path);
    }
}

impl ToolLoader {
    /// Create a new tool loader
    pub fn new(
        config: LoaderConfig,
        registry: Arc<RuneToolRegistry>,
        context_manager: Arc<Mutex<ContextManager>>,
    ) -> Result<Self, RuneError> {
        let discovery_config = DiscoveryConfig {
            extensions: config.file_patterns
                .iter()
                .filter_map(|p| p.strip_prefix('*').strip_prefix('.'))
                .collect(),
            exclude_dirs: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
            ],
            exclude_files: vec![
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
            ],
            hot_reload: config.enable_hot_reload,
            validate_tools: config.validate_before_loading,
            max_file_size: 10 * 1024 * 1024, // 10MB
            follow_symlinks: false,
            patterns: Default::default(),
        };

        let discovery = Arc::new(ToolDiscovery::new(discovery_config)?);

        let mut loader = Self {
            config,
            registry,
            context_manager,
            discovery,
            #[cfg(feature = "hot-reload")]
            file_watcher: None,
            loaded_tools: Arc::new(RwLock::new(HashMap::new())),
            loading_state: Arc::new(RwLock::new(LoadingState::default())),
            event_handlers: Arc::new(Mutex::new(Vec::new())),
        };

        // Add default event handler
        loader.add_event_handler(Box::new(DefaultEventHandler));

        #[cfg(feature = "hot-reload")]
        if loader.config.enable_hot_reload {
            loader.setup_file_watcher()?;
        }

        Ok(loader)
    }

    /// Load tools from configured directories
    pub async fn load_tools(&self) -> Result<usize, ContextualError> {
        info!("Loading tools from directories: {:?}", self.config.tool_directories);

        let mut total_loaded = 0;
        let mut loading_errors = Vec::new();

        for directory in &self.config.tool_directories {
            match self.load_tools_from_directory(directory).await {
                Ok(count) => {
                    total_loaded += count;
                    info!("Loaded {} tools from {:?}", count, directory);
                }
                Err(e) => {
                    error!("Failed to load tools from {:?}: {}", directory, e);
                    loading_errors.push((directory.clone(), e));
                }
            }
        }

        if !loading_errors.is_empty() {
            warn!("Encountered {} loading errors", loading_errors.len());
        }

        info!("Successfully loaded {} tools total", total_loaded);
        Ok(total_loaded)
    }

    /// Load tools from a specific directory
    pub async fn load_tools_from_directory(&self, directory: &PathBuf) -> Result<usize, ContextualError> {
        if !directory.exists() {
            warn!("Tool directory does not exist: {:?}", directory);
            return Ok(0);
        }

        let context = ErrorContext::new()
            .with_operation("load_tools_from_directory")
            .with_file_path(directory);

        let discoveries = self.discovery.discover_from_directory(directory).await
            .map_err(|e| ContextualError::new(
                RuneError::DiscoveryError {
                    message: format!("Failed to discover tools in directory: {}", e),
                    path: Some(directory.clone()),
                },
                context,
            ))?;

        let mut loaded_count = 0;
        let mut loaded_tools = HashMap::new();

        for discovery in discoveries {
            for discovered_tool in discovery.tools {
                match self.load_tool_from_discovery(&discovery.file_path, &discovered_tool).await {
                    Ok(tool_name) => {
                        loaded_tools.insert(discovery.file_path.clone(), tool_name.clone());
                        loaded_count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to load tool from {:?}: {}", discovery.file_path, e);
                    }
                }
            }
        }

        // Update loaded tools tracking
        {
            let mut loaded = self.loaded_tools.write().await;
            for (path, tool_name) in loaded_tools {
                loaded.insert(path, tool_name);
            }
        }

        Ok(loaded_count)
    }

    /// Load a tool from a discovery result
    async fn load_tool_from_discovery(
        &self,
        file_path: &PathBuf,
        discovered_tool: &crate::discovery::DiscoveredTool,
    ) -> Result<String, ContextualError> {
        let start_time = std::time::Instant::now();
        let tool_name = discovered_tool.name.clone();

        let context = ErrorContext::new()
            .with_operation("load_tool_from_discovery")
            .with_tool_name(&tool_name)
            .with_file_path(file_path);

        // Check if already loading
        {
            let mut state = self.loading_state.write().await;
            if state.active_loads.contains_key(&tool_name) {
                return Err(ContextualError::new(
                    RuneError::LoadingError {
                        tool_name: tool_name.clone(),
                        source: anyhow::anyhow!("Tool is already being loaded"),
                    },
                    context,
                ));
            }

            // Add to active loads
            let operation_id = Uuid::new_v4().to_string();
            state.active_loads.insert(tool_name.clone(), LoadingOperation {
                id: operation_id,
                file_path: file_path.clone(),
                start_time: chrono::Utc::now(),
                status: LoadingStatus::Loading,
            });
        }

        let result = async {
            // Get or create context
            let mut context_manager = self.context_manager.lock().await;
            let rune_context = context_manager.get_context("default")
                .map_err(|e| ContextualError::new(
                    RuneError::ContextError {
                        message: format!("Failed to get context: {}", e),
                        context_type: Some("loader".to_string()),
                    },
                    context.clone(),
                ))?;

            // Load the tool
            let tool = RuneTool::from_file(file_path, &rune_context)
                .map_err(|e| ContextualError::new(
                    RuneError::LoadingError {
                        tool_name: tool_name.clone(),
                        source: e,
                    },
                    context.clone(),
                ))?;

            // Register the tool
            let registered_name = self.registry.register_tool(tool).await
                .map_err(|e| ContextualError::new(e, context.clone()))?;

            Ok::<String, ContextualError>(registered_name)
        }.await;

        // Update loading state and record result
        {
            let mut state = self.loading_state.write().await;
            state.active_loads.remove(&tool_name);

            let duration_ms = start_time.elapsed().as_millis() as u64;
            let loading_result = match &result {
                Ok(_) => ToolLoadingResult {
                    status: LoadingStatus::Success,
                    tool: None, // Could fetch from registry
                    duration_ms,
                    error: None,
                    warnings: Vec::new(),
                },
                Err(e) => ToolLoadingResult {
                    status: LoadingStatus::Error,
                    tool: None,
                    duration_ms,
                    error: Some(e.error.to_string()),
                    warnings: Vec::new(),
                },
            };

            state.loading_history.push(loading_result);
        }

        // Notify event handlers
        match &result {
            Ok(name) => {
                self.notify_tool_loaded(name, file_path).await;
                Ok(name.clone())
            }
            Err(e) => {
                self.notify_tool_load_failed(&tool_name, file_path, &e.error.to_string()).await;
                Err(e)
            }
        }
    }

    /// Reload a tool from its file
    pub async fn reload_tool(&self, tool_name: &str) -> Result<bool, ContextualError> {
        // Find the file path for the tool
        let file_path = {
            let loaded = self.loaded_tools.read().await;
            loaded.iter()
                .find(|(_, name)| name == tool_name)
                .map(|(path, _)| path.clone())
        };

        if let Some(file_path) = file_path {
            self.reload_tool_from_file(&file_path).await
        } else {
            warn!("Cannot reload tool '{}': file path not found", tool_name);
            Ok(false)
        }
    }

    /// Reload a tool from a specific file
    pub async fn reload_tool_from_file(&self, file_path: &PathBuf) -> Result<bool, ContextualError> {
        let context = ErrorContext::new()
            .with_operation("reload_tool_from_file")
            .with_file_path(file_path);

        // Check if file exists
        if !file_path.exists() {
            return Err(ContextualError::new(
                RuneError::LoadingError {
                    tool_name: "unknown".to_string(),
                    source: anyhow::anyhow!("File does not exist"),
                },
                context,
            ));
        }

        // Find tools that were loaded from this file
        let tool_names: Vec<String> = {
            let loaded = self.loaded_tools.read().await;
            loaded.iter()
                .filter(|(path, _)| path == file_path)
                .map(|(_, name)| name.clone())
                .collect()
        };

        if tool_names.is_empty() {
            return Ok(false);
        }

        // Rediscover tools in the file
        let discoveries = self.discovery.discover_from_file(file_path).await
            .map_err(|e| ContextualError::new(
                RuneError::DiscoveryError {
                    message: format!("Failed to rediscover tools: {}", e),
                    path: Some(file_path.clone()),
                },
                context,
            ))?;

        let mut reloaded_count = 0;

        for discovered_tool in discoveries.tools {
            if tool_names.contains(&discovered_tool.name) {
                match self.reload_tool_from_discovery(file_path, &discovered_tool).await {
                    Ok(_) => {
                        reloaded_count += 1;
                        self.notify_tool_reloaded(&discovered_tool.name, file_path).await;
                    }
                    Err(e) => {
                        warn!("Failed to reload tool '{}': {}", discovered_tool.name, e);
                    }
                }
            }
        }

        Ok(reloaded_count > 0)
    }

    /// Reload a specific tool from discovery
    async fn reload_tool_from_discovery(
        &self,
        file_path: &PathBuf,
        discovered_tool: &crate::discovery::DiscoveredTool,
    ) -> Result<(), ContextualError> {
        let tool_name = discovered_tool.name.clone();

        // Unregister existing tool
        let _ = self.registry.unregister_tool(&tool_name).await;

        // Load the tool again
        self.load_tool_from_discovery(file_path, discovered_tool).await?;

        Ok(())
    }

    /// Add an event handler
    pub async fn add_event_handler(&self, handler: Box<dyn LoaderEventHandler>) {
        let mut handlers = self.event_handlers.lock().await;
        handlers.push(handler);
    }

    /// Remove all event handlers
    pub async fn clear_event_handlers(&self) {
        let mut handlers = self.event_handlers.lock().await;
        handlers.clear();
    }

    /// Get loading statistics
    pub async fn get_loading_stats(&self) -> LoadingStats {
        let state = self.loading_state.read().await;
        let loaded = self.loaded_tools.read().await;

        let recent_results = state.loading_history.iter()
            .rev()
            .take(100)
            .collect::<Vec<_>>();

        let successful_loads = recent_results.iter()
            .filter(|r| matches!(r.status, LoadingStatus::Success))
            .count();

        let failed_loads = recent_results.iter()
            .filter(|r| matches!(r.status, LoadingStatus::Error))
            .count();

        let avg_loading_time = if recent_results.is_empty() {
            0.0
        } else {
            recent_results.iter()
                .map(|r| r.duration_ms as f64)
                .sum::<f64>() / recent_results.len() as f64
        };

        LoadingStats {
            total_loaded: loaded.len(),
            active_loads: state.active_loads.len(),
            successful_loads,
            failed_loads,
            avg_loading_time_ms: avg_loading_time,
            pending_reloads: state.pending_reloads.len(),
        }
    }

    /// Get loading history
    pub async fn get_loading_history(&self) -> Vec<ToolLoadingResult> {
        let state = self.loading_state.read().await;
        state.loading_history.clone()
    }

    /// Notify event handlers of tool loaded
    async fn notify_tool_loaded(&self, tool_name: &str, file_path: &PathBuf) {
        let handlers = self.event_handlers.lock().await;
        for handler in handlers.iter() {
            handler.on_tool_loaded(tool_name, file_path);
        }
    }

    /// Notify event handlers of tool load failed
    async fn notify_tool_load_failed(&self, tool_name: &str, file_path: &PathBuf, error: &str) {
        let handlers = self.event_handlers.lock().await;
        for handler in handlers.iter() {
            handler.on_tool_load_failed(tool_name, file_path, error);
        }
    }

    /// Notify event handlers of tool reloaded
    async fn notify_tool_reloaded(&self, tool_name: &str, file_path: &PathBuf) {
        let handlers = self.event_handlers.lock().await;
        for handler in handlers.iter() {
            handler.on_tool_reloaded(tool_name, file_path);
        }
    }

    /// Setup file watcher for hot-reload (if feature enabled)
    #[cfg(feature = "hot-reload")]
    fn setup_file_watcher(&self) -> Result<(), RuneError> {
        use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default(),
        ).map_err(|e| RuneError::HotReloadError {
            message: format!("Failed to create file watcher: {}", e),
            file_path: None,
        })?;

        // Watch all configured directories
        for directory in &self.config.tool_directories {
            if directory.exists() {
                watcher.watch(directory, RecursiveMode::Recursive)
                    .map_err(|e| RuneError::HotReloadError {
                        message: format!("Failed to watch directory {:?}: {}", directory, e),
                        file_path: Some(directory.clone()),
                    })?;
            }
        }

        // Start watching thread
        let loaded_tools = Arc::clone(&self.loaded_tools);
        let file_patterns = self.config.file_patterns.clone();
        let debounce_ms = self.config.hot_reload_debounce_ms;

        tokio::spawn(async move {
            use std::collections::HashMap;
            use std::sync::Mutex;
            use std::time::{Duration, Instant};

            let mut pending_changes: HashMap<PathBuf, Instant> = HashMap::new();
            let pending_changes = Arc::new(Mutex::new(pending_changes));

            // Process file change events
            while let Ok(event) = rx.recv() {
                for path in event.paths {
                    // Check if file matches our patterns
                    if file_patterns.iter().any(|pattern| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| {
                                // Simple pattern matching - in a real implementation,
                                // you'd use proper glob patterns
                                name.contains(pattern.strip_prefix("*").unwrap_or(pattern))
                            })
                            .unwrap_or(false)
                    }) {
                        let now = Instant::now();
                        let mut pending = pending_changes.lock().unwrap();
                        pending.insert(path, now);
                    }
                }
            }
        });

        Ok(())
    }
}

/// Loading statistics
#[derive(Debug, Clone)]
pub struct LoadingStats {
    /// Total number of loaded tools
    pub total_loaded: usize,
    /// Number of active loading operations
    pub active_loads: usize,
    /// Number of successful recent loads
    pub successful_loads: usize,
    /// Number of failed recent loads
    pub failed_loads: usize,
    /// Average loading time in milliseconds
    pub avg_loading_time_ms: f64,
    /// Number of pending reloads
    pub pending_reloads: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContextBuilder, create_safe_context};
    use tempfile::TempDir;
    use std::fs;

    #[tokio::test]
    async fn test_loader_config_default() {
        let config = LoaderConfig::default();
        assert_eq!(config.tool_directories.len(), 3);
        assert!(config.enable_hot_reload);
        assert_eq!(config.hot_reload_debounce_ms, 500);
    }

    #[tokio::test]
    async fn test_tool_loading() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let tool_dir = temp_dir.path().to_path_buf();

        // Create a test tool file
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
        fs::write(&tool_path, tool_source)?;

        // Setup loader
        let config = LoaderConfig {
            tool_directories: vec![tool_dir],
            ..Default::default()
        };

        let context = create_safe_context()?;
        let mut context_manager = ContextManager::new(crate::context::ContextConfig::default());
        let registry = Arc::new(RuneToolRegistry::new()?);

        let loader = ToolLoader::new(config, registry, Arc::new(Mutex::new(context_manager)))?;

        // Load tools
        let loaded_count = loader.load_tools().await?;
        assert_eq!(loaded_count, 1);

        // Check loading stats
        let stats = loader.get_loading_stats().await;
        assert_eq!(stats.total_loaded, 1);
        assert_eq!(stats.successful_loads, 1);

        Ok(())
    }

    #[test]
    fn test_default_event_handler() {
        let handler = DefaultEventHandler;
        // These would log output in a real scenario
        handler.on_tool_loaded("test", &PathBuf::from("test.rn"));
        handler.on_tool_load_failed("test", &PathBuf::from("test.rn"), "error");
        handler.on_tool_reloaded("test", &PathBuf::from("test.rn"));
        handler.on_file_changed(&PathBuf::from("test.rn"));
    }
}