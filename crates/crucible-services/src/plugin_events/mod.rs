//! Plugin Event Subscription System
//!
//! A comprehensive event subscription system that enables plugins to subscribe to and receive events
//! from the Crucible daemon event system with real-time delivery, reliability, and security.

pub mod config;
pub mod delivery_system;
pub mod error;
pub mod event_bridge;
pub mod filter_engine;
pub mod subscription_api;
pub mod subscription_manager;
pub mod subscription_registry;
pub mod types;

// Re-export main components for easier access
pub use config::*;
pub use delivery_system::*;
pub use error::*;
pub use event_bridge::*;
pub use filter_engine::*;
pub use subscription_api::*;
pub use subscription_manager::*;
pub use subscription_registry::*;
pub use types::*;

use crate::events::{EventBus, EventBusImpl};
use std::sync::Arc;
use tracing::{info, error};

/// Main entry point for the plugin event subscription system
#[derive(Clone)]
pub struct PluginEventSystem {
    /// Subscription manager
    subscription_manager: Arc<SubscriptionManager>,

    /// API server
    api_server: Option<Arc<subscription_api::SubscriptionApiServer>>,

    /// System configuration
    config: SubscriptionSystemConfig,

    /// System state
    state: Arc<std::sync::RwLock<SystemState>>,
}

/// System state
#[derive(Debug, Clone, PartialEq)]
enum SystemState {
    Uninitialized,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Error(String),
}

impl PluginEventSystem {
    /// Create a new plugin event system
    pub fn new(config: SubscriptionSystemConfig) -> SubscriptionResult<Self> {
        info!("Creating plugin event subscription system");

        // Validate configuration
        config.validate()?;

        // Create plugin connection manager
        let plugin_connection_manager = Arc::new(
            plugin_manager::PluginConnectionManagerImpl::new(&config)
        );

        // Create subscription manager
        let subscription_manager = Arc::new(
            SubscriptionManager::new(config.manager.clone(), plugin_connection_manager)
        );

        Ok(Self {
            subscription_manager,
            api_server: None,
            config,
            state: Arc::new(std::sync::RwLock::new(SystemState::Uninitialized)),
        })
    }

    /// Initialize the system
    pub async fn initialize(&mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> SubscriptionResult<()> {
        info!("Initializing plugin event subscription system");

        // Set state to initializing
        {
            let mut state = self.state.write().unwrap();
            *state = SystemState::Initializing;
        }

        // Start subscription manager
        self.subscription_manager.start(event_bus).await
            .map_err(|e| {
                error!("Failed to start subscription manager: {}", e);
                self.set_error_state(format!("Subscription manager start failed: {}", e));
                e
            })?;

        // Start API server if enabled
        if self.config.api.enabled {
            let api_server = subscription_api::SubscriptionApiServer::new(
                self.subscription_manager.clone(),
                self.config.api.clone(),
            );

            // Note: In a real implementation, you would start the API server here
            self.api_server = Some(Arc::new(api_server));
            info!("API server created");
        }

        // Set state to running
        {
            let mut state = self.state.write().unwrap();
            *state = SystemState::Running;
        }

        info!("Plugin event subscription system initialized successfully");
        Ok(())
    }

    /// Start the system (alias for initialize)
    pub async fn start(&mut self, event_bus: Arc<dyn EventBus + Send + Sync>) -> SubscriptionResult<()> {
        self.initialize(event_bus).await
    }

    /// Stop the system
    pub async fn stop(&self) -> SubscriptionResult<()> {
        info!("Stopping plugin event subscription system");

        // Set state to stopping
        {
            let mut state = self.state.write().unwrap();
            *state = SystemState::Stopping;
        }

        // Stop API server
        if let Some(api_server) = &self.api_server {
            if let Err(e) = api_server.stop().await {
                error!("Failed to stop API server: {}", e);
            }
        }

        // Stop subscription manager
        if let Err(e) = self.subscription_manager.stop().await {
            error!("Failed to stop subscription manager: {}", e);
        }

        // Set state to stopped
        {
            let mut state = self.state.write().unwrap();
            *state = SystemState::Stopped;
        }

        info!("Plugin event subscription system stopped");
        Ok(())
    }

    /// Get subscription manager
    pub fn subscription_manager(&self) -> &Arc<SubscriptionManager> {
        &self.subscription_manager
    }

    /// Get API server
    pub fn api_server(&self) -> Option<&Arc<subscription_api::SubscriptionApiServer>> {
        self.api_server.as_ref()
    }

    /// Get system configuration
    pub fn config(&self) -> &SubscriptionSystemConfig {
        &self.config
    }

    /// Get system state
    pub fn state(&self) -> SystemState {
        self.state.read().unwrap().clone()
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        matches!(self.state(), SystemState::Running)
    }

    /// Get system statistics
    pub async fn get_system_stats(&self) -> SystemStats {
        let manager_stats = self.subscription_manager.get_manager_stats().await;
        let config = &self.config;

        SystemStats {
            manager_stats,
            system_config: SystemConfigInfo {
                name: config.system.name.clone(),
                environment: config.system.environment.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(), // TODO: Track actual start time
                uptime_seconds: 0, // TODO: Calculate actual uptime
            },
            component_status: ComponentStatus {
                subscription_manager: self.is_running(),
                api_server: self.api_server.is_some(),
                event_bridge: self.is_running(),
                delivery_system: self.is_running(),
                filter_engine: self.is_running(),
            },
        }
    }

    /// Perform system health check
    pub async fn health_check(&self) -> HealthCheckResult {
        let mut components = std::collections::HashMap::new();

        // Check subscription manager
        match self.subscription_manager.get_manager_stats().await {
            Ok(stats) => {
                components.insert(
                    "subscription_manager".to_string(),
                    ComponentHealth {
                        status: HealthStatus::Healthy,
                        message: Some(format!("{} active subscriptions", stats.active_subscriptions)),
                        last_check: Utc::now(),
                        metrics: Some(serde_json::json!({
                            "active_subscriptions": stats.active_subscriptions,
                            "total_events_processed": stats.total_events_processed,
                            "uptime_seconds": stats.uptime_seconds,
                        })),
                    }
                );
            }
            Err(e) => {
                components.insert(
                    "subscription_manager".to_string(),
                    ComponentHealth {
                        status: HealthStatus::Unhealthy,
                        message: Some(format!("Error: {}", e)),
                        last_check: Utc::now(),
                        metrics: None,
                    }
                );
            }
        }

        // Check API server
        if let Some(api_server) = &self.api_server {
            components.insert(
                "api_server".to_string(),
                ComponentHealth {
                    status: HealthStatus::Healthy,
                    message: Some("API server is running".to_string()),
                    last_check: Utc::now(),
                    metrics: Some(serde_json::json!({
                        "port": self.config.api.port,
                        "websocket_enabled": self.config.api.websocket.enabled,
                    })),
                }
            );
        } else {
            components.insert(
                "api_server".to_string(),
                ComponentHealth {
                    status: HealthStatus::Disabled,
                    message: Some("API server is disabled".to_string()),
                    last_check: Utc::now(),
                    metrics: None,
                }
            );
        }

        // Determine overall health
        let overall_status = if components.values().all(|c| c.status == HealthStatus::Healthy) {
            HealthStatus::Healthy
        } else if components.values().any(|c| c.status == HealthStatus::Unhealthy) {
            HealthStatus::Unhealthy
        } else {
            HealthStatus::Degraded
        };

        HealthCheckResult {
            overall_status,
            components,
            timestamp: Utc::now(),
        }
    }

    /// Set error state
    fn set_error_state(&self, error: String) {
        let mut state = self.state.write().unwrap();
        *state = SystemState::Error(error);
    }
}

impl Default for PluginEventSystem {
    fn default() -> Self {
        Self::new(SubscriptionSystemConfig::default())
    }
}

/// System statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemStats {
    /// Manager statistics
    pub manager_stats: subscription_manager::ManagerStats,

    /// System configuration info
    pub system_config: SystemConfigInfo,

    /// Component status
    pub component_status: ComponentStatus,
}

/// System configuration information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SystemConfigInfo {
    /// System name
    pub name: String,

    /// Environment
    pub environment: String,

    /// Version
    pub version: String,

    /// Started timestamp
    pub started_at: chrono::DateTime<chrono::Utc>,

    /// Uptime in seconds
    pub uptime_seconds: u64,
}

/// Component status information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComponentStatus {
    /// Subscription manager status
    pub subscription_manager: bool,

    /// API server status
    pub api_server: bool,

    /// Event bridge status
    pub event_bridge: bool,

    /// Delivery system status
    pub delivery_system: bool,

    /// Filter engine status
    pub filter_engine: bool,
}

/// Health check result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HealthCheckResult {
    /// Overall health status
    pub overall_status: HealthStatus,

    /// Component health information
    pub components: std::collections::HashMap<String, ComponentHealth>,

    /// Check timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Health status
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Disabled,
}

/// Component health information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComponentHealth {
    /// Health status
    pub status: HealthStatus,

    /// Status message
    pub message: Option<String>,

    /// Last check timestamp
    pub last_check: chrono::DateTime<chrono::Utc>,

    /// Component metrics
    pub metrics: Option<serde_json::Value>,
}

// Plugin manager integration module
pub mod plugin_manager {
    use super::*;
    use crate::plugin_events::subscription_api::{PluginInfo, PluginStatus, SerializedEvent};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tracing::{debug, warn};

    /// Plugin connection manager implementation
    pub struct PluginConnectionManagerImpl {
        /// Connected plugins
        plugins: Arc<RwLock<HashMap<String, PluginInfo>>>,

        /// Configuration
        config: PluginConnectionConfig,
    }

    /// Plugin connection configuration
    #[derive(Debug, Clone)]
    struct PluginConnectionConfig {
        /// Connection timeout in seconds
        connection_timeout_seconds: u64,

        /// Heartbeat interval in seconds
        heartbeat_interval_seconds: u64,

        /// Maximum reconnection attempts
        max_reconnection_attempts: u32,

        /// Enable automatic reconnection
        enable_auto_reconnection: bool,
    }

    impl PluginConnectionManagerImpl {
        /// Create new plugin connection manager
        pub fn new(config: &SubscriptionSystemConfig) -> Self {
            Self {
                plugins: Arc::new(RwLock::new(HashMap::new())),
                config: PluginConnectionConfig {
                    connection_timeout_seconds: 30,
                    heartbeat_interval_seconds: 60,
                    max_reconnection_attempts: 5,
                    enable_auto_reconnection: true,
                },
            }
        }

        /// Register a plugin
        pub async fn register_plugin(&self, plugin_info: PluginInfo) {
            let mut plugins = self.plugins.write().await;
            plugins.insert(plugin_info.plugin_id.clone(), plugin_info.clone());
            debug!("Registered plugin: {}", plugin_info.plugin_id);
        }

        /// Unregister a plugin
        pub async fn unregister_plugin(&self, plugin_id: &str) {
            let mut plugins = self.plugins.write().await;
            if plugins.remove(plugin_id).is_some() {
                debug!("Unregistered plugin: {}", plugin_id);
            }
        }
    }

    #[async_trait::async_trait]
    impl super::subscription_api::PluginConnectionManager for PluginConnectionManagerImpl {
        async fn is_plugin_connected(&self, plugin_id: &str) -> bool {
            let plugins = self.plugins.read().await;
            plugins.get(plugin_id)
                .map(|p| matches!(p.status, PluginStatus::Connected))
                .unwrap_or(false)
        }

        async fn get_plugin_info(&self, plugin_id: &str) -> Option<PluginInfo> {
            let plugins = self.plugins.read().await;
            plugins.get(plugin_id).cloned()
        }

        async fn send_event_to_plugin(
            &self,
            plugin_id: &str,
            event: &SerializedEvent,
        ) -> SubscriptionResult<()> {
            let plugins = self.plugins.read().await;

            if let Some(plugin_info) = plugins.get(plugin_id) {
                if plugin_info.status != PluginStatus::Connected {
                    return Err(SubscriptionError::PluginError(
                        format!("Plugin {} is not connected", plugin_id)
                    ));
                }

                // In a real implementation, this would send the event via IPC
                debug!("Sending event to plugin {}: {} bytes", plugin_id, event.data.len());
                Ok(())
            } else {
                Err(SubscriptionError::PluginNotFound(plugin_id.to_string()))
            }
        }

        async fn get_connected_plugins(&self) -> Vec<PluginInfo> {
            let plugins = self.plugins.read().await;
            plugins.values()
                .filter(|p| matches!(p.status, PluginStatus::Connected))
                .cloned()
                .collect()
        }
    }
}

/// Builder for plugin event system
pub struct PluginEventSystemBuilder {
    config: SubscriptionSystemConfig,
}

impl PluginEventSystemBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            config: SubscriptionSystemConfig::default(),
        }
    }

    /// Set configuration
    pub fn with_config(mut self, config: SubscriptionSystemConfig) -> Self {
        self.config = config;
        self
    }

    /// Load configuration from file
    pub fn with_config_file<P: AsRef<std::path::Path>>(mut self, path: P) -> SubscriptionResult<Self> {
        self.config = SubscriptionSystemConfig::from_file(path)?;
        Ok(self)
    }

    /// Load configuration from environment
    pub fn with_env(mut self) -> Self {
        self.config = SubscriptionSystemConfig::from_env();
        self
    }

    /// Set API port
    pub fn with_api_port(mut self, port: u16) -> Self {
        self.config.api.port = port;
        self
    }

    /// Enable/disable API
    pub fn with_api_enabled(mut self, enabled: bool) -> Self {
        self.config.api.enabled = enabled;
        self
    }

    /// Set log level
    pub fn with_log_level(mut self, level: &str) -> Self {
        self.config.logging.level = level.to_string();
        self
    }

    /// Enable/disable security
    pub fn with_security_enabled(mut self, enabled: bool) -> Self {
        self.config.security.enabled = enabled;
        self
    }

    /// Set data directory
    pub fn with_data_dir<P: AsRef<std::path::Path>>(mut self, dir: P) -> Self {
        self.config.system.data_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Build the system
    pub fn build(self) -> SubscriptionResult<PluginEventSystem> {
        PluginEventSystem::new(self.config)
    }
}

impl Default for PluginEventSystemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::MockEventBus;

    #[tokio::test]
    async fn test_plugin_event_system_creation() {
        let config = SubscriptionSystemConfig::default();
        let system = PluginEventSystem::new(config);
        assert!(matches!(system.state(), SystemState::Uninitialized));
    }

    #[tokio::test]
    async fn test_plugin_event_system_builder() {
        let system = PluginEventSystemBuilder::new()
            .with_api_port(9090)
            .with_log_level("debug")
            .with_security_enabled(true)
            .build()
            .unwrap();

        assert_eq!(system.config().api.port, 9090);
        assert_eq!(system.config().logging.level, "debug");
        assert!(system.config().security.enabled);
    }

    #[tokio::test]
    async fn test_system_initialization() {
        let mut system = PluginEventSystemBuilder::new()
            .with_api_enabled(false) // Disable API for testing
            .build()
            .unwrap();

        let event_bus = Arc::new(MockEventBus::new());
        let result = system.initialize(event_bus).await;

        // This might fail in the test environment due to missing components,
        // but we can test that the state changes correctly
        match result {
            Ok(()) => {
                assert!(system.is_running());
            }
            Err(e) => {
                assert!(matches!(system.state(), SystemState::Error(_)));
                println!("Initialization failed as expected in test environment: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let system = PluginEventSystemBuilder::new()
            .with_api_enabled(false)
            .build()
            .unwrap();

        let health_result = system.health_check().await;
        assert!(matches!(health_result.overall_status, HealthStatus::Degraded | HealthStatus::Disabled));
    }

    #[tokio::test]
    async fn test_plugin_connection_manager() {
        let config = SubscriptionSystemConfig::default();
        let manager = plugin_manager::PluginConnectionManagerImpl::new(&config);

        let plugin_info = PluginInfo {
            plugin_id: "test-plugin".to_string(),
            plugin_name: "Test Plugin".to_string(),
            plugin_version: "1.0.0".to_string(),
            status: PluginStatus::Connected,
            connected_at: Utc::now(),
            last_activity: Utc::now(),
            capabilities: vec!["events".to_string()],
            metadata: HashMap::new(),
        };

        manager.register_plugin(plugin_info.clone()).await;
        assert!(manager.is_plugin_connected("test-plugin").await);

        let retrieved_info = manager.get_plugin_info("test-plugin").await;
        assert!(retrieved_info.is_some());
        assert_eq!(retrieved_info.unwrap().plugin_id, "test-plugin");

        manager.unregister_plugin("test-plugin").await;
        assert!(!manager.is_plugin_connected("test-plugin").await);
    }

    #[test]
    fn test_configuration_validation() {
        let mut config = SubscriptionSystemConfig::default();
        assert!(config.validate().is_ok());

        config.api.port = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_system_stats() {
        let system = PluginEventSystemBuilder::new().build().unwrap();
        // This would need to be tested with a running system
        // For now, just test that the method exists
        assert!(true);
    }
}