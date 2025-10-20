//! Simplified service orchestration system for Crucible
//!
//! This module provides basic service lifecycle management that doesn't depend on
//! the external crucible-services crate, avoiding cyclic dependencies.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{error, info, warn};

use crate::config::ConfigManager;

/// Simple service type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ServiceType {
    Tool,
    Database,
    LLM,
    Config,
    FileSystem,
    Network,
    Custom(String),
}

/// Simple service status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ServiceStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed(String),
}

/// Simple service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub id: uuid::Uuid,
    pub name: String,
    pub service_type: ServiceType,
    pub version: String,
    pub description: Option<String>,
    pub status: ServiceStatus,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl ServiceInfo {
    /// Check if the service is healthy based on its status
    pub fn is_healthy(&self) -> bool {
        matches!(self.status, ServiceStatus::Running)
    }
}

/// Simple service trait
#[async_trait::async_trait]
pub trait SimpleService: Send + Sync {
    fn service_info(&self) -> ServiceInfo;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn health_check(&self) -> Result<bool>;
}

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
#[derive(Clone)]
pub struct ServiceInstance {
    pub service_info: ServiceInfo,
    pub service: Arc<dyn SimpleService>,
    pub started_at: Option<Instant>,
    pub last_health_check: Option<Instant>,
    pub health_status: bool,
    pub restart_count: u32,
}

impl std::fmt::Debug for ServiceInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceInstance")
            .field("service_info", &self.service_info)
            .field("started_at", &self.started_at)
            .field("last_health_check", &self.last_health_check)
            .field("health_status", &self.health_status)
            .field("restart_count", &self.restart_count)
            .finish()
    }
}

/// Service orchestration command
pub enum OrchestrationCommand {
    RegisterService {
        service_info: ServiceInfo,
        service: Arc<dyn SimpleService>,
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

impl std::fmt::Debug for OrchestrationCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrchestrationCommand::RegisterService { service_info, .. } => {
                write!(f, "RegisterService {{ service_info: {:?} }}", service_info)
            }
            OrchestrationCommand::UnregisterService { service_id, .. } => {
                write!(f, "UnregisterService {{ service_id: {:?} }}", service_id)
            }
            OrchestrationCommand::StartService { service_id, .. } => {
                write!(f, "StartService {{ service_id: {:?} }}", service_id)
            }
            OrchestrationCommand::StopService { service_id, .. } => {
                write!(f, "StopService {{ service_id: {:?} }}", service_id)
            }
            OrchestrationCommand::GetServices { .. } => write!(f, "GetServices"),
            OrchestrationCommand::GetMetrics { .. } => write!(f, "GetMetrics"),
            OrchestrationCommand::HealthCheck { service_id, .. } => {
                write!(f, "HealthCheck {{ service_id: {:?} }}", service_id)
            }
        }
    }
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

/// Simple service orchestrator
#[derive(Debug)]
pub struct SimpleServiceOrchestrator {
    /// Configuration manager
    config_manager: Arc<ConfigManager>,
    /// Registered services
    services: Arc<RwLock<HashMap<uuid::Uuid, ServiceInstance>>>,
    /// Command receiver
    command_rx: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<OrchestrationCommand>>>,
    /// Command sender
    command_tx: mpsc::UnboundedSender<OrchestrationCommand>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ServiceEvent>,
    /// Orchestration metrics
    metrics: Arc<RwLock<OrchestrationMetrics>>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

impl SimpleServiceOrchestrator {
    /// Create a new simple service orchestrator
    pub async fn new(config_manager: Arc<ConfigManager>) -> Result<Self> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1000);

        Ok(Self {
            config_manager,
            services: Arc::new(RwLock::new(HashMap::new())),
            command_rx: Arc::new(tokio::sync::Mutex::new(command_rx)),
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
        info!("Simple service orchestrator started");

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

        info!("Simple service orchestrator stopped");
        Ok(())
    }

    /// Register a new service
    pub async fn register_service(
        &self,
        service_info: ServiceInfo,
        service: Arc<dyn SimpleService>,
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
        let event_tx = self.event_tx.clone();
        let metrics = self.metrics.clone();
        let config_manager = self.config_manager.clone();
        let running = self.running.clone();
        let command_rx = self.command_rx.clone();

        tokio::spawn(async move {
            while *running.read().await {
                let command = {
                    let mut rx = command_rx.lock().await;
                    rx.recv().await
                };

                match command {
                    Some(command) => {
                        Self::handle_command(
                            command,
                            services.clone(),
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
        event_tx: broadcast::Sender<ServiceEvent>,
        metrics: Arc<RwLock<OrchestrationMetrics>>,
        config_manager: Arc<ConfigManager>,
    ) {
        match command {
            OrchestrationCommand::RegisterService { service_info, service, response_tx } => {
                let result = Self::register_service_internal(
                    &services,
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
        service_info: ServiceInfo,
        service: Arc<dyn SimpleService>,
        event_tx: &broadcast::Sender<ServiceEvent>,
        _config_manager: &Arc<ConfigManager>,
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
                        Ok(results) => {
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

/// Service registration builder
pub struct SimpleServiceRegistrationBuilder {
    service_info: ServiceInfo,
}

impl SimpleServiceRegistrationBuilder {
    /// Create a new service registration builder
    pub fn new(name: String, service_type: ServiceType) -> Self {
        Self {
            service_info: ServiceInfo {
                id: uuid::Uuid::new_v4(),
                name,
                service_type,
                version: "1.0.0".to_string(),
                description: None,
                status: ServiceStatus::Starting,
                capabilities: Vec::new(),
                metadata: HashMap::new(),
            },
        }
    }

    /// Set service version
    pub fn with_version(mut self, version: String) -> Self {
        self.service_info.version = version;
        self
    }

    /// Set service description
    pub fn with_description(mut self, description: String) -> Self {
        self.service_info.description = Some(description);
        self
    }

    /// Add service capability
    pub fn with_capability(mut self, capability: String) -> Self {
        self.service_info.capabilities.push(capability);
        self
    }

    /// Add service metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.service_info.metadata.insert(key, value);
        self
    }

    /// Set service status
    pub fn with_status(mut self, status: ServiceStatus) -> Self {
        self.service_info.status = status;
        self
    }

    /// Build service info
    pub fn build(self) -> ServiceInfo {
        self.service_info
    }
}

/// Mock service for testing
#[cfg(test)]
pub struct MockSimpleService {
    service_info: ServiceInfo,
}

#[cfg(test)]
impl MockSimpleService {
    pub fn new(name: String, service_type: ServiceType) -> Self {
        Self {
            service_info: SimpleServiceRegistrationBuilder::new(name, service_type)
                .with_status(ServiceStatus::Running)
                .build(),
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl SimpleService for MockSimpleService {
    fn service_info(&self) -> ServiceInfo {
        self.service_info.clone()
    }

    async fn start(&self) -> Result<()> {
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        Ok(())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_simple_orchestrator_creation() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = SimpleServiceOrchestrator::new(config_manager).await;

        assert!(orchestrator.is_ok());
    }

    #[tokio::test]
    async fn test_service_registration() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = SimpleServiceOrchestrator::new(config_manager).await.unwrap();

        let service_info = SimpleServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockSimpleService::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

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
        let orchestrator = SimpleServiceOrchestrator::new(config_manager).await.unwrap();

        let service_info = SimpleServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockSimpleService::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

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
        let orchestrator = SimpleServiceOrchestrator::new(config_manager).await.unwrap();

        let metrics = orchestrator.get_metrics().await;
        assert_eq!(metrics.services_started, 0);
        assert_eq!(metrics.services_stopped, 0);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let config_manager = Arc::new(ConfigManager::new().await.unwrap());
        let orchestrator = SimpleServiceOrchestrator::new(config_manager).await.unwrap();
        let mut events = orchestrator.subscribe_events();

        let service_info = SimpleServiceRegistrationBuilder::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ).build();

        let mock_service = Arc::new(MockSimpleService::new(
            "test-service".to_string(),
            ServiceType::Tool,
        ));

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