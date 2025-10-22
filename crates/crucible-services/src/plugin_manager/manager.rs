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
use super::resource_monitor::{ResourceMonitor, ResourceMonitoringService};
use super::health_checker::{HealthChecker, HealthCheckingService};
use super::lifecycle_manager::{LifecycleManager, LifecycleManagerService, LifecycleOperation, LifecycleOperationRequest};
use super::state_machine::{PluginStateMachine, StateMachineService, StateTransition, StateTransitionResult};
use super::dependency_resolver::{DependencyResolver, DependencyGraph, DependencyResolutionResult};
use super::lifecycle_policy::{LifecyclePolicyEngine, PolicyEngineService, PolicyDecision, PolicyEvaluationContext};
use super::automation_engine::{AutomationEngine, AutomationEngineService, AutomationRule, AutomationEvent, AutomationExecutionContext};
use super::batch_operations::{BatchOperationsCoordinator, BatchOperationsService, BatchOperation, BatchExecutionResult, BatchExecutionContext};
use crate::service_types::*;
use crate::service_traits::*;
use crate::errors::{ServiceError, ServiceResult};
use crate::events::EventEmitter;
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

    /// Advanced monitoring components
    resource_monitor: Arc<ResourceMonitor>,
    health_checker: Arc<HealthChecker>,

    /// Advanced lifecycle management components
    lifecycle_manager: Arc<LifecycleManager>,
    state_machine: Arc<PluginStateMachine>,
    dependency_resolver: Arc<DependencyResolver>,
    policy_engine: Arc<LifecyclePolicyEngine>,
    automation_engine: Arc<AutomationEngine>,
    batch_coordinator: Arc<BatchOperationsCoordinator>,

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

/// Monitoring statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitoringStatistics {
    /// Resource monitoring enabled
    pub resource_monitoring_enabled: bool,
    /// Health monitoring enabled
    pub health_monitoring_enabled: bool,
    /// Total monitored instances
    pub total_monitored_instances: u64,
    /// Active resource monitors
    pub active_resource_monitors: u64,
    /// Active health checks
    pub active_health_checks: u64,
    /// Resource alerts triggered
    pub resource_alerts_triggered: u64,
    /// Health alerts triggered
    pub health_alerts_triggered: u64,
    /// Average resource collection time
    pub average_resource_collection_time: Duration,
    /// Average health check time
    pub average_health_check_time: Duration,
    /// Last updated timestamp
    pub last_updated: SystemTime,
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

        // Create event emitter for monitoring components
        let event_emitter = EventEmitter::new();

        // Initialize advanced monitoring components
        let resource_monitor = Arc::new(ResourceMonitor::new(
            config.resource_management.monitoring.clone(),
            event_emitter.clone(),
        ));
        let health_checker = Arc::new(HealthChecker::new(
            config.health_monitoring.scheduling.clone(),
            event_emitter.clone(),
        ));

        // Initialize advanced lifecycle management components
        let lifecycle_manager = Arc::new(LifecycleManager::new(config.clone()));
        let state_machine = Arc::new(PluginStateMachine::new());
        let dependency_resolver = Arc::new(DependencyResolver::new());
        let policy_engine = Arc::new(LifecyclePolicyEngine::new());
        let automation_engine = Arc::new(AutomationEngine::new(
            lifecycle_manager.clone(),
            policy_engine.clone(),
            dependency_resolver.clone(),
            state_machine.clone(),
        ));
        let batch_coordinator = Arc::new(BatchOperationsCoordinator::new(
            lifecycle_manager.clone(),
            policy_engine.clone(),
            dependency_resolver.clone(),
            state_machine.clone(),
            automation_engine.clone(),
        ));

        Self {
            config: Arc::new(config),
            registry: Arc::new(RwLock::new(registry)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            resource_manager: Arc::new(RwLock::new(resource_manager)),
            security_manager: Arc::new(RwLock::new(security_manager)),
            health_monitor: Arc::new(RwLock::new(health_monitor)),
            resource_monitor,
            health_checker,
            lifecycle_manager,
            state_machine,
            dependency_resolver,
            policy_engine,
            automation_engine,
            batch_coordinator,
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

        // Initialize advanced monitoring components
        info!("Initializing advanced monitoring components");

        // Start resource monitoring
        {
            let mut resource_monitor = self.resource_monitor.as_ref().clone();
            resource_monitor.start_monitoring_loop().await?;
        }

        // Start health checking
        {
            let mut health_checker = self.health_checker.as_ref().clone();
            health_checker.start_health_checking_loop().await?;
        }

        // Initialize advanced lifecycle management components
        info!("Initializing advanced lifecycle management components");

        self.state_machine.initialize().await?;
        self.dependency_resolver.initialize().await?;
        self.policy_engine.initialize().await?;
        self.lifecycle_manager.initialize().await?;
        self.automation_engine.initialize().await?;
        self.batch_coordinator.initialize().await?;

        // Setup event handlers and integration
        self.setup_lifecycle_integration().await?;
        self.setup_event_handlers().await?;
        self.setup_monitoring_integration().await?;

        info!("PluginManager components initialized");
        Ok(())
    }

    /// Setup lifecycle integration between components
    async fn setup_lifecycle_integration(&mut self) -> PluginResult<()> {
        info!("Setting up lifecycle integration between components");

        // Setup state machine event handling
        let state_machine_events = self.state_machine.subscribe_events().await;
        let lifecycle_manager = self.lifecycle_manager.clone();
        let automation_engine = self.automation_engine.clone();
        let event_subscribers = self.event_subscribers.clone();

        tokio::spawn(async move {
            while let Some(event) = state_machine_events.recv().await {
                match event {
                    StateMachineEvent::StateEntered { instance_id, state } => {
                        debug!("Instance {} entered state: {:?}", instance_id, state);

                        // Trigger automation rules based on state changes
                        let automation_event = AutomationEvent {
                            event_id: uuid::Uuid::new_v4().to_string(),
                            event_type: "state_change".to_string(),
                            source: "state_machine".to_string(),
                            timestamp: SystemTime::now(),
                            data: HashMap::from([
                                ("instance_id".to_string(), serde_json::Value::String(instance_id.clone())),
                                ("state".to_string(), serde_json::Value::String(format!("{:?}", state))),
                            ]),
                            severity: AutomationEventSeverity::Normal,
                        };

                        if let Err(e) = automation_engine.process_event(automation_event).await {
                            error!("Failed to process automation event: {}", e);
                        }
                    }
                    StateMachineEvent::TransitionFailed { instance_id, transition, error } => {
                        warn!("Instance {} transition {:?} failed: {}", instance_id, transition, error);

                        // Trigger failure handling automation
                        let automation_event = AutomationEvent {
                            event_id: uuid::Uuid::new_v4().to_string(),
                            event_type: "transition_failure".to_string(),
                            source: "state_machine".to_string(),
                            timestamp: SystemTime::now(),
                            data: HashMap::from([
                                ("instance_id".to_string(), serde_json::Value::String(instance_id.clone())),
                                ("transition".to_string(), serde_json::Value::String(format!("{:?}", transition))),
                                ("error".to_string(), serde_json::Value::String(error.clone())),
                            ]),
                            severity: AutomationEventSeverity::High,
                        };

                        if let Err(e) = automation_engine.process_event(automation_event).await {
                            error!("Failed to process automation event: {}", e);
                        }
                    }
                    _ => {
                        debug!("State machine event: {:?}", event);
                    }
                }
            }
        });

        // Setup lifecycle manager event handling
        let lifecycle_events = self.lifecycle_manager.subscribe_events().await;
        let state_machine = self.state_machine.clone();
        let policy_engine = self.policy_engine.clone();

        tokio::spawn(async move {
            while let Some(event) = lifecycle_events.recv().await {
                match event {
                    LifecycleEvent::InstanceStarted { instance_id, plugin_id } => {
                        info!("Instance {} started for plugin {}", instance_id, plugin_id);

                        // Update state machine
                        if let Err(e) = state_machine.transition_state(&instance_id, StateTransition::CompleteStart).await {
                            error!("Failed to update state machine for instance {}: {}", instance_id, e);
                        }
                    }
                    LifecycleEvent::InstanceStopped { instance_id, plugin_id } => {
                        info!("Instance {} stopped for plugin {}", instance_id, plugin_id);

                        // Update state machine
                        if let Err(e) = state_machine.transition_state(&instance_id, StateTransition::CompleteStop).await {
                            error!("Failed to update state machine for instance {}: {}", instance_id, e);
                        }
                    }
                    LifecycleEvent::InstanceCrashed { instance_id, plugin_id, error } => {
                        error!("Instance {} crashed for plugin {}: {}", instance_id, plugin_id, error);

                        // Update state machine to error state
                        if let Err(e) = state_machine.transition_state(&instance_id, StateTransition::Error(error.clone())).await {
                            error!("Failed to update state machine for crashed instance {}: {}", instance_id, e);
                        }
                    }
                    _ => {
                        debug!("Lifecycle event: {:?}", event);
                    }
                }
            }
        });

        // Setup policy engine event handling
        let policy_events = self.policy_engine.subscribe_events().await;
        let lifecycle_manager = self.lifecycle_manager.clone();

        tokio::spawn(async move {
            while let Some(event) = policy_events.recv().await {
                match event {
                    PolicyEvent::PolicyEvaluated { policy_id, decision } => {
                        debug!("Policy {} evaluated: allowed={}", policy_id, decision.allowed);

                        if !decision.allowed {
                            warn!("Operation blocked by policy {}: {}", policy_id, decision.reason);
                            // Policy engine would have already blocked the operation
                        }
                    }
                    PolicyEvent::ActionExecuted { action_id, result } => {
                        if result.success {
                            debug!("Policy action {} executed successfully", action_id);
                        } else {
                            error!("Policy action {} failed: {:?}", action_id, result.error);
                        }
                    }
                    _ => {
                        debug!("Policy event: {:?}", event);
                    }
                }
            }
        });

        info!("Lifecycle integration setup completed");
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

    /// Setup monitoring integration between components
    async fn setup_monitoring_integration(&mut self) -> PluginResult<()> {
        info!("Setting up monitoring integration between components");

        // Start monitoring for existing instances when they're created
        let resource_monitor = self.resource_monitor.clone();
        let health_checker = self.health_checker.clone();
        let event_subscribers = self.event_subscribers.clone();

        // Subscribe to instance events to start/stop monitoring
        let mut subscribers = event_subscribers.write().await;

        // Setup monitoring event handlers
        let resource_monitor_clone = resource_monitor.clone();
        let health_checker_clone = health_checker.clone();

        // In a real implementation, you would subscribe to instance lifecycle events
        // and automatically start/stop monitoring when instances are created/destroyed

        info!("Monitoring integration setup completed");
        Ok(())
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

    // ============================================================================
    // ADVANCED MONITORING METHODS
    // ============================================================================

    /// Get detailed resource usage history for a plugin instance
    pub async fn get_resource_usage_history(&self, instance_id: &str) -> PluginResult<Option<super::resource_monitor::ResourceUsageHistory>> {
        self.update_activity().await;

        let history = self.resource_monitor.get_usage_history(instance_id).await?;
        self.record_operation("get_resource_usage_history", true).await;
        Ok(history)
    }

    /// Get current resource usage for all monitored plugins
    pub async fn get_aggregated_resource_usage(&self) -> PluginResult<HashMap<String, ResourceUsage>> {
        self.update_activity().await;

        let usage = self.resource_monitor.get_aggregated_usage().await?;
        self.record_operation("get_aggregated_resource_usage", true).await;
        Ok(usage)
    }

    /// Get system-wide resource information
    pub async fn get_system_resource_info(&self) -> PluginResult<super::resource_monitor::SystemResourceInfo> {
        self.update_activity().await;

        let info = self.resource_monitor.get_system_info().await?;
        self.record_operation("get_system_resource_info", true).await;
        Ok(info)
    }

    /// Update resource monitoring thresholds for a plugin
    pub async fn update_resource_thresholds(
        &self,
        instance_id: &str,
        thresholds: HashMap<super::resource_monitor::ResourceType, super::resource_monitor::ResourceThreshold>,
    ) -> PluginResult<()> {
        self.update_activity().await;

        self.resource_monitor.update_thresholds(instance_id, thresholds).await?;
        self.record_operation("update_resource_thresholds", true).await;
        Ok(())
    }

    /// Get health status for a plugin instance
    pub async fn get_plugin_health_status(&self, instance_id: &str) -> PluginResult<Option<PluginHealthStatus>> {
        self.update_activity().await;

        let status = self.health_checker.get_health_status(instance_id).await?;
        self.record_operation("get_plugin_health_status", true).await;
        Ok(status)
    }

    /// Get health status history for a plugin instance
    pub async fn get_plugin_health_history(&self, instance_id: &str) -> PluginResult<Option<super::health_checker::HealthStatusHistory>> {
        self.update_activity().await;

        let history = self.health_checker.get_health_history(instance_id).await?;
        self.record_operation("get_plugin_health_history", true).await;
        Ok(history)
    }

    /// Get health statistics for a plugin instance
    pub async fn get_plugin_health_statistics(&self, instance_id: &str) -> PluginResult<Option<super::health_checker::HealthStatistics>> {
        self.update_activity().await;

        let stats = self.health_checker.get_health_statistics(instance_id).await?;
        self.record_operation("get_plugin_health_statistics", true).await;
        Ok(stats)
    }

    /// Trigger immediate health check for a plugin instance
    pub async fn trigger_plugin_health_check(
        &self,
        instance_id: &str,
        check_types: Option<Vec<super::config::HealthCheckType>>,
    ) -> PluginResult<Vec<super::health_checker::HealthCheckResult>> {
        self.update_activity().await;

        let results = self.health_checker.trigger_health_check(instance_id, check_types).await?;
        self.record_operation("trigger_plugin_health_check", true).await;
        Ok(results)
    }

    /// Get monitoring statistics for the plugin manager
    pub async fn get_monitoring_statistics(&self) -> PluginResult<MonitoringStatistics> {
        self.update_activity().await;

        let mut stats = MonitoringStatistics::default();

        // Get resource monitoring statistics
        // Note: This would require adding statistics methods to the resource monitor
        stats.resource_monitoring_enabled = self.config.resource_management.monitoring.enabled;
        stats.health_monitoring_enabled = self.config.health_monitoring.enabled;

        self.record_operation("get_monitoring_statistics", true).await;
        Ok(stats)
    }

    /// Check resource quota violations for a plugin instance
    pub async fn check_resource_quota_violations(&self, instance_id: &str) -> PluginResult<Vec<super::resource_monitor::ResourceType>> {
        self.update_activity().await;

        let violations = self.resource_monitor.check_quota_violations(instance_id).await?;
        self.record_operation("check_resource_quota_violations", true).await;
        Ok(violations)
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

    // ============================================================================
    // ADVANCED LIFECYCLE MANAGEMENT API
    // ============================================================================

    /// Get dependency resolver for advanced dependency management
    pub async fn get_dependency_resolver(&self) -> &DependencyResolver {
        &self.dependency_resolver
    }

    /// Resolve plugin dependencies and get startup order
    pub async fn resolve_plugin_dependencies(&self, root_instances: &[String]) -> PluginResult<DependencyResolutionResult> {
        self.dependency_resolver.resolve_dependencies(None).await
    }

    /// Get dependency graph visualization
    pub async fn get_dependency_graph_visualization(&self, format: super::dependency_resolver::GraphFormat) -> PluginResult<String> {
        let graph = self.dependency_resolver.get_graph_visualization(format).await?;
        Ok(graph)
    }

    /// Get plugin state machine for advanced state management
    pub async fn get_state_machine(&self) -> &PluginStateMachine {
        &self.state_machine
    }

    /// Get current state of all plugin instances
    pub async fn get_all_instance_states(&self) -> PluginResult<HashMap<String, PluginInstanceState>> {
        self.state_machine.get_all_states().await
    }

    /// Get state history for an instance
    pub async fn get_instance_state_history(&self, instance_id: &str, limit: Option<usize>) -> PluginResult<Vec<StateTransitionResult>> {
        self.state_machine.get_state_history(instance_id, limit).await
    }

    /// Get policy engine for rule-based management
    pub async fn get_policy_engine(&self) -> &LifecyclePolicyEngine {
        &self.policy_engine
    }

    /// Add a lifecycle policy
    pub async fn add_lifecycle_policy(&self, policy: super::lifecycle_policy::LifecyclePolicy) -> PluginResult<()> {
        self.policy_engine.add_policy(policy).await
    }

    /// Evaluate policies for an operation
    pub async fn evaluate_lifecycle_policies(&self, context: &PolicyEvaluationContext) -> PluginResult<PolicyDecision> {
        self.policy_engine.evaluate_operation(context).await
    }

    /// Get automation engine for automated operations
    pub async fn get_automation_engine(&self) -> &AutomationEngine {
        &self.automation_engine
    }

    /// Add an automation rule
    pub async fn add_automation_rule(&self, rule: AutomationRule) -> PluginResult<()> {
        self.automation_engine.add_rule(rule).await
    }

    /// Trigger an automation rule manually
    pub async fn trigger_automation_rule(&self, rule_id: &str, trigger_data: HashMap<String, serde_json::Value>) -> PluginResult<String> {
        self.automation_engine.trigger_rule(rule_id, trigger_data).await
    }

    /// Get batch operations coordinator for bulk operations
    pub async fn get_batch_coordinator(&self) -> &BatchOperationsCoordinator {
        &self.batch_coordinator
    }

    /// Create and execute a batch operation
    pub async fn execute_batch_operation(&self, batch: BatchOperation) -> PluginResult<String> {
        let execution_context = BatchExecutionContext {
            batch_id: batch.batch_id.clone(),
            execution_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            mode: super::batch_operations::ExecutionMode::Normal,
            dry_run: false,
            additional_context: HashMap::new(),
        };

        self.batch_coordinator.create_batch(batch).await?;
        self.batch_coordinator.execute_batch(&execution_context.batch_id, execution_context).await
    }

    /// Get batch execution progress
    pub async fn get_batch_execution_progress(&self, execution_id: &str) -> PluginResult<Option<super::batch_operations::BatchProgressUpdate>> {
        self.batch_coordinator.get_execution_progress(execution_id).await
    }

    /// Execute rolling restart of plugin instances
    pub async fn execute_rolling_restart(&self, instances: Vec<String>, batch_size: u32) -> PluginResult<String> {
        let operations = instances.into_iter().enumerate().map(|(index, instance_id)| {
            super::batch_operations::BatchOperationItem {
                item_id: format!("restart-{}", index),
                operation: LifecycleOperation::Restart { instance_id },
                target: instance_id,
                priority: super::batch_operations::BatchItemPriority::Normal,
                dependencies: Vec::new(),
                timeout: Some(Duration::from_secs(300)),
                retry_config: Some(super::batch_operations::BatchRetryConfig {
                    max_attempts: 3,
                    initial_delay: Duration::from_secs(5),
                    backoff_strategy: super::lifecycle_manager::BackoffStrategy::Exponential,
                    retry_on_errors: vec!["timeout".to_string()],
                    delay_multiplier: 2.0,
                }),
                rollback_config: None,
                metadata: HashMap::new(),
            }
        }).collect();

        let batch = BatchOperation {
            batch_id: format!("rolling-restart-{}", uuid::Uuid::new_v4()),
            name: "Rolling Restart".to_string(),
            description: format!("Rolling restart of {} instances", operations.len()),
            operations,
            strategy: super::batch_operations::BatchExecutionStrategy::Rolling {
                batch_size,
                pause_duration: Duration::from_secs(30),
                health_check_between_batches: true,
                rollback_on_batch_failure: true,
            },
            config: super::batch_operations::BatchConfig::default(),
            scope: super::batch_operations::BatchScope::default(),
            metadata: super::batch_operations::BatchMetadata {
                created_at: SystemTime::now(),
                created_by: "plugin_manager".to_string(),
                updated_at: SystemTime::now(),
                updated_by: "plugin_manager".to_string(),
                tags: vec!["restart".to_string(), "rolling".to_string()],
                documentation: Some("Rolling restart of plugin instances".to_string()),
                additional_info: HashMap::new(),
            },
        };

        self.execute_batch_operation(batch).await
    }

    /// Execute zero-downtime restart with canary deployment
    pub async fn execute_zero_downtime_restart(&self, instances: Vec<String>, canary_percentage: u32) -> PluginResult<String> {
        let operations = instances.into_iter().enumerate().map(|(index, instance_id)| {
            super::batch_operations::BatchOperationItem {
                item_id: format!("restart-{}", index),
                operation: LifecycleOperation::Restart { instance_id },
                target: instance_id,
                priority: super::batch_operations::BatchItemPriority::High,
                dependencies: Vec::new(),
                timeout: Some(Duration::from_secs(600)),
                retry_config: Some(super::batch_operations::BatchRetryConfig {
                    max_attempts: 5,
                    initial_delay: Duration::from_secs(10),
                    backoff_strategy: super::lifecycle_manager::BackoffStrategy::Exponential,
                    retry_on_errors: vec!["timeout".to_string(), "health_check_failure".to_string()],
                    delay_multiplier: 2.0,
                }),
                rollback_config: Some(super::batch_operations::BatchRollbackConfig {
                    auto_rollback: true,
                    strategy: super::batch_operations::RollbackStrategy::ReverseOrder,
                    timeout: Duration::from_secs(300),
                    preserve_data: true,
                    notifications: Vec::new(),
                }),
                metadata: HashMap::from([
                    ("restart_type".to_string(), serde_json::Value::String("zero_downtime".to_string())),
                ]),
            }
        }).collect();

        let batch = BatchOperation {
            batch_id: format!("zero-downtime-restart-{}", uuid::Uuid::new_v4()),
            name: "Zero-Downtime Restart".to_string(),
            description: format!("Zero-downtime restart of {} instances with {}% canary", operations.len(), canary_percentage),
            operations,
            strategy: super::batch_operations::BatchExecutionStrategy::Canary {
                canary_size: super::batch_operations::CanarySize::Percentage(canary_percentage),
                pause_duration: Duration::from_secs(300), // 5 minutes
                success_criteria: super::batch_operations::CanarySuccessCriteria {
                    success_rate_threshold: 95.0,
                    health_criteria: vec![
                        super::batch_operations::HealthCriteria {
                            metric: "availability".to_string(),
                            operator: super::lifecycle_policy::ComparisonOperator::GreaterThanOrEqual,
                            threshold: 99.0,
                        },
                    ],
                    performance_criteria: vec![],
                    evaluation_window: Duration::from_secs(300),
                },
                auto_promote: true,
            },
            config: super::batch_operations::BatchConfig::default(),
            scope: super::batch_operations::BatchScope::default(),
            metadata: super::batch_operations::BatchMetadata {
                created_at: SystemTime::now(),
                created_by: "plugin_manager".to_string(),
                updated_at: SystemTime::now(),
                updated_by: "plugin_manager".to_string(),
                tags: vec!["restart".to_string(), "zero_downtime".to_string(), "canary".to_string()],
                documentation: Some("Zero-downtime restart with canary deployment".to_string()),
                additional_info: HashMap::from([
                    ("canary_percentage".to_string(), serde_json::Value::Number(canary_percentage.into())),
                ]),
            },
        };

        self.execute_batch_operation(batch).await
    }

    /// Scale plugin instances with dependency awareness
    pub async fn scale_plugin_with_dependencies(&self, plugin_id: &str, target_instances: u32) -> PluginResult<Vec<String>> {
        info!("Scaling plugin {} to {} instances with dependency awareness", plugin_id, target_instances);

        // Get current instances
        let current_instances = self.list_instances().await?;
        let current_count = current_instances.len() as u32;

        if target_instances == current_count {
            return Ok(current_instances.iter().map(|i| i.instance_id.clone()).collect());
        }

        if target_instances > current_count {
            // Scale up
            let instances_to_add = target_instances - current_count;
            let mut new_instance_ids = Vec::new();

            for i in 0..instances_to_add {
                let instance_id = format!("{}-{}", plugin_id, uuid::Uuid::new_v4().to_string()[..8]);

                // Create instance
                let created_instance_id = self.create_instance(plugin_id, None).await?;
                new_instance_ids.push(created_instance_id);

                // Add instance to dependency resolver
                let dependencies = Vec::new(); // TODO: Get from plugin manifest
                self.dependency_resolver.add_instance(created_instance_id.clone(), dependencies).await?;

                // Start instance with dependency resolution
                self.lifecycle_manager.start_instance_with_dependencies(&created_instance_id).await?;
            }

            info!("Successfully scaled up plugin {} from {} to {} instances", plugin_id, current_count, target_instances);
            Ok(new_instance_ids)
        } else {
            // Scale down
            let instances_to_remove = current_count - target_instances;

            // Get instances to remove (remove newest first)
            let mut instance_ids: Vec<String> = current_instances.iter()
                .map(|i| i.instance_id.clone())
                .collect();
            instance_ids.sort(); // Simple sort by ID (newer instances have higher UUIDs)
            instance_ids.truncate(instances_to_remove as usize);

            for instance_id in &instance_ids {
                // Stop instance gracefully
                self.lifecycle_manager.stop_instance_gracefully(instance_id, Some(Duration::from_secs(60))).await?;

                // Remove from dependency resolver
                self.dependency_resolver.remove_instance(instance_id).await?;
            }

            info!("Successfully scaled down plugin {} from {} to {} instances", plugin_id, current_count, target_instances);
            Ok(Vec::new())
        }
    }

    /// Perform health-based plugin restart
    pub async fn perform_health_based_restart(&self, instance_id: &str) -> PluginResult<()> {
        info!("Performing health-based restart for instance: {}", instance_id);

        // Get current health status
        let health_status = self.get_instance_health(instance_id).await?;

        match health_status {
            PluginHealthStatus::Healthy => {
                info!("Instance {} is healthy, no restart needed", instance_id);
                Ok(())
            }
            PluginHealthStatus::Degraded | PluginHealthStatus::Unhealthy => {
                warn!("Instance {} health status is {:?}, performing restart", instance_id, health_status);

                // Perform graceful restart
                self.lifecycle_manager.restart_instance_zero_downtime(instance_id).await
            }
            PluginHealthStatus::Unknown => {
                warn!("Instance {} health status is unknown, performing health check", instance_id);

                // Perform health check first
                let health_result = self.perform_health_check(instance_id).await?;

                if health_result.healthy {
                    info!("Health check passed for instance {}, no restart needed", instance_id);
                    Ok(())
                } else {
                    warn!("Health check failed for instance {}, performing restart", instance_id);
                    self.lifecycle_manager.restart_instance_zero_downtime(instance_id).await
                }
            }
        }
    }

    /// Get comprehensive lifecycle analytics
    pub async fn get_lifecycle_analytics(&self) -> PluginResult<LifecycleAnalytics> {
        let dependency_analytics = self.dependency_resolver.get_analytics().await?;
        let lifecycle_metrics = self.lifecycle_manager.get_metrics().await?;
        let state_metrics = self.state_machine.get_metrics().await?;
        let policy_metrics = self.policy_engine.get_metrics().await?;
        let automation_metrics = self.automation_engine.get_metrics().await?;
        let batch_metrics = self.batch_coordinator.get_metrics().await?;

        Ok(LifecycleAnalytics {
            dependency_analytics,
            lifecycle_metrics,
            state_metrics,
            policy_metrics,
            automation_metrics,
            batch_metrics,
            timestamp: SystemTime::now(),
        })
    }

    /// Export lifecycle configuration
    pub async fn export_lifecycle_configuration(&self) -> PluginResult<ExportedLifecycleConfig> {
        let policies = self.policy_engine.list_policies().await?;
        let automation_rules = self.automation_engine.list_rules().await?;
        let batch_templates = self.batch_coordinator.list_templates().await?;

        Ok(ExportedLifecycleConfig {
            version: "1.0.0".to_string(),
            exported_at: SystemTime::now(),
            policies,
            automation_rules,
            batch_templates,
            metadata: HashMap::new(),
        })
    }

    /// Import lifecycle configuration
    pub async fn import_lifecycle_configuration(&self, config: ExportedLifecycleConfig) -> PluginResult<()> {
        info!("Importing lifecycle configuration version {}", config.version);

        // Import policies
        for policy in config.policies {
            self.policy_engine.add_policy(policy).await?;
        }

        // Import automation rules
        for rule in config.automation_rules {
            self.automation_engine.add_rule(rule).await?;
        }

        // Import batch templates
        for template in config.batch_templates {
            self.batch_coordinator.create_template(template).await?;
        }

        info!("Successfully imported lifecycle configuration");
        Ok(())
    }
}

/// ============================================================================
/// ADDITIONAL TYPE DEFINITIONS FOR ADVANCED LIFECYCLE MANAGEMENT
/// ============================================================================

/// Comprehensive lifecycle analytics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleAnalytics {
    /// Dependency analytics
    pub dependency_analytics: super::dependency_resolver::DependencyAnalytics,
    /// Lifecycle manager metrics
    pub lifecycle_metrics: super::lifecycle_manager::LifecycleManagerMetrics,
    /// State machine metrics
    pub state_metrics: super::state_machine::StateMachineMetrics,
    /// Policy engine metrics
    pub policy_metrics: super::lifecycle_policy::PolicyEngineMetrics,
    /// Automation engine metrics
    pub automation_metrics: super::automation_engine::AutomationEngineMetrics,
    /// Batch coordinator metrics
    pub batch_metrics: super::batch_operations::BatchCoordinatorMetrics,
    /// Analytics timestamp
    pub timestamp: SystemTime,
}

/// Exported lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedLifecycleConfig {
    /// Configuration version
    pub version: String,
    /// Export timestamp
    pub exported_at: SystemTime,
    /// Lifecycle policies
    pub policies: Vec<super::lifecycle_policy::LifecyclePolicy>,
    /// Automation rules
    pub automation_rules: Vec<super::automation_engine::AutomationRule>,
    /// Batch templates
    pub batch_templates: Vec<super::batch_operations::BatchTemplate>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
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