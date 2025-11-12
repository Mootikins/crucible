//! Service Locator Pattern - Non-global dependency management
//!
//! This provides a clean way to manage dependencies without global state.
//! Services are registered in a container and injected where needed.

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use async_trait::async_trait;

/// Service locator that manages all application services
#[derive(Debug)]
pub struct ServiceLocator {
    /// Registered services
    services: RwLock<HashMap<ServiceId, Box<dyn Service + Send + Sync>>>,
    /// Configuration
    config: ServiceLocatorConfig,
}

/// Configuration for service locator
#[derive(Debug, Clone)]
pub struct ServiceLocatorConfig {
    /// Enable service caching
    pub enable_caching: bool,
    /// Track service creation for debugging
    pub track_creation: bool,
}

impl Default for ServiceLocatorConfig {
    fn default() -> Self {
        Self {
            enable_caching: true,
            track_creation: cfg!(debug_assertions),
        }
    }
}

/// Unique identifier for services
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ServiceId {
    /// Tool execution service
    ToolService,
    /// File system service
    FileService,
    /// Configuration service
    ConfigService,
    /// Caching service
    CacheService,
    /// Logging service
    LoggingService,
    /// Custom service with name
    Custom(String),
}

impl std::fmt::Display for ServiceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceId::ToolService => write!(f, "ToolService"),
            ServiceId::FileService => write!(f, "FileService"),
            ServiceId::ConfigService => write!(f, "ConfigService"),
            ServiceId::CacheService => write!(f, "CacheService"),
            ServiceId::LoggingService => write!(f, "LoggingService"),
            ServiceId::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// Base trait for all services
pub trait Service: std::fmt::Debug {
    /// Get service ID
    fn service_id(&self) -> ServiceId;

    /// Initialize the service
    fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Shutdown the service
    fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }

    /// Check if service is healthy
    fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Tool execution service interface
#[async_trait]
pub trait ToolService: Service + Send + Sync {
    /// Execute a tool
    async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<crucible_tools::ToolResult>;

    /// List available tools
    async fn list_tools(&self) -> Result<Vec<String>>;

    /// Check if a tool exists
    async fn has_tool(&self, name: &str) -> bool;

    /// Get tool information
    async fn get_tool_info(&self, name: &str) -> Result<Option<ToolInfo>>;
}

/// Information about a tool
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: Option<String>,
    /// Tool category
    pub category: Option<String>,
    /// Whether the tool is enabled
    pub enabled: bool,
}

/// File system service interface
#[async_trait]
pub trait FileService: Service + Send + Sync {
    /// Read file content
    async fn read_file(&self, path: &PathBuf) -> Result<String>;

    /// Write file content
    async fn write_file(&self, path: &PathBuf, content: &str) -> Result<()>;

    /// List directory contents
    async fn list_directory(&self, path: &PathBuf, recursive: bool) -> Result<Vec<PathBuf>>;

    /// Check if path exists
    async fn path_exists(&self, path: &PathBuf) -> bool;

    /// Get file metadata
    async fn get_metadata(&self, path: &PathBuf) -> Result<FileMetadata>;
}

/// File metadata information
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,
    /// Last modified time
    pub modified: std::time::SystemTime,
    /// Whether it's a directory
    pub is_directory: bool,
    /// Whether it's a file
    pub is_file: bool,
}

/// Configuration service interface
pub trait ConfigService: Service + Send + Sync {
    /// Get configuration value
    fn get_config(&self, key: &str) -> Option<String>;

    /// Set configuration value
    fn set_config(&mut self, key: String, value: String);

    /// Get kiln path
    fn get_kiln_path(&self) -> Option<PathBuf>;

    /// Set kiln path
    fn set_kiln_path(&mut self, path: PathBuf);
}

impl ServiceLocator {
    /// Create a new service locator
    pub fn new() -> Self {
        Self::with_config(ServiceLocatorConfig::default())
    }

    /// Create a service locator with custom configuration
    pub fn with_config(config: ServiceLocatorConfig) -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Register a service
    pub fn register<S: Service + Send + Sync + 'static>(&self, service: S) -> Result<()> {
        let id = service.service_id();
        let mut services = self.services.write().unwrap();

        if services.contains_key(&id) {
            return Err(anyhow::anyhow!("Service {} already registered", id));
        }

        if self.config.track_creation {
            tracing::debug!("Registering service: {}", id);
        }

        services.insert(id, Box::new(service));
        Ok(())
    }

    /// Get a service by type
    pub fn get<T: Service + Send + Sync + 'static>(&self) -> Result<Arc<T>> {
        let id = self.get_service_id::<T>();
        let services = self.services.read().unwrap();

        let service = services
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Service {} not found", id))?;

        // Try to downcast the service
        let service = service.as_ref() as &dyn std::any::Any;
        service
            .downcast_ref::<T>()
            .map(|s| Arc::new(s.clone()))
            .ok_or_else(|| anyhow::anyhow!("Failed to downcast service {}", id))
    }

    /// Get a service by ID (for dynamic access)
    pub fn get_by_id(&self, id: ServiceId) -> Result<Arc<dyn Service + Send + Sync>> {
        let services = self.services.read().unwrap();

        let service = services
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("Service {} not found", id))?;

        // Note: This is a simplified approach. In practice, you'd need a more
        sophisticated way to handle downcasting for dynamic access.
        Err(anyhow::anyhow!("Dynamic service access requires explicit type information"))
    }

    /// Check if a service is registered
    pub fn has<T: Service + Send + Sync + 'static>(&self) -> bool {
        let id = self.get_service_id::<T>();
        let services = self.services.read().unwrap();
        services.contains_key(&id)
    }

    /// Initialize all services
    pub fn initialize_all(&self) -> Result<()> {
        let mut services = self.services.write().unwrap();

        for (id, service) in services.iter_mut() {
            if let Err(e) = service.initialize() {
                tracing::error!("Failed to initialize service {}: {}", id, e);
                return Err(e);
            }

            if self.config.track_creation {
                tracing::debug!("Initialized service: {}", id);
            }
        }

        Ok(())
    }

    /// Shutdown all services
    pub fn shutdown_all(&self) -> Result<()> {
        let mut services = self.services.write().unwrap();

        for (id, service) in services.iter_mut() {
            if let Err(e) = service.shutdown() {
                tracing::warn!("Failed to shutdown service {}: {}", id, e);
            } else if self.config.track_creation {
                tracing::debug!("Shutdown service: {}", id);
            }
        }

        Ok(())
    }

    /// Health check all services
    pub async fn health_check_all(&self) -> HashMap<ServiceId, bool> {
        let services = self.services.read().unwrap();
        let mut results = HashMap::new();

        for (id, service) in services.iter() {
            match service.health_check() {
                Ok(healthy) => results.insert(id.clone(), healthy),
                Err(e) => {
                    tracing::warn!("Health check failed for {}: {}", id, e);
                    results.insert(id.clone(), false);
                }
            };
        }

        results
    }

    /// List all registered services
    pub fn list_services(&self) -> Vec<ServiceId> {
        let services = self.services.read().unwrap();
        services.keys().cloned().collect()
    }

    /// Helper method to get service ID from type
    fn get_service_id<T: Service + Send + Sync + 'static>(&self) -> ServiceId {
        // This is a simplified approach. In practice, you'd use a more robust
        // way to map types to service IDs, possibly using macros or traits.

        // For now, we'll use a simple type name based approach
        let type_name = std::any::type_name::<T>();
        match type_name {
            name if name.contains("ToolServiceImpl") => ServiceId::ToolService,
            name if name.contains("FileServiceImpl") => ServiceId::FileService,
            name if name.contains("ConfigServiceImpl") => ServiceId::ConfigService,
            _ => ServiceId::Custom(type_name.to_string()),
        }
    }
}

impl Default for ServiceLocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ServiceLocator {
    fn drop(&mut self) {
        if let Err(e) = self.shutdown_all() {
            tracing::warn!("Failed to shutdown all services: {}", e);
        }
    }
}

/// Concrete implementation of ToolService
#[derive(Debug, Clone)]
pub struct ToolServiceImpl {
    service_id: ServiceId,
    registry: Arc<crucible_tools::types_refactored::ToolRegistry>,
}

impl ToolServiceImpl {
    /// Create a new tool service
    pub fn new(kiln_path: PathBuf) -> Self {
        let registry = Arc::new(crucible_tools::types_refactored::ToolRegistry::new());
        Self {
            service_id: ServiceId::ToolService,
            registry,
        }
    }

    /// Create a tool service with existing registry
    pub fn with_registry(registry: Arc<crucible_tools::types_refactored::ToolRegistry>) -> Self {
        Self {
            service_id: ServiceId::ToolService,
            registry,
        }
    }
}

#[async_trait]
impl ToolService for ToolServiceImpl {
    async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        user_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<crucible_tools::ToolResult> {
        self.registry
            .execute_tool(
                tool_name.to_string(),
                parameters,
                user_id,
                session_id,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Tool execution failed: {}", e))
    }

    async fn list_tools(&self) -> Result<Vec<String>> {
        Ok(self.registry.list_tools().await)
    }

    async fn has_tool(&self, name: &str) -> bool {
        self.registry.has_tool(name).await
    }

    async fn get_tool_info(&self, name: &str) -> Result<Option<ToolInfo>> {
        if self.has_tool(name).await {
            Ok(Some(ToolInfo {
                name: name.to_string(),
                description: None,
                category: None,
                enabled: true,
            }))
        } else {
            Ok(None)
        }
    }
}

impl Service for ToolServiceImpl {
    fn service_id(&self) -> ServiceId {
        self.service_id.clone()
    }

    fn initialize(&mut self) -> Result<()> {
        // Load all tools into registry
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            self.registry.load_all_tools().await
        })
    }
}

/// Concrete implementation of ConfigService
#[derive(Debug, Clone)]
pub struct ConfigServiceImpl {
    service_id: ServiceId,
    config: RwLock<HashMap<String, String>>,
    kiln_path: RwLock<Option<PathBuf>>,
}

impl ConfigServiceImpl {
    /// Create a new config service
    pub fn new() -> Self {
        Self {
            service_id: ServiceId::ConfigService,
            config: RwLock::new(HashMap::new()),
            kiln_path: RwLock::new(None),
        }
    }

    /// Create a config service with initial values
    pub fn with_values(initial_config: HashMap<String, String>, kiln_path: Option<PathBuf>) -> Self {
        Self {
            service_id: ServiceId::ConfigService,
            config: RwLock::new(initial_config),
            kiln_path: RwLock::new(kiln_path),
        }
    }
}

impl Service for ConfigServiceImpl {
    fn service_id(&self) -> ServiceId {
        self.service_id.clone()
    }
}

impl ConfigService for ConfigServiceImpl {
    fn get_config(&self, key: &str) -> Option<String> {
        self.config.read().unwrap().get(key).cloned()
    }

    fn set_config(&mut self, key: String, value: String) {
        self.config.write().unwrap().insert(key, value);
    }

    fn get_kiln_path(&self) -> Option<PathBuf> {
        self.kiln_path.read().unwrap().clone()
    }

    fn set_kiln_path(&mut self, path: PathBuf) {
        *self.kiln_path.write().unwrap() = Some(path);
    }
}

/// Builder for setting up a service locator with common services
#[derive(Debug)]
pub struct ServiceLocatorBuilder {
    config: ServiceLocatorConfig,
    services: Vec<Box<dyn Service + Send + Sync>>,
}

impl ServiceLocatorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: ServiceLocatorConfig::default(),
            services: Vec::new(),
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: ServiceLocatorConfig) -> Self {
        self.config = config;
        self
    }

    /// Add a tool service
    pub fn with_tool_service(mut self, kiln_path: PathBuf) -> Self {
        self.services.push(Box::new(ToolServiceImpl::new(kiln_path)));
        self
    }

    /// Add a config service
    pub fn with_config_service(mut self, initial_config: HashMap<String, String>, kiln_path: Option<PathBuf>) -> Self {
        self.services.push(Box::new(ConfigServiceImpl::with_values(initial_config, kiln_path)));
        self
    }

    /// Add a custom service
    pub fn with_service<S: Service + Send + Sync + 'static>(mut self, service: S) -> Self {
        self.services.push(Box::new(service));
        self
    }

    /// Build the service locator
    pub fn build(self) -> Result<ServiceLocator> {
        let locator = ServiceLocator::with_config(self.config);

        // Register all services
        for service in self.services {
            let id = service.service_id();
            locator.register_any(service)
                .map_err(|e| anyhow::anyhow!("Failed to register service {}: {}", id, e))?;
        }

        // Initialize all services
        locator.initialize_all()?;

        Ok(locator)
    }
}

// Helper method for ServiceLocator to register any service
impl ServiceLocator {
    fn register_any(&self, service: Box<dyn Service + Send + Sync>) -> Result<()> {
        let id = service.service_id();
        let mut services = self.services.write().unwrap();
        services.insert(id, service);
        Ok(())
    }
}

impl Default for ServiceLocatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_service_locator() {
        let locator = ServiceLocator::new();

        // Should be empty initially
        assert_eq!(locator.list_services().len(), 0);

        // Register a service
        let tool_service = ToolServiceImpl::new(std::env::temp_dir());
        locator.register(tool_service.clone()).unwrap();

        // Should have one service
        assert_eq!(locator.list_services().len(), 1);
        assert!(locator.has::<ToolServiceImpl>());

        // Get the service
        let retrieved: Arc<ToolServiceImpl> = locator.get().unwrap();
        assert_eq!(retrieved.service_id(), tool_service.service_id());
    }

    #[tokio::test]
    async fn test_service_builder() {
        let mut config = HashMap::new();
        config.insert("test_key".to_string(), "test_value".to_string());

        let locator = ServiceLocatorBuilder::new()
            .with_config_service(config.clone(), Some(std::env::temp_dir()))
            .with_tool_service(std::env::temp_dir())
            .build()
            .unwrap();

        // Should have both services
        assert_eq!(locator.list_services().len(), 2);
        assert!(locator.has::<ConfigServiceImpl>());
        assert!(locator.has::<ToolServiceImpl>());

        // Test the config service
        let config_service: Arc<ConfigServiceImpl> = locator.get().unwrap();
        assert_eq!(config_service.get_config("test_key"), Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_tool_service() {
        let service = ToolServiceImpl::new(std::env::temp_dir());

        // Initialize the service
        let mut init_service = service.clone();
        init_service.initialize().unwrap();

        // Should have tools
        let tools = service.list_tools().await.unwrap();
        assert!(!tools.is_empty());

        // Should be able to execute a tool
        let result = service
            .execute_tool("system_info", serde_json::json!({}), None, None)
            .await
            .unwrap();

        assert!(result.success);
    }
}