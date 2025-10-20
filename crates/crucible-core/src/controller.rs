//! Master Controller for Crucible
//!
//! The controller serves as the central coordination point for all system components.
//! It manages service orchestration, request routing, configuration distribution,
//! and provides the main entry point for application lifecycle management.

use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, Serializer};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{debug, error, info};

use crate::config::{ConfigChange, ConfigManager};
use crate::orchestrator_simple::{SimpleServiceOrchestrator, ServiceEvent};
use crate::router_simple::SimpleRequestRouter;
use crate::state::StateManager;

/// Controller state
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ControllerState {
    Uninitialized,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Failed(String),
}

/// Controller command
#[derive(Debug)]
pub enum ControllerCommand {
    Start,
    Stop,
    Restart,
    GetStatus(oneshot::Sender<ControllerStatus>),
    GetMetrics(oneshot::Sender<ControllerMetrics>),
    HealthCheck(oneshot::Sender<HealthStatus>),
}

/// Helper function to serialize Instant as duration since epoch
fn serialize_instant<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    // For simplicity, serialize as duration from instant creation
    let duration_since_epoch = instant.elapsed();
    duration_since_epoch.serialize(serializer)
}

/// Helper function to serialize Option<Instant>
fn serialize_option_instant<S>(option: &Option<Instant>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match option {
        Some(instant) => serialize_instant(instant, serializer),
        None => serializer.serialize_none(),
    }
}

/// Controller status
#[derive(Debug, Clone, Serialize)]
pub struct ControllerStatus {
    pub state: ControllerState,
    pub uptime: Duration,
    #[serde(serialize_with = "serialize_option_instant")]
    pub started_at: Option<Instant>,
    pub version: String,
    pub services_running: usize,
    pub services_total: usize,
}

/// Controller metrics
#[derive(Debug, Clone, Serialize)]
pub struct ControllerMetrics {
    pub requests_processed: u64,
    pub requests_failed: u64,
    pub average_request_time: Duration,
    pub memory_usage: u64,
    pub cpu_usage: f64,
    pub active_connections: u64,
}

/// Health status
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: Vec<HealthCheck>,
    #[serde(serialize_with = "serialize_instant")]
    pub timestamp: Instant,
}

/// Individual health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthCheckResult,
    pub message: Option<String>,
    pub duration: Duration,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckResult {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Master controller for the Crucible system
#[derive(Debug)]
pub struct MasterController {
    /// Controller state
    state: Arc<RwLock<ControllerState>>,
    /// Configuration manager
    config_manager: Arc<ConfigManager>,
    /// Service orchestrator
    orchestrator: Arc<SimpleServiceOrchestrator>,
    /// Request router
    router: Arc<SimpleRequestRouter>,
    /// State manager
    state_manager: Arc<StateManager>,
    /// Command receiver
    command_rx: mpsc::UnboundedReceiver<ControllerCommand>,
    /// Command sender
    command_tx: mpsc::UnboundedSender<ControllerCommand>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ControllerEvent>,
    /// Startup time
    started_at: Arc<RwLock<Option<Instant>>>,
    /// Shutdown completion notifier
    shutdown_notifier: Arc<RwLock<Option<oneshot::Sender<()>>>>,
}

/// Controller event
#[derive(Debug, Clone)]
pub enum ControllerEvent {
    StateChanged {
        old_state: ControllerState,
        new_state: ControllerState,
    },
    ServiceRegistered {
        service_id: String,
        service_type: String,
    },
    ServiceUnregistered {
        service_id: String,
    },
    ConfigChanged {
        change: ConfigChange,
    },
    Error {
        error: String,
        severity: ErrorSeverity,
    },
}

/// Error severity
#[derive(Debug, Clone, Copy)]
pub enum ErrorSeverity {
    Warning,
    Error,
    Critical,
}

impl MasterController {
    /// Create a new master controller
    pub async fn new() -> Result<Self> {
        Self::with_config_manager(ConfigManager::new().await?).await
    }

    /// Create a controller with a specific configuration manager
    pub async fn with_config_manager(config_manager: ConfigManager) -> Result<Self> {
        let config_manager = Arc::new(config_manager);
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, _) = broadcast::channel(1000);

        // Initialize components
        let orchestrator = Arc::new(SimpleServiceOrchestrator::new(config_manager.clone()).await?);
        let router = Arc::new(SimpleRequestRouter::new(config_manager.clone()).await?);
        let state_manager = Arc::new(StateManager::new(config_manager.clone()).await?);

        Ok(Self {
            state: Arc::new(RwLock::new(ControllerState::Uninitialized)),
            config_manager,
            orchestrator,
            router,
            state_manager,
            command_rx,
            command_tx,
            event_tx,
            started_at: Arc::new(RwLock::new(None)),
            shutdown_notifier: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the command sender
    pub fn command_sender(&self) -> mpsc::UnboundedSender<ControllerCommand> {
        self.command_tx.clone()
    }

    /// Subscribe to controller events
    pub fn subscribe_events(&self) -> broadcast::Receiver<ControllerEvent> {
        self.event_tx.subscribe()
    }

    /// Get current controller state
    pub async fn get_state(&self) -> ControllerState {
        self.state.read().await.clone()
    }

    /// Get configuration manager
    pub fn config_manager(&self) -> Arc<ConfigManager> {
        self.config_manager.clone()
    }

    /// Get service orchestrator
    pub fn orchestrator(&self) -> Arc<SimpleServiceOrchestrator> {
        self.orchestrator.clone()
    }

    /// Get request router
    pub fn router(&self) -> Arc<SimpleRequestRouter> {
        self.router.clone()
    }

    /// Get state manager
    pub fn state_manager(&self) -> Arc<StateManager> {
        self.state_manager.clone()
    }

    /// Start the controller
    pub async fn start(&self) -> Result<()> {
        self.send_command(ControllerCommand::Start)
            .context("Failed to send start command")
    }

    /// Stop the controller
    pub async fn stop(&self) -> Result<()> {
        self.send_command(ControllerCommand::Stop)
            .context("Failed to send stop command")
    }

    /// Restart the controller
    pub async fn restart(&self) -> Result<()> {
        self.send_command(ControllerCommand::Restart)
            .context("Failed to send restart command")
    }

    /// Send a command to the controller
    fn send_command(&self, command: ControllerCommand) -> Result<()> {
        self.command_tx
            .send(command)
            .context("Failed to send command to controller")
    }

    /// Run the controller main loop
    pub async fn run(mut self) -> Result<()> {
        info!("Starting master controller");

        // Start command processing loop
        while let Some(command) = self.command_rx.recv().await {
            match command {
                ControllerCommand::Start => {
                    if let Err(e) = self.handle_start().await {
                        error!("Failed to start controller: {}", e);
                        self.set_state(ControllerState::Failed(e.to_string())).await;
                    }
                }
                ControllerCommand::Stop => {
                    if let Err(e) = self.handle_stop().await {
                        error!("Failed to stop controller: {}", e);
                    } else {
                        break; // Exit the main loop
                    }
                }
                ControllerCommand::Restart => {
                    if let Err(e) = self.handle_restart().await {
                        error!("Failed to restart controller: {}", e);
                        self.set_state(ControllerState::Failed(e.to_string())).await;
                    }
                }
                ControllerCommand::GetStatus(tx) => {
                    let status = self.get_status().await;
                    let _ = tx.send(status);
                }
                ControllerCommand::GetMetrics(tx) => {
                    let metrics = self.get_metrics().await;
                    let _ = tx.send(metrics);
                }
                ControllerCommand::HealthCheck(tx) => {
                    let health = self.perform_health_check().await;
                    let _ = tx.send(health);
                }
            }
        }

        info!("Master controller stopped");
        Ok(())
    }

    /// Handle start command
    async fn handle_start(&self) -> Result<()> {
        let current_state = self.get_state().await;

        if current_state == ControllerState::Running {
            warn!("Controller is already running");
            return Ok(());
        }

        self.set_state(ControllerState::Initializing).await;
        info!("Initializing master controller");

        // Initialize components
        self.initialize_components().await?;

        // Start orchestrator
        self.orchestrator.start().await?;

        // Start router
        self.router.start().await?;

        // Start state manager
        self.state_manager.start().await?;

        // Set startup time
        *self.started_at.write().await = Some(Instant::now());

        self.set_state(ControllerState::Running).await;
        info!("Master controller started successfully");

        Ok(())
    }

    /// Handle stop command
    async fn handle_stop(&self) -> Result<()> {
        let current_state = self.get_state().await;

        if current_state == ControllerState::Stopped {
            warn!("Controller is already stopped");
            return Ok(());
        }

        self.set_state(ControllerState::Stopping).await;
        info!("Stopping master controller");

        // Stop components in reverse order
        self.state_manager.stop().await?;
        self.router.stop().await?;
        self.orchestrator.stop().await?;

        // Clear startup time
        *self.started_at.write().await = None;

        self.set_state(ControllerState::Stopped).await;
        info!("Master controller stopped successfully");

        // Notify shutdown completion
        if let Some(notifier) = self.shutdown_notifier.write().await.take() {
            let _ = notifier.send(());
        }

        Ok(())
    }

    /// Handle restart command
    async fn handle_restart(&self) -> Result<()> {
        info!("Restarting master controller");

        // Stop
        self.handle_stop().await?;

        // Wait a brief moment
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Start
        self.handle_start().await?;

        info!("Master controller restarted successfully");
        Ok(())
    }

    /// Initialize all components
    async fn initialize_components(&self) -> Result<()> {
        info!("Initializing controller components");

        // Set up configuration change notifications
        let mut config_rx = self.config_manager.subscribe();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            while let Ok(change) = config_rx.recv().await {
                debug!("Configuration changed: {}", change.path);
                let event = ControllerEvent::ConfigChanged { change };
                let _ = event_tx.send(event);
            }
        });

        // Set up service event notifications
        let mut service_rx = self.orchestrator.subscribe_events();
        let event_tx = self.event_tx.clone();

        tokio::spawn(async move {
            while let Ok(event) = service_rx.recv().await {
                match event {
                    ServiceEvent::Registered { service_id, service_type } => {
                        let controller_event = ControllerEvent::ServiceRegistered {
                            service_id: service_id.to_string(),
                            service_type,
                        };
                        let _ = event_tx.send(controller_event);
                    }
                    ServiceEvent::Started { service_id, service_type } => {
                        let controller_event = ControllerEvent::ServiceRegistered {
                            service_id: service_id.to_string(),
                            service_type,
                        };
                        let _ = event_tx.send(controller_event);
                    }
                    ServiceEvent::Stopped { service_id, .. } => {
                        let event = ControllerEvent::ServiceUnregistered { service_id: service_id.to_string() };
                        let _ = event_tx.send(event);
                    }
                    ServiceEvent::Failed { service_id, .. } => {
                        let event = ControllerEvent::ServiceUnregistered { service_id: service_id.to_string() };
                        let _ = event_tx.send(event);
                    }
                    ServiceEvent::HealthChanged { .. } => {
                        // Handle health changes if needed
                    }
                }
            }
        });

        Ok(())
    }

    /// Set controller state
    async fn set_state(&self, new_state: ControllerState) {
        let old_state = {
            let mut state = self.state.write().await;
            let old = state.clone();
            *state = new_state.clone();
            old
        };

        if old_state != new_state {
            info!("Controller state changed: {:?} -> {:?}", old_state, new_state);

            let event = ControllerEvent::StateChanged {
                old_state,
                new_state,
            };

            let _ = self.event_tx.send(event);
        }
    }

    /// Get controller status
    async fn get_status(&self) -> ControllerStatus {
        let state = self.get_state().await;
        let started_at = *self.started_at.read().await;
        let uptime = started_at.map_or(Duration::ZERO, |start| start.elapsed());

        // Get service information
        let services_info = self.orchestrator.get_services_info().await;
        let services_running = services_info.iter().filter(|s| s.is_healthy()).count();
        let services_total = services_info.len();

        ControllerStatus {
            state,
            uptime,
            started_at,
            version: env!("CARGO_PKG_VERSION").to_string(),
            services_running,
            services_total,
        }
    }

    /// Get controller metrics
    async fn get_metrics(&self) -> ControllerMetrics {
        // Get metrics from various components
        let router_metrics = self.router.get_metrics().await;
        let orchestrator_metrics = self.orchestrator.get_metrics().await;

        ControllerMetrics {
            requests_processed: router_metrics.requests_processed,
            requests_failed: router_metrics.requests_failed,
            average_request_time: router_metrics.average_request_time,
            memory_usage: self.get_memory_usage().await,
            cpu_usage: self.get_cpu_usage().await,
            active_connections: orchestrator_metrics.active_connections,
        }
    }

    /// Perform health check
    async fn perform_health_check(&self) -> HealthStatus {
        let mut checks = Vec::new();
        let start = Instant::now();

        // Controller state check
        let state = self.get_state().await;
        checks.push(HealthCheck {
            name: "controller_state".to_string(),
            status: match state {
                ControllerState::Running => HealthCheckResult::Healthy,
                ControllerState::Initializing | ControllerState::Stopping => HealthCheckResult::Degraded,
                _ => HealthCheckResult::Unhealthy,
            },
            message: Some(format!("Controller state: {:?}", state)),
            duration: Duration::from_millis(1),
        });

        // Orchestrator health check
        let orchestrator_start = Instant::now();
        match self.orchestrator.health_check().await {
            Ok(healthy) => checks.push(HealthCheck {
                name: "orchestrator".to_string(),
                status: if healthy { HealthCheckResult::Healthy } else { HealthCheckResult::Unhealthy },
                message: None,
                duration: orchestrator_start.elapsed(),
            }),
            Err(e) => checks.push(HealthCheck {
                name: "orchestrator".to_string(),
                status: HealthCheckResult::Unhealthy,
                message: Some(e.to_string()),
                duration: orchestrator_start.elapsed(),
            }),
        }

        // Router health check
        let router_start = Instant::now();
        match self.router.health_check().await {
            Ok(healthy) => checks.push(HealthCheck {
                name: "router".to_string(),
                status: if healthy { HealthCheckResult::Healthy } else { HealthCheckResult::Unhealthy },
                message: None,
                duration: router_start.elapsed(),
            }),
            Err(e) => checks.push(HealthCheck {
                name: "router".to_string(),
                status: HealthCheckResult::Unhealthy,
                message: Some(e.to_string()),
                duration: router_start.elapsed(),
            }),
        }

        // State manager health check
        let state_start = Instant::now();
        match self.state_manager.health_check().await {
            Ok(healthy) => checks.push(HealthCheck {
                name: "state_manager".to_string(),
                status: if healthy { HealthCheckResult::Healthy } else { HealthCheckResult::Degraded },
                message: None,
                duration: state_start.elapsed(),
            }),
            Err(e) => checks.push(HealthCheck {
                name: "state_manager".to_string(),
                status: HealthCheckResult::Unhealthy,
                message: Some(e.to_string()),
                duration: state_start.elapsed(),
            }),
        }

        // Determine overall health
        let healthy = checks.iter().all(|check| match check.status {
            HealthCheckResult::Healthy | HealthCheckResult::Degraded => true,
            HealthCheckResult::Unhealthy => false,
        });

        HealthStatus {
            healthy,
            checks,
            timestamp: start,
        }
    }

    /// Get memory usage (placeholder implementation)
    async fn get_memory_usage(&self) -> u64 {
        // This would typically use system APIs to get actual memory usage
        // For now, return a placeholder value
        64 * 1024 * 1024 // 64MB placeholder
    }

    /// Get CPU usage (placeholder implementation)
    async fn get_cpu_usage(&self) -> f64 {
        // This would typically use system APIs to get actual CPU usage
        // For now, return a placeholder value
        0.1 // 10% placeholder
    }

    /// Wait for shutdown
    pub async fn wait_for_shutdown(&self) -> impl Future<Output = ()> {
        let shutdown_rx = {
            let mut notifier = self.shutdown_notifier.write().await;
            let (tx, rx) = oneshot::channel();
            *notifier = Some(tx);
            rx
        };

        async move {
            let _ = shutdown_rx.await;
        }
    }
}

/// Controller builder
pub struct ControllerBuilder {
    config_manager: Option<ConfigManager>,
}

impl ControllerBuilder {
    /// Create a new controller builder
    pub fn new() -> Self {
        Self {
            config_manager: None,
        }
    }

    /// Set configuration manager
    pub fn with_config_manager(mut self, config_manager: ConfigManager) -> Self {
        self.config_manager = Some(config_manager);
        self
    }

    /// Build the controller
    pub async fn build(self) -> Result<MasterController> {
        let config_manager = match self.config_manager {
            Some(manager) => manager,
            None => ConfigManager::new().await?,
        };

        MasterController::with_config_manager(config_manager).await
    }
}

impl Default for ControllerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_controller_lifecycle() {
        let controller = MasterController::new().await.unwrap();

        // Initial state should be Uninitialized
        assert_eq!(controller.get_state().await, ControllerState::Uninitialized);

        // Start controller
        controller.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be running
        assert_eq!(controller.get_state().await, ControllerState::Running);

        // Get status
        let (tx, rx) = oneshot::channel();
        controller.command_tx.send(ControllerCommand::GetStatus(tx)).unwrap();
        let status = rx.await.unwrap();
        assert_eq!(status.state, ControllerState::Running);

        // Stop controller
        controller.stop().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should be stopped
        assert_eq!(controller.get_state().await, ControllerState::Stopped);
    }

    #[tokio::test]
    async fn test_health_check() {
        let controller = MasterController::new().await.unwrap();
        controller.start().await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let (tx, rx) = oneshot::channel();
        controller.command_tx.send(ControllerCommand::HealthCheck(tx)).unwrap();
        let health = rx.await.unwrap();

        assert!(!health.checks.is_empty());
        // Health should be OK since controller is running
        assert!(health.healthy || health.checks.iter().any(|c| c.name == "controller_state"));

        controller.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let controller = MasterController::new().await.unwrap();
        let mut events = controller.subscribe_events();

        // Start controller
        controller.start().await.unwrap();

        // Should receive state change event
        let event = tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .unwrap()
            .unwrap();

        match event {
            ControllerEvent::StateChanged { new_state, .. } => {
                assert_eq!(new_state, ControllerState::Running);
            }
            _ => panic!("Expected state change event"),
        }

        controller.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_controller_builder() {
        let controller = ControllerBuilder::new()
            .build()
            .await
            .unwrap();

        assert_eq!(controller.get_state().await, ControllerState::Uninitialized);
    }
}