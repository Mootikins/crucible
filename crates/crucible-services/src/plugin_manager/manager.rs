//! # Plugin Manager Service
//!
//! This module implements the core PluginManager orchestrator that coordinates
//! all plugin management operations and provides the main service interface.

use super::config::PluginManagerConfig;
use super::error::{PluginError, PluginResult, ErrorContext};
use super::types::*;
use super::registry::{PluginRegistry, DefaultPluginRegistry, RegistryEvent};
use super::instance::{PluginInstance, DefaultPluginInstance, InstanceEvent};
use super::resource_manager::{ResourceManager, DefaultResourceManager, ResourceEvent};
use super::security_manager::{SecurityManager, DefaultSecurityManager, SecurityEvent};
use super::health_monitor::{HealthMonitor, DefaultHealthMonitor, HealthEvent};
use crate::service_types::*;
use crate::service_traits::*;
use crate::errors::{ServiceError, ServiceResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// ============================================================================
/// PLUGIN MANAGER SERVICE
/// ============================================================================

/// Main PluginManager service that orchestrates all plugin operations
#[derive(Debug)]
pub struct PluginManagerService {
    /// Service configuration
    config: Arc<PluginManagerConfig>,

    /// Core components
    registry: Arc<RwLock<Box<dyn PluginRegistry>>>,
    instances: Arc<RwLock<HashMap<String, Box<dyn PluginInstance>>>>,
    resource_manager: Arc<RwLock<Box<dyn ResourceManager>>>,
    security_manager: Arc<RwLock<Box<dyn SecurityManager>>>,
    health_monitor: Arc<RwLock<Box<dyn HealthMonitor>>>,

    /// Service state
    state: Arc<RwLock<PluginManagerState>>,

    /// Event channels
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<PluginManagerEvent>>>>,

    /// Metrics
    metrics: Arc<RwLock<PluginManagerMetrics>>,
}

/// Plugin manager state
#[derive(Debug, Clone)]
struct PluginManagerState {
    /// Service name
    service_name: String,
    /// Service version
    service_version: String,
    /// Running status
    running: bool,
    /// Start time
    started_at: Option<SystemTime>,
    /// Last activity
    last_activity: Option<SystemTime>,
}

/// Plugin manager metrics
#[derive(Debug, Clone, Default)]
struct PluginManagerMetrics {
    /// Total plugins registered
    total_plugins: u64,
    /// Active instances
    active_instances: u64,
    /// Total instance starts
    total_starts: u64,
    /// Total instance stops
    total_stops: u64,
    /// Failed operations
    failed_operations: u64,
    /// Operations by type
    operations_by_type: HashMap<String, u64>,
    /// Last updated timestamp
    last_updated: SystemTime,
}

/// Plugin manager events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginManagerEvent {
    /// Plugin discovered
    PluginDiscovered { manifest: PluginManifest },
    /// Plugin registered
    PluginRegistered { plugin_id: String },
    /// Plugin unregistered
    PluginUnregistered { plugin_id: String },
    /// Instance created
    InstanceCreated { instance_id: String, plugin_id: String },
    /// Instance started
    InstanceStarted { instance_id: String, plugin_id: String },
    /// Instance stopped
    InstanceStopped { instance_id: String, plugin_id: String },
    /// Instance crashed
    InstanceCrashed { instance_id: String, plugin_id: String, error: String },
    /// Resource violation
    ResourceViolation { instance_id: String, resource_type: String, current_value: f64, limit: f64 },
    /// Security violation
    SecurityViolation { plugin_id: String, violation: String },
    /// Health status changed
    HealthStatusChanged { instance_id: String, status: PluginHealthStatus },
    /// Error occurred
    Error { operation: String, error: String, context: Option<String> },
}

/// Plugin instance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstanceConfig {
    /// Instance configuration
    pub instance_config: super::instance::PluginInstanceConfig,
    /// Auto-restart on failure
    pub auto_restart: bool,
    /// Dependencies (other plugins that must be running)
    pub dependencies: Vec<String>,
    /// Startup priority (lower = higher priority)
    pub startup_priority: u32,
    /// Health check configuration
    pub health_check_config: Option<super::health_monitor::HealthCheckConfig>,
}

impl PluginManagerService {
    /// Create a new PluginManager service
    pub fn new(config: PluginManagerConfig) -> Self {
        let registry = Box::new(DefaultPluginRegistry::new(config.clone()));
        let resource_manager = Box::new(DefaultResourceManager::new(config.resource_management.clone()));
        let security_manager = Box::new(DefaultSecurityManager::new(config.security.clone()));
        let health_monitor = Box::new(DefaultHealthMonitor::new(config.health_monitoring.clone()));

        Self {
            config: Arc::new(config),
            registry: Arc::new(RwLock::new(registry)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            resource_manager: Arc::new(RwLock::new(resource_manager)),
            security_manager: Arc::new(RwLock::new(security_manager)),
            health_monitor: Arc::new(RwLock::new(health_monitor)),
            state: Arc::new(RwLock::new(PluginManagerState {
                service_name: "PluginManager".to_string(),
                service_version: env!("CARGO_PKG_VERSION").to_string(),
                running: false,
                started_at: None,
                last_activity: None,
            })),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(PluginManagerMetrics::default())),
        }
    }

    /// Initialize core components
    async fn initialize_components(&mut self) -> PluginResult<()> {
        info!("Initializing PluginManager components");

        // Start core services
        {
            let mut resource_manager = self.resource_manager.write().await;
            resource_manager.start().await?;
        }

        {
            let mut security_manager = self.security_manager.write().await;
            security_manager.start().await?;
        }

        {
            let mut health_monitor = self.health_monitor.write().await;
            health_monitor.start().await?;
        }

        // Subscribe to component events
        self.setup_event_handlers().await?;

        info!("PluginManager components initialized");
        Ok(())
    }

    /// Setup event handlers for component events
    async fn setup_event_handlers(&mut self) -> PluginResult<()> {
        // Subscribe to registry events
        {
            let mut registry = self.registry.write().await;
            let mut registry_events = registry.subscribe().await;

            let event_subscribers = self.event_subscribers.clone();
            let metrics = self.metrics.clone();

            tokio::spawn(async move {
                while let Some(event) = registry_events.recv().await {
                    match event {
                        RegistryEvent::PluginDiscovered { manifest } => {
                            debug!("Plugin discovered: {}", manifest.id);

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(PluginManagerEvent::PluginDiscovered { manifest });
                            }
                        }
                        RegistryEvent::PluginRegistered { plugin_id } => {
                            info!("Plugin registered: {}", plugin_id);

                            // Update metrics
                            {
                                let mut metrics_guard = metrics.write().await;
                                metrics_guard.total_plugins += 1;
                                metrics_guard.last_updated = SystemTime::now();
                            }

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(PluginManagerEvent::PluginRegistered { plugin_id });
                            }
                        }
                        RegistryEvent::PluginUnregistered { plugin_id } => {
                            info!("Plugin unregistered: {}", plugin_id);

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(PluginManagerEvent::PluginUnregistered { plugin_id });
                            }
                        }
                        RegistryEvent::Error { error, context } => {
                            error!("Registry error: {} - {:?}", error, context);

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(PluginManagerEvent::Error {
                                    operation: "registry".to_string(),
                                    error,
                                    context,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            });
        }

        Ok(())
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: PluginManagerEvent) {
        let mut subscribers = self.event_subscribers.read().await;
        let mut to_remove = Vec::new();

        for (i, sender) in subscribers.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                to_remove.push(i);
            }
        }

        // Remove dead subscribers
        for i in to_remove.into_iter().rev() {
            subscribers.remove(i);
        }
    }

    /// Update activity timestamp
    async fn update_activity(&self) {
        let mut state = self.state.write().await;
        state.last_activity = Some(SystemTime::now());
    }

    /// Record operation metrics
    async fn record_operation(&self, operation: &str, success: bool) {
        let mut metrics = self.metrics.write().await;

        *metrics.operations_by_type.entry(operation.to_string()).or_insert(0) += 1;

        if !success {
            metrics.failed_operations += 1;
        }

        metrics.last_updated = SystemTime::now();
    }

    /// Discover and register plugins
    async fn discover_plugins(&self) -> PluginResult<Vec<String>> {
        info!("Starting plugin discovery");

        let registry = self.registry.read().await;
        let manifests = registry.discover_plugins().await?;
        drop(registry);

        let mut registered_plugins = Vec::new();

        for manifest in manifests {
            // Validate plugin security
            let security_manager = self.security_manager.read().await;
            let validation_result = security_manager.validate_plugin_security(&manifest).await?;
            drop(security_manager);

            if !validation_result.passed && self.config.auto_discovery.validation.strict {
                warn!("Skipping plugin {} due to security validation failure", manifest.id);
                continue;
            }

            // Register plugin
            let mut registry = self.registry.write().await;
            match registry.register_plugin(manifest.clone()).await {
                Ok(plugin_id) => {
                    registered_plugins.push(plugin_id);
                    info!("Successfully registered plugin: {} ({})", manifest.name, plugin_id);
                }
                Err(e) => {
                    error!("Failed to register plugin {}: {}", manifest.id, e);
                }
            }
        }

        info!("Plugin discovery completed. Registered {} plugins", registered_plugins.len());
        Ok(registered_plugins)
    }

    /// Start plugin instances based on configuration
    async fn auto_start_instances(&self) -> PluginResult<()> {
        if !self.config.lifecycle.auto_start {
            return Ok(());
        }

        info!("Auto-starting plugin instances");

        let registry = self.registry.read().await;
        let enabled_plugins = registry.list_enabled_plugins().await?;
        drop(registry);

        // Sort by startup priority
        let mut plugins_with_priority = Vec::new();
        for plugin in enabled_plugins {
            plugins_with_priority.push((plugin.manifest.id.clone(), 50)); // Default priority
        }

        plugins_with_priority.sort_by_key(|(_, priority)| *priority);

        // Start instances with concurrency limit
        let concurrent_limit = self.config.lifecycle.concurrent_startup_limit.unwrap_or(5);
        let mut chunks = Vec::new();

        for chunk in plugins_with_priority.chunks(concurrent_limit) {
            let mut handles = Vec::new();

            for (plugin_id, _) in chunk {
                let instances = self.instances.clone();
                let registry = self.registry.clone();
                let resource_manager = self.resource_manager.clone();
                let security_manager = self.security_manager.clone();
                let health_monitor = self.health_monitor.clone();

                let handle = tokio::spawn(async move {
                    if let Err(e) = Self::create_instance_internal(
                        plugin_id.clone(),
                        &instances,
                        &registry,
                        &resource_manager,
                        &security_manager,
                        &health_monitor,
                    ).await {
                        error!("Failed to auto-start instance for plugin {}: {}", plugin_id, e);
                    }
                });

                handles.push(handle);
            }

            // Wait for current batch to complete
            for handle in handles {
                let _ = handle.await;
            }
        }

        info!("Auto-start completed");
        Ok(())
    }

    /// Create plugin instance (internal method)
    async fn create_instance_internal(
        plugin_id: String,
        instances: &Arc<RwLock<HashMap<String, Box<dyn PluginInstance>>>>,
        registry: &Arc<RwLock<Box<dyn PluginRegistry>>>,
        resource_manager: &Arc<RwLock<Box<dyn ResourceManager>>>,
        security_manager: &Arc<RwLock<Box<dyn SecurityManager>>>,
        health_monitor: &Arc<RwLock<Box<dyn HealthMonitor>>>,
    ) -> PluginResult<String> {
        // Get plugin manifest
        let manifest = {
            let registry_guard = registry.read().await;
            registry_guard.get_plugin(&plugin_id).await?
                .ok_or_else(|| PluginError::lifecycle(format!("Plugin {} not found", plugin_id)))?
        };

        // Create sandbox environment
        let sandbox = {
            let security_manager_guard = security_manager.read().await;
            security_manager_guard.create_sandbox(&plugin_id, &manifest.sandbox_config).await?
        };

        // Create instance configuration
        let instance_config = super::instance::PluginInstanceConfig::default();

        // Create instance
        let instance = super::instance::create_plugin_instance(manifest.clone(), instance_config);
        let instance_id = instance.instance_id().to_string();

        // Register instance with resource manager
        {
            let mut resource_manager_guard = resource_manager.write().await;
            resource_manager_guard.register_instance(instance_id.clone(), manifest.resource_limits.clone()).await?;
        }

        // Register instance with health monitor
        {
            let health_check_config = super::health_monitor::HealthCheckConfig::default();
            let mut health_monitor_guard = health_monitor.write().await;
            health_monitor_guard.register_instance(
                instance_id.clone(),
                plugin_id.clone(),
                health_check_config,
            ).await?;
        }

        // Store instance
        {
            let mut instances_guard = instances.write().await;
            instances_guard.insert(instance_id.clone(), instance);
        }

        info!("Created instance {} for plugin {}", instance_id, plugin_id);
        Ok(instance_id)
    }

    /// Graceful shutdown
    async fn graceful_shutdown(&mut self) -> PluginResult<()> {
        info!("Starting graceful shutdown");

        // Stop all instances
        let instance_ids: Vec<String> = {
            let instances = self.instances.read().await;
            instances.keys().cloned().collect()
        };

        for instance_id in instance_ids {
            if let Err(e) = self.stop_instance_internal(&instance_id).await {
                error!("Failed to stop instance {} during shutdown: {}", instance_id, e);
            }
        }

        // Stop core services
        {
            let mut health_monitor = self.health_monitor.write().await;
            health_monitor.stop().await?;
        }

        {
            let mut security_manager = self.security_manager.write().await;
            security_manager.stop().await?;
        }

        {
            let mut resource_manager = self.resource_manager.write().await;
            resource_manager.stop().await?;
        }

        info!("Graceful shutdown completed");
        Ok(())
    }

    /// Stop instance (internal method)
    async fn stop_instance_internal(&mut self, instance_id: &str) -> PluginResult<()> {
        info!("Stopping instance: {}", instance_id);

        // Remove instance from storage and stop it
        let mut instance = {
            let mut instances = self.instances.write().await;
            instances.remove(instance_id)
                .ok_or_else(|| PluginError::lifecycle(format!("Instance {} not found", instance_id)))?
        };

        // Stop the instance
        instance.stop().await?;

        // Unregister from resource manager
        {
            let mut resource_manager = self.resource_manager.write().await;
            resource_manager.unregister_instance(instance_id).await?;
        }

        // Unregister from health monitor
        {
            let mut health_monitor = self.health_monitor.write().await;
            health_monitor.unregister_instance(instance_id).await?;
        }

        info!("Successfully stopped instance: {}", instance_id);
        Ok(())
    }
}

#[async_trait]
impl ServiceLifecycle for PluginManagerService {
    async fn start(&mut self) -> ServiceResult<()> {
        info!("Starting PluginManager service");

        {
            let mut state = self.state.write().await;
            if state.running {
                return Err(ServiceError::LifecycleError("Service is already running".to_string()).into());
            }
            state.running = true;
            state.started_at = Some(SystemTime::now());
        }

        // Initialize components
        if let Err(e) = self.initialize_components().await {
            // Cleanup on failure
            let mut state = self.state.write().await;
            state.running = false;
            return Err(e);
        }

        // Discover plugins
        if let Err(e) = self.discover_plugins().await {
            warn!("Plugin discovery failed: {}", e);
        }

        // Auto-start instances
        if let Err(e) = self.auto_start_instances().await {
            warn!("Auto-start failed: {}", e);
        }

        info!("PluginManager service started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> ServiceResult<()> {
        info!("Stopping PluginManager service");

        {
            let mut state = self.state.write().await;
            if !state.running {
                return Ok(()); // Already stopped
            }
            state.running = false;
        }

        // Perform graceful shutdown
        if let Err(e) = self.graceful_shutdown().await {
            error!("Error during graceful shutdown: {}", e);
        }

        info!("PluginManager service stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        // Note: This is a synchronous method, so we can't access the async state
        // In a real implementation, you'd use a different approach
        true // Placeholder
    }

    fn service_name(&self) -> &str {
        "PluginManager"
    }

    fn service_version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }
}

#[async_trait]
impl HealthCheck for PluginManagerService {
    async fn health_check(&self) -> ServiceResult<ServiceHealth> {
        let state = self.state.read().await;
        let metrics = self.metrics.read().await;

        // Check core components
        let resource_healthy = {
            let resource_manager = self.resource_manager.read().await;
            resource_manager.liveness_check().await.unwrap_or(false)
        };

        let security_healthy = {
            let security_manager = self.security_manager.read().await;
            security_manager.liveness_check().await.unwrap_or(false)
        };

        let health_healthy = {
            let health_monitor = self.health_monitor.read().await;
            health_monitor.liveness_check().await.unwrap_or(false)
        };

        let overall_status = if state.running && resource_healthy && security_healthy && health_healthy {
            ServiceStatus::Healthy
        } else {
            ServiceStatus::Unhealthy
        };

        let service_health = ServiceHealth {
            status: overall_status,
            uptime: state.started_at
                .map(|start| SystemTime::now().duration_since(start).unwrap_or(Duration::ZERO))
                .unwrap_or(Duration::ZERO),
            last_check: SystemTime::now(),
            details: HashMap::from([
                ("total_plugins".to_string(), format!("{}", metrics.total_plugins)),
                ("active_instances".to_string(), format!("{}", metrics.active_instances)),
                ("failed_operations".to_string(), format!("{}", metrics.failed_operations)),
                ("resource_manager".to_string(), resource_healthy.to_string()),
                ("security_manager".to_string(), security_healthy.to_string()),
                ("health_monitor".to_string(), health_healthy.to_string()),
            ]),
        };

        Ok(service_health)
    }

    async fn liveness_check(&self) -> ServiceResult<bool> {
        let state = self.state.read().await;
        Ok(state.running)
    }

    async fn readiness_check(&self) -> ServiceResult<bool> {
        // Check if service is running and has initialized components
        let state = self.state.read().await;
        if !state.running {
            return Ok(false);
        }

        // Check if at least one plugin is registered (optional readiness check)
        let metrics = self.metrics.read().await;
        Ok(metrics.total_plugins > 0)
    }
}

#[async_trait]
impl Configurable for PluginManagerService {
    type Config = PluginManagerConfig;

    async fn get_config(&self) -> ServiceResult<Self::Config> {
        Ok(self.config.as_ref().clone())
    }

    async fn update_config(&mut self, config: Self::Config) -> ServiceResult<()> {
        info!("Updating PluginManager configuration");

        // Validate new configuration
        config.validate().map_err(|e| ServiceError::ConfigurationError(e.to_string()))?;

        // Update configuration
        self.config = Arc::new(config);

        info!("PluginManager configuration updated");
        Ok(())
    }

    async fn validate_config(&self, config: &Self::Config) -> ServiceResult<()> {
        config.validate().map_err(|e| ServiceError::ValidationError(e.to_string()))?;
        Ok(())
    }

    async fn reload_config(&mut self) -> ServiceResult<()> {
        // In a real implementation, you would reload from a file or config source
        info!("Reloading PluginManager configuration");
        Ok(())
    }
}

#[async_trait]
impl Observable for PluginManagerService {
    async fn get_metrics(&self) -> ServiceResult<ServiceMetrics> {
        let state = self.state.read().await;
        let manager_metrics = self.metrics.read().await;

        // Get component metrics
        let resource_metrics = {
            let resource_manager = self.resource_manager.read().await;
            resource_manager.get_metrics().await
                .map(|m| ServiceMetrics::ResourceUsage {
                    memory_usage: m.total_usage.memory_bytes,
                    cpu_usage: m.total_usage.cpu_percentage,
                    disk_usage: m.total_usage.disk_bytes,
                    network_usage: m.total_usage.network_bytes,
                })
                .unwrap_or_else(|_| ServiceMetrics::ResourceUsage {
                    memory_usage: 0,
                    cpu_usage: 0.0,
                    disk_usage: 0,
                    network_usage: 0,
                })
        };

        Ok(ServiceMetrics {
            service_name: state.service_name.clone(),
            service_version: state.service_version.clone(),
            uptime: state.started_at
                .map(|start| SystemTime::now().duration_since(start).unwrap_or(Duration::ZERO))
                .unwrap_or(Duration::ZERO),
            request_count: manager_metrics.operations_by_type.values().sum(),
            error_count: manager_metrics.failed_operations,
            custom_metrics: HashMap::from([
                ("total_plugins".to_string(), manager_metrics.total_plugins.into()),
                ("active_instances".to_string(), manager_metrics.active_instances.into()),
                ("total_starts".to_string(), manager_metrics.total_starts.into()),
                ("total_stops".to_string(), manager_metrics.total_stops.into()),
            ]),
        })
    }

    async fn reset_metrics(&mut self) -> ServiceResult<()> {
        let mut metrics = self.metrics.write().await;
        *metrics = PluginManagerMetrics::default();
        Ok(())
    }

    async fn get_performance_metrics(&self) -> ServiceResult<PerformanceMetrics> {
        let metrics = self.metrics.read().await;

        Ok(PerformanceMetrics {
            request_times: Vec::new(), // Would need to track actual request times
            memory_usage: 0, // Would need to get actual memory usage
            cpu_usage: 0.0, // Would need to get actual CPU usage
            active_connections: metrics.active_instances as u32,
            queue_sizes: HashMap::new(),
            custom_metrics: HashMap::from([
                ("operations_per_second".to_string(), 0.0), // Would need to calculate rate
                ("average_response_time".to_string(), 0.0), // Would need to track response times
            ]),
            timestamp: SystemTime::now(),
        })
    }
}

#[async_trait]
impl EventDriven for PluginManagerService {
    type Event = PluginManagerEvent;

    async fn subscribe(&mut self, event_type: &str) -> ServiceResult<mpsc::UnboundedReceiver<Self::Event>> {
        let (tx, rx) = mpsc::unbounded_channel();

        {
            let mut subscribers = self.event_subscribers.write().await;
            subscribers.push(tx);
        }

        info!("Subscribed to plugin manager events: {}", event_type);
        Ok(rx)
    }

    async fn unsubscribe(&mut self, event_type: &str) -> ServiceResult<()> {
        // In a real implementation, you'd manage subscriptions by event type
        info!("Unsubscribed from plugin manager events: {}", event_type);
        Ok(())
    }

    async fn publish(&self, event: Self::Event) -> ServiceResult<()> {
        self.publish_event(event);
        Ok(())
    }

    async fn handle_event(&mut self, event: Self::Event) -> ServiceResult<()> {
        debug!("Handling plugin manager event: {:?}", event);

        match event {
            PluginManagerEvent::InstanceCrashed { instance_id, plugin_id, error } => {
                error!("Instance {} crashed: {}", instance_id, error);

                // Attempt recovery based on configuration
                if self.config.lifecycle.auto_start {
                    warn!("Attempting to restart crashed instance: {}", instance_id);
                    // In a real implementation, you'd restart the instance
                }
            }
            PluginManagerEvent::ResourceViolation { instance_id, resource_type, current_value, limit } => {
                warn!("Resource violation for instance {}: {} = {:.2} > {:.2}",
                      instance_id, resource_type, current_value, limit);

                // Could trigger scaling or alerting
            }
            PluginManagerEvent::SecurityViolation { plugin_id, violation } => {
                error!("Security violation for plugin {}: {}", plugin_id, violation);

                // Could trigger plugin quarantine or disabling
            }
            PluginManagerEvent::Error { operation, error, context } => {
                error!("Error in operation {}: {} - {:?}", operation, error, context);

                // Update error metrics
                self.record_operation(&format!("error_{}", operation), false).await;
            }
            _ => {}
        }

        self.update_activity().await;
        Ok(())
    }
}

/// ============================================================================
/// PLUGIN MANAGER EXTENDED API
/// ============================================================================

impl PluginManagerService {
    /// List all registered plugins
    pub async fn list_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        self.update_activity().await;

        let registry = self.registry.read().await;
        let plugins = registry.list_plugins().await?;

        self.record_operation("list_plugins", true).await;
        Ok(plugins)
    }

    /// Get plugin details
    pub async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginManifest>> {
        self.update_activity().await;

        let registry = self.registry.read().await;
        let plugin = registry.get_plugin(plugin_id).await?;

        self.record_operation("get_plugin", true).await;
        Ok(plugin)
    }

    /// Create a new plugin instance
    pub async fn create_instance(&mut self, plugin_id: &str, config: Option<PluginInstanceConfig>) -> PluginResult<String> {
        info!("Creating instance for plugin: {}", plugin_id);

        self.update_activity().await;

        let instance_id = Self::create_instance_internal(
            plugin_id.to_string(),
            &self.instances,
            &self.registry,
            &self.resource_manager,
            &self.security_manager,
            &self.health_monitor,
        ).await?;

        // Publish event
        self.publish_event(PluginManagerEvent::InstanceCreated {
            instance_id: instance_id.clone(),
            plugin_id: plugin_id.to_string(),
        });

        self.record_operation("create_instance", true).await;
        Ok(instance_id)
    }

    /// Start a plugin instance
    pub async fn start_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        info!("Starting instance: {}", instance_id);

        self.update_activity().await;

        let instance = {
            let mut instances = self.instances.write().await;
            instances.get_mut(instance_id)
                .ok_or_else(|| PluginError::lifecycle(format!("Instance {} not found", instance_id)))?
        };

        instance.start().await?;

        // Get plugin_id for event
        let plugin_id = instance.plugin_id().to_string();

        // Publish event
        self.publish_event(PluginManagerEvent::InstanceStarted {
            instance_id: instance_id.to_string(),
            plugin_id,
        });

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_starts += 1;
            metrics.active_instances += 1;
        }

        self.record_operation("start_instance", true).await;
        Ok(())
    }

    /// Stop a plugin instance
    pub async fn stop_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        info!("Stopping instance: {}", instance_id);

        self.update_activity().await;

        let plugin_id = {
            let instances = self.instances.read().await;
            instances.get(instance_id)
                .map(|instance| instance.plugin_id().to_string())
                .ok_or_else(|| PluginError::lifecycle(format!("Instance {} not found", instance_id)))?
        };

        self.stop_instance_internal(instance_id).await?;

        // Publish event
        self.publish_event(PluginManagerEvent::InstanceStopped {
            instance_id: instance_id.to_string(),
            plugin_id,
        });

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_stops += 1;
            metrics.active_instances = metrics.active_instances.saturating_sub(1);
        }

        self.record_operation("stop_instance", true).await;
        Ok(())
    }

    /// List all active instances
    pub async fn list_instances(&self) -> PluginResult<Vec<PluginInstance>> {
        self.update_activity().await;

        let instances = self.instances.read().await;
        let instance_list: Vec<PluginInstance> = instances.values()
            .map(|instance| {
                // This is a simplified conversion - in a real implementation,
                // you'd need to properly expose instance data
                PluginInstance::default()
            })
            .collect();

        self.record_operation("list_instances", true).await;
        Ok(instance_list)
    }

    /// Get instance health status
    pub async fn get_instance_health(&self, instance_id: &str) -> PluginResult<PluginHealthStatus> {
        self.update_activity().await;

        let health_monitor = self.health_monitor.read().await;
        let health = health_monitor.get_instance_health(instance_id).await?;

        self.record_operation("get_instance_health", true).await;
        Ok(health)
    }

    /// Get system health summary
    pub async fn get_system_health(&self) -> PluginResult<SystemHealthSummary> {
        self.update_activity().await;

        let health_monitor = self.health_monitor.read().await;
        let health = health_monitor.get_system_health().await?;

        self.record_operation("get_system_health", true).await;
        Ok(health)
    }

    /// Get resource usage
    pub async fn get_resource_usage(&self, instance_id: Option<&str>) -> PluginResult<ResourceUsage> {
        self.update_activity().await;

        let resource_manager = self.resource_manager.read().await;
        let usage = if let Some(id) = instance_id {
            resource_manager.get_instance_usage(id).await?
        } else {
            resource_manager.get_global_usage().await?
        };

        self.record_operation("get_resource_usage", true).await;
        Ok(usage)
    }

    /// Perform manual health check
    pub async fn perform_health_check(&self, instance_id: &str) -> PluginResult<HealthCheckResult> {
        self.update_activity().await;

        let health_monitor = self.health_monitor.read().await;
        let result = health_monitor.perform_health_check(instance_id).await?;

        self.record_operation("perform_health_check", true).await;
        Ok(result)
    }

    /// Subscribe to plugin manager events
    pub async fn subscribe_events(&mut self) -> mpsc::UnboundedReceiver<PluginManagerEvent> {
        self.subscribe("plugin_manager").await
            .map_err(|e| PluginError::generic(e.to_string()))
            .unwrap_or_else(|_| {
                let (tx, rx) = mpsc::unbounded_channel();
                tx
            })
    }
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Create a new PluginManager service
pub fn create_plugin_manager(config: PluginManagerConfig) -> PluginManagerService {
    PluginManagerService::new(config)
}

/// Create a PluginManager service with default configuration
pub fn create_default_plugin_manager() -> PluginManagerService {
    let config = PluginManagerConfig::default();
    PluginManagerService::new(config)
}