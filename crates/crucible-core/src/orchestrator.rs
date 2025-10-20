//! Service orchestration system for Crucible
//!
//! This module provides service lifecycle management, dependency injection,
//! health monitoring, and automatic recovery mechanisms for all system services.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, error, info, warn};

use crate::config::ConfigManager;
use crucible_services::*;

/// Service orchestration metrics
#[derive(Debug, Clone, Default)]
pub struct OrchestrationMetrics {
    pub active_connections: u64,
    pub services_started: u64,
    pub services_stopped: u64,
    pub services_failed: u64,
    pub health_checks_passed: u64,
    pub health_checks_failed: u64,
}

/// Service instance information
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    pub service_info: ServiceInfo,
    pub service: Arc<dyn BaseService>,
    pub started_at: Option<Instant>,
    pub last_health_check: Option<Instant>,
    pub health_status: bool,
    pub restart_count: u32,
    pub dependencies: HashSet<String>,
}

/// Service orchestration command
#[derive(Debug)]
pub enum OrchestrationCommand {
    RegisterService {
        service_info: ServiceInfo,
        service: Arc<dyn BaseService>,
        response_tx: oneshot::Sender<Result<()>>,
    },
    UnregisterService {
        service_id: uuid::Uuid,
        response_tx: oneshot::Sender<Result<bool>>,
    },
    StartService {
        service_id: uuid::Uuid,
        response_tx: oneshot::Sender<Result<()>>,
    },
    StopService {
        service_id: uuid::Uuid,
        response_tx: oneshot::Sender<Result<()>>,
    },
    GetServices {
        response_tx: oneshot::Sender<Vec<ServiceInstance>>,
    },
    GetMetrics {
        response_tx: oneshot::Sender<OrchestrationMetrics>,
    },
    HealthCheck {
        service_id: Option<uuid::Uuid>,
        response_tx: oneshot::Sender<Result<HashMap<uuid::Uuid, bool>>>,
    },
}

/// Service orchestrator for managing service lifecycle
#[derive(Debug)]
pub struct ServiceOrchestrator {
    /// Configuration manager
    config_manager: Arc<ConfigManager>,
    /// Registered services
    services: Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
    /// Service dependency graph
    dependencies: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Command receiver
    command_rx: mpsc::UnboundedReceiver<OrchestrationCommand>,
    /// Command sender
    command_tx: mpsc::UnboundedSender<OrchestrationCommand>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ServiceEvent>,
    /// Orchestration metrics
    metrics: Arc<RwLock<OrchestrationMetrics>>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

/// Service events
#[derive(Debug, Clone)]
pub enum ServiceEvent {
    Registered {
        service_id: uuid::Uuid,
        service_type: String,
    },
    Started {
        service_id: uuid::Uuid,
        service_type: String,
    },
    Stopped {
        service_id: uuid::Uuid,
        service_type: String,
    },
    Failed {
        service_id: uuid::Uuid,
        service_type: String,
        error: String,
    },
    HealthChanged {
        service_id: uuid::Uuid,
        service_type: String,
        healthy: bool,
    },
}

impl ServiceOrchestrator {
    /// Create a new service orchestrator
    pub async fn new(config_manager: Arc<ConfigManager>) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1000);

        Ok(Self {
            config_manager,
            services: Arc::new(RwLock::new(HashMap::new())),
            dependencies: Arc::new(RwLock::new(HashMap::new())),
            command_rx,
            command_tx,
            event_tx,
            metrics: Arc::new(RwLock::new(OrchestrationMetrics::default())),
            running: Arc::new(RwLock::new(false)),
        })
    }

    /// Get command sender
    pub fn command_sender(&self) -> mpsc::UnboundedSender<OrchestrationCommand> {
        self.command_tx.clone()
    }

    /// Subscribe to service events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ServiceEvent> {
        self.event_tx.subscribe()
    }

    /// Start the orchestrator
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            warn!("Orchestrator is already running");
            return Ok(());
        }

        *running = true;
        info!("Service orchestrator started");

        // Start background tasks
        self.start_health_check_task();
        self.start_command_processing_task();

        Ok(())
    }

    /// Stop the orchestrator
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            warn!("Orchestrator is not running");
            return Ok(());
        }

        *running = false;

        // Stop all services
        let services = self.services.read().await;
        for (service_id, instance) in services.iter() {
            if let Err(e) = instance.service.stop().await {
                error!("Failed to stop service {}: {}", service_id, e);
            } else {
                info!("Stopped service: {}", service_id);
            }
        }

        info!("Service orchestrator stopped");
        Ok(())
    }

    /// Register a new service
    pub async fn register_service(
        &self,
        service_info: ServiceInfo,
        service: Arc<dyn BaseService>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::RegisterService {
            service_info,
            service,
            response_tx: tx,
        })?;

        rx.await?
    }

    /// Unregister a service
    pub async fn unregister_service(&self, service_id: uuid::Uuid) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::UnregisterService {
            service_id,
            response_tx: tx,
        })?;

        rx.await?
    }

    /// Start a specific service
    pub async fn start_service(&self, service_id: uuid::Uuid) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::StartService {
            service_id,
            response_tx: tx,
        })?;

        rx.await?
    }

    /// Stop a specific service
    pub async fn stop_service(&self, service_id: uuid::Uuid) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::StopService {
            service_id,
            response_tx: tx,
        })?;

        rx.await?
    }

    /// Get all services
    pub async fn get_services(&self) -> Vec<ServiceInstance> {
        let (tx, rx) = oneshot::channel();
        let _ = self.command_tx.send(OrchestrationCommand::GetServices { response_tx: tx });
        rx.await.unwrap_or_default()
    }

    /// Get services information for status reporting
    pub async fn get_services_info(&self) -> Vec<ServiceInfo> {
        self.get_services().await.into_iter().map(|instance| instance.service_info).collect()
    }

    /// Get orchestration metrics
    pub async fn get_metrics(&self) -> OrchestrationMetrics {
        let (tx, rx) = oneshot::channel();
        let _ = self.command_tx.send(OrchestrationCommand::GetMetrics { response_tx: tx });
        rx.await.unwrap_or_default()
    }

    /// Perform health check
    pub async fn health_check(&self) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::HealthCheck {
            service_id: None,
            response_tx: tx,
        })?;

        let results = rx.await??;
        Ok(results.values().all(|&healthy| healthy))
    }

    /// Perform health check for specific service
    pub async fn health_check_service(&self, service_id: uuid::Uuid) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.command_tx.send(OrchestrationCommand::HealthCheck {
            service_id: Some(service_id),
            response_tx: tx,
        })?;

        let results = rx.await??;
        Ok(results.get(&service_id).copied().unwrap_or(false))
    }

    /// Start command processing task
    fn start_command_processing_task(&self) {
        let services = self.services.clone();
        let dependencies = self.dependencies.clone();
        let mut command_rx = self.command_rx.clone();
        let event_tx = self.event_tx.clone();
        let metrics = self.metrics.clone();
        let config_manager = self.config_manager.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            while *running.read().await {
                match command_rx.recv().await {
                    Some(command) => {
                        Self::handle_command(
                            command,
                            services.clone(),
                            dependencies.clone(),
                            event_tx.clone(),
                            metrics.clone(),
                            config_manager.clone(),
                        ).await;
                    }
                    None => break,
                }
            }
        });
    }

    /// Handle orchestration command
    async fn handle_command(
        command: OrchestrationCommand,
        services: Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        dependencies: Arc<RwLock<HashMap<String, HashSet<String>>>>,
        event_tx: broadcast::Sender<ServiceEvent>,
        metrics: Arc<RwLock<OrchestrationMetrics>>,
        config_manager: Arc<ConfigManager>,
    ) {
        match command {
            OrchestrationCommand::RegisterService { service_info, service, response_tx } => {
                let result = Self::register_service_internal(
                    &services,
                    &dependencies,
                    service_info,
                    service,
                    &event_tx,
                    &config_manager,
                ).await;

                let _ = response_tx.send(result);
            }
            OrchestrationCommand::UnregisterService { service_id, response_tx } => {
                let result = Self::unregister_service_internal(
                    &services,
                    service_id,
                    &event_tx,
                ).await;

                let _ = response_tx.send(result);
            }
            OrchestrationCommand::StartService { service_id, response_tx } => {
                let result = Self::start_service_internal(
                    &services,
                    &dependencies,
                    service_id,
                    &event_tx,
                    &metrics,
                ).await;

                let _ = response_tx.send(result);
            }
            OrchestrationCommand::StopService { service_id, response_tx } => {
                let result = Self::stop_service_internal(
                    &services,
                    service_id,
                    &event_tx,
                    &metrics,
                ).await;

                let _ = response_tx.send(result);
            }
            OrchestrationCommand::GetServices { response_tx } => {
                let services_vec = services.read().await.values().cloned().collect();
                let _ = response_tx.send(services_vec);
            }
            OrchestrationCommand::GetMetrics { response_tx } => {
                let metrics_snapshot = metrics.read().await.clone();
                let _ = response_tx.send(metrics_snapshot);
            }
            OrchestrationCommand::HealthCheck { service_id, response_tx } => {
                let result = Self::perform_health_check_internal(
                    &services,
                    service_id,
                    &metrics,
                ).await;

                let _ = response_tx.send(result);
            }
        }
    }

    /// Internal service registration
    async fn register_service_internal(
        services: &Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        dependencies: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
        service_info: ServiceInfo,
        service: Arc<dyn BaseService>,
        event_tx: &broadcast::Sender<ServiceEvent>,
        config_manager: &Arc<ConfigManager>,
    ) -> Result<()> {
        let service_id = service_info.id;

        // Check if service already exists
        if services.read().await.contains_key(&service_id) {
            return Err(anyhow::anyhow!("Service {} already registered", service_id));
        }

        // Create service instance
        let instance = ServiceInstance {
            service_info: service_info.clone(),
            service,
            started_at: None,
            last_health_check: None,
            health_status: false,
            restart_count: 0,
            dependencies: HashSet::new(),
        };

        // Register service
        services.write().await.insert(service_id, instance);

        info!("Registered service: {} ({})", service_id, service_info.name);

        // Send event
        let event = ServiceEvent::Registered {
            service_id,
            service_type: format!("{:?}", service_info.service_type),
        };
        let _ = event_tx.send(event);

        // Auto-start service if configured
        let config = config_manager.get().await;
        if config.services.orchestration.restart_policy != crate::config::RestartPolicy::Never {
            if let Err(e) = Self::start_service_internal(
                services,
                dependencies,
                service_id,
                event_tx,
                &Arc::new(RwLock::new(OrchestrationMetrics::default())),
            ).await {
                error!("Failed to auto-start service {}: {}", service_id, e);
            }
        }

        Ok(())
    }

    /// Internal service unregistration
    async fn unregister_service_internal(
        services: &Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        service_id: uuid::Uuid,
        event_tx: &broadcast::Sender<ServiceEvent>,
    ) -> Result<bool> {
        // Stop service first
        if let Some(instance) = services.read().await.get(&service_id) {
            if let Err(e) = instance.service.stop().await {
                warn!("Failed to stop service {} during unregistration: {}", service_id, e);
            }
        }

        // Remove service
        let removed = services.write().await.remove(&service_id).is_some();

        if removed {
            info!("Unregistered service: {}", service_id);

            // Send event
            let event = ServiceEvent::Stopped {
                service_id,
                service_type: "unknown".to_string(), // We don't have the service type after removal
            };
            let _ = event_tx.send(event);
        }

        Ok(removed)
    }

    /// Internal service start
    async fn start_service_internal(
        services: &Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        dependencies: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
        service_id: uuid::Uuid,
        event_tx: &broadcast::Sender<ServiceEvent>,
        metrics: &Arc<RwLock<OrchestrationMetrics>>,
    ) -> Result<()> {
        let service_info = {
            let services_read = services.read().await;
            match services_read.get(&service_id) {
                Some(instance) => instance.service_info.clone(),
                None => return Err(anyhow::anyhow!("Service {} not found", service_id)),
            }
        };

        // Check dependencies
        let deps = dependencies.read().await;
        if let Some(service_deps) = deps.get(&service_info.name) {
            for dep in service_deps {
                let services_read = services.read().await;
                let dep_found = services_read.values().any(|instance| {
                    instance.service_info.name == *dep && instance.started_at.is_some()
                });

                if !dep_found {
                    return Err(anyhow::anyhow!("Dependency {} not satisfied for service {}", dep, service_id));
                }
            }
        }

        // Start service
        let services_read = services.read().await;
        if let Some(instance) = services_read.get(&service_id) {
            match instance.service.start().await {
                Ok(()) => {
                    // Update instance state
                    drop(services_read);
                    let mut services_write = services.write().await;
                    if let Some(instance) = services_write.get_mut(&service_id) {
                        instance.started_at = Some(Instant::now());
                        instance.health_status = true;
                    }

                    // Update metrics
                    metrics.write().await.services_started += 1;

                    info!("Started service: {} ({})", service_id, service_info.name);

                    // Send event
                    let event = ServiceEvent::Started {
                        service_id,
                        service_type: format!("{:?}", service_info.service_type),
                    };
                    let _ = event_tx.send(event);

                    Ok(())
                }
                Err(e) => {
                    // Update metrics
                    metrics.write().await.services_failed += 1;

                    error!("Failed to start service {}: {}", service_id, e);

                    // Send event
                    let event = ServiceEvent::Failed {
                        service_id,
                        service_type: format!("{:?}", service_info.service_type),
                        error: e.to_string(),
                    };
                    let _ = event_tx.send(event);

                    Err(anyhow::anyhow!("Service start failed: {}", e))
                }
            }
        } else {
            Err(anyhow::anyhow!("Service {} not found", service_id))
        }
    }

    /// Internal service stop
    async fn stop_service_internal(
        services: &Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        service_id: uuid::Uuid,
        event_tx: &broadcast::Sender<ServiceEvent>,
        metrics: &Arc<RwLock<OrchestrationMetrics>>,
    ) -> Result<()> {
        let services_read = services.read().await;
        if let Some(instance) = services_read.get(&service_id) {
            let service_info = instance.service_info.clone();
            match instance.service.stop().await {
                Ok(()) => {
                    // Update instance state
                    drop(services_read);
                    let mut services_write = services.write().await;
                    if let Some(instance) = services_write.get_mut(&service_id) {
                        instance.started_at = None;
                        instance.health_status = false;
                    }

                    // Update metrics
                    metrics.write().await.services_stopped += 1;

                    info!("Stopped service: {} ({})", service_id, service_info.name);

                    // Send event
                    let event = ServiceEvent::Stopped {
                        service_id,
                        service_type: format!("{:?}", service_info.service_type),
                    };
                    let _ = event_tx.send(event);

                    Ok(())
                }
                Err(e) => {
                    error!("Failed to stop service {}: {}", service_id, e);
                    Err(anyhow::anyhow!("Service stop failed: {}", e))
                }
            }
        } else {
            Err(anyhow::anyhow!("Service {} not found", service_id))
        }
    }

    /// Internal health check
    async fn perform_health_check_internal(
        services: &Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
        service_id: Option<uuid::Uuid>,
        metrics: &Arc<RwLock<OrchestrationMetrics>>,
    ) -> Result<HashMap<uuid::Uuid, bool>> {
        let services_read = services.read().await;
        let mut results = HashMap::new();

        let services_to_check = if let Some(id) = service_id {
            vec![(id, services_read.get(&id).cloned())]
        } else {
            services_read.iter().map(|(id, instance)| (*id, Some(instance.clone()))).collect()
        };

        drop(services_read);

        for (id, instance) in services_to_check {
            if let Some(instance) = instance {
                let healthy = match instance.service.health_check().await {
                    Ok(health) => health,
                    Err(e) => {
                        warn!("Health check failed for service {}: {}", id, e);
                        false
                    }
                };

                results.insert(id, healthy);

                // Update metrics
                let mut metrics_write = metrics.write().await;
                if healthy {
                    metrics_write.health_checks_passed += 1;
                } else {
                    metrics_write.health_checks_failed += 1;
                }

                // Update instance health status
                let mut services_write = services.write().await;
                if let Some(instance) = services_write.get_mut(&id) {
                    let old_health = instance.health_status;
                    instance.health_status = healthy;
                    instance.last_health_check = Some(Instant::now());

                    // Send health change event if status changed
                    if old_health != healthy {
                        // Note: This would need access to event_tx
                    }
                }
            }
        }

        Ok(results)
    }

    /// Start health check task
    fn start_health_check_task(&self) {
        let services = self.services.clone();
        let metrics = self.metrics.clone();
        let event_tx = self.event_tx.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30)); // Health check every 30 seconds

            while *running.read().await {
                interval.tick().await;

                let services_to_check: Vec<_> = {
                    services.read().await.keys().copied().collect()
                };

                for service_id in services_to_check {
                    match Self::perform_health_check_internal(
                        &services,
                        Some(service_id),
                        &metrics,
                    ).await {
                        Ok(mut results) => {
                            if let Some(&healthy) = results.get(&service_id) {
                                // Send health change event if needed
                                let services_read = services.read().await;
                                if let Some(instance) = services_read.get(&service_id) {
                                    if instance.health_status != healthy {
                                        let event = ServiceEvent::HealthChanged {
                                            service_id,
                                            service_type: format!("{:?}", instance.service_info.service_type),
                                            healthy,
                                        };
                                        let _ = event_tx.send(event);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Health check failed for service {}: {}", service_id, e);
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::tests::MockService;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = ServiceOrchestrator::new(config_manager).await;

        assert!(orchestrator.is_ok());
    }

    #[tokio::test]
    async fn test_service_registration() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = ServiceOrchestrator::new(config_manager).await.unwrap();

        let service_info = ServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockService);

        // Register service
        orchestrator.register_service(service_info.clone(), mock_service).await.unwrap();

        // Get services
        let services = orchestrator.get_services().await;
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].service_info.name, "test-service");
    }

    #[tokio::test]
    async fn test_service_lifecycle() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = ServiceOrchestrator::new(config_manager).await.unwrap();

        let service_info = ServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockService);

        // Register service
        orchestrator.register_service(service_info.clone(), mock_service).await.unwrap();

        let service_id = service_info.id;

        // Start service
        orchestrator.start_service(service_id).await.unwrap();

        // Health check
        let healthy = orchestrator.health_check_service(service_id).await.unwrap();
        assert!(healthy);

        // Stop service
        orchestrator.stop_service(service_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_orchestrator_metrics() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = ServiceOrchestrator::new(config_manager).await.unwrap();

        let metrics = orchestrator.get_metrics().await;
        assert_eq!(metrics.services_started, 0);
        assert_eq!(metrics.services_stopped, 0);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = ServiceOrchestrator::new(config_manager).await.unwrap();
        let mut events = orchestrator.subscribe_events();

        let service_info = ServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockService);

        // Register service
        orchestrator.register_service(service_info.clone(), mock_service).await.unwrap();

        // Should receive registered event
        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            ServiceEvent::Registered { service_id, service_type } => {
                assert_eq!(service_id, service_info.id);
                assert_eq!(service_type, "Tool");
            }
            _ => panic!("Expected registered event"),
        }
    }
}