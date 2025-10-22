//! # Plugin Instance Management
//!
//! This module implements the PluginInstance system which handles individual plugin
//! process lifecycle, communication, and monitoring.

use super::config::SandboxConfig;
use super::error::{PluginError, PluginResult, ErrorContext};
use super::types::*;
// use super::plugin_ipc::{IpcClient, IpcServer, PluginMessage, PluginMessageType};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Stdio, Command as StdCommand};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{interval, timeout};
use tracing::{debug, error, info, warn};

/// ============================================================================
/// PLUGIN INSTANCE TRAIT
/// ============================================================================

#[async_trait]
pub trait PluginInstance: Send + Sync {
    /// Get instance ID
    fn instance_id(&self) -> &str;

    /// Get plugin ID
    fn plugin_id(&self) -> &str;

    /// Get current state
    async fn get_state(&self) -> PluginInstanceState;

    /// Start the plugin instance
    async fn start(&mut self) -> PluginResult<()>;

    /// Stop the plugin instance
    async fn stop(&mut self) -> PluginResult<()>;

    /// Restart the plugin instance
    async fn restart(&mut self) -> PluginResult<()>;

    /// Send a message to the plugin
    async fn send_message(&self, message: PluginMessage) -> PluginResult<PluginMessage>;

    /// Get resource usage
    async fn get_resource_usage(&self) -> PluginResult<ResourceUsage>;

    /// Get health status
    async fn get_health_status(&self) -> PluginResult<PluginHealthStatus>;

    /// Update configuration
    async fn update_config(&mut self, config: HashMap<String, serde_json::Value>) -> PluginResult<()>;

    /// Get execution statistics
    async fn get_execution_stats(&self) -> PluginResult<PluginExecutionStats>;
}

/// ============================================================================
/// DEFAULT PLUGIN INSTANCE IMPLEMENTATION
/// ============================================================================

/// Default implementation of PluginInstance
#[derive(Debug)]
pub struct DefaultPluginInstance {
    /// Instance information
    pub info: super::types::PluginInstance,
    /// Plugin manifest
    manifest: PluginManifest,
    /// Child process handle
    child_process: Arc<Mutex<Option<Child>>>,
    /// IPC client
    ipc_client: Option<Arc<dyn IpcClient>>,
    /// IPC server (for receiving messages)
    ipc_server: Option<Arc<dyn IpcServer>>,
    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<InstanceEvent>>>>,
    /// Configuration
    config: Arc<PluginInstanceConfig>,
    /// Process supervisor
    supervisor: Arc<ProcessSupervisor>,
}

/// Configuration for plugin instance
#[derive(Debug, Clone)]
pub struct PluginInstanceConfig {
    /// Working directory
    pub working_directory: Option<PathBuf>,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Arguments to pass to plugin
    pub arguments: Vec<String>,
    /// Startup timeout
    pub startup_timeout: Duration,
    /// Shutdown timeout
    pub shutdown_timeout: Duration,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Resource monitoring interval
    pub resource_monitoring_interval: Duration,
    /// Auto-restart on failure
    pub auto_restart: bool,
    /// Maximum restart attempts
    pub max_restart_attempts: u32,
    /// Restart delay
    pub restart_delay: Duration,
}

// Placeholder IPC traits for compilation
trait IpcClient {
    async fn send_message(&self, message: super::types::PluginMessage) -> PluginResult<super::types::PluginMessage>;
}

trait IpcServer {
    // Server trait placeholder
}

impl Default for DefaultPluginInstance {
    fn default() -> Self {
        Self::new(
            PluginInstance::default(),
            PluginManifest::default(),
            PluginInstanceConfig::default(),
        )
    }
}

impl Default for PluginInstanceConfig {
    fn default() -> Self {
        Self {
            working_directory: None,
            environment: HashMap::new(),
            arguments: Vec::new(),
            startup_timeout: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(10),
            health_check_interval: Duration::from_secs(10),
            resource_monitoring_interval: Duration::from_secs(5),
            auto_restart: true,
            max_restart_attempts: 3,
            restart_delay: Duration::from_secs(5),
        }
    }
}

impl DefaultPluginInstance {
    /// Create a new plugin instance
    pub fn new(
        mut info: PluginInstance,
        manifest: PluginManifest,
        config: PluginInstanceConfig,
    ) -> Self {
        info.plugin_id = manifest.id.clone();
        info.resource_limits = manifest.resource_limits.clone();
        info.sandbox_config = manifest.sandbox_config.clone();

        Self {
            info,
            manifest,
            child_process: Arc::new(Mutex::new(None)),
            ipc_client: None,
            ipc_server: None,
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            config: Arc::new(config),
            supervisor: Arc::new(ProcessSupervisor::new()),
        }
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: InstanceEvent) {
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

    /// Update instance state
    async fn update_state(&mut self, new_state: PluginInstanceState) {
        let old_state = std::mem::replace(&mut self.info.state, new_state.clone());

        if old_state != new_state {
            debug!("Instance {} state changed: {:?} -> {:?}", self.info.instance_id, old_state, new_state);

            self.publish_event(InstanceEvent::StateChanged {
                instance_id: self.info.instance_id.clone(),
                old_state,
                new_state,
            }).await;

            // Update timestamps
            match new_state {
                PluginInstanceState::Running => {
                    self.info.started_at = Some(SystemTime::now());
                    self.info.last_activity = Some(SystemTime::now());
                }
                PluginInstanceState::Stopped | PluginInstanceState::Error(_) | PluginInstanceState::Crashed => {
                    self.info.last_activity = Some(SystemTime::now());
                }
                _ => {}
            }
        }
    }

    /// Build command to start plugin
    fn build_command(&self) -> PluginResult<TokioCommand> {
        let entry_point = &self.manifest.entry_point;

        let mut cmd = match self.manifest.plugin_type {
            PluginType::Rune => {
                let mut cmd = TokioCommand::new("rune");
                cmd.arg(entry_point);
                cmd
            }
            PluginType::Python => {
                let mut cmd = TokioCommand::new("python3");
                cmd.arg(entry_point);
                cmd
            }
            PluginType::JavaScript => {
                let mut cmd = TokioCommand::new("node");
                cmd.arg(entry_point);
                cmd
            }
            PluginType::Wasm => {
                let mut cmd = TokioCommand::new("wasmtime");
                cmd.arg(entry_point);
                cmd
            }
            PluginType::Binary => {
                TokioCommand::new(entry_point)
            }
            PluginType::Microservice => {
                // For microservices, we might start a connector process
                let mut cmd = TokioCommand::new("crucible-connector");
                cmd.arg("--service");
                cmd.arg(entry_point);
                cmd
            }
        };

        // Set working directory
        if let Some(working_dir) = &self.config.working_directory {
            cmd.current_dir(working_dir);
        } else {
            cmd.current_dir(entry_point.parent().unwrap_or_else(|| std::path::Path::new(".")));
        }

        // Set environment variables
        // Start with plugin environment
        for (key, value) in &self.manifest.environment {
            cmd.env(key, value);
        }

        // Override/add instance environment
        for (key, value) in &self.config.environment {
            cmd.env(key, value);
        }

        // Add standard environment variables
        cmd.env("CRUCIBLE_PLUGIN_ID", &self.manifest.id);
        cmd.env("CRUCIBLE_INSTANCE_ID", &self.info.instance_id);
        cmd.env("CRUCIBLE_PLUGIN_VERSION", &self.manifest.version);

        // Add IPC configuration
        if let Some(port) = self.get_ipc_port() {
            cmd.env("CRUCIBLE_IPC_PORT", port.to_string());
        }

        // Add arguments
        for arg in &self.config.arguments {
            cmd.arg(arg);
        }

        // Configure stdin/stdout/stderr
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Apply sandbox configuration
        self.apply_sandbox_config(&mut cmd)?;

        Ok(cmd)
    }

    /// Apply sandbox configuration to command
    fn apply_sandbox_config(&self, cmd: &mut TokioCommand) -> PluginResult<()> {
        if !self.info.sandbox_config.enabled {
            return Ok(());
        }

        match self.info.sandbox_config.sandbox_type {
            SandboxType::Process => {
                // Basic process isolation
                #[cfg(unix)]
                {
                    // use std::os::unix::process::CommandExt;

                    // Set resource limits
                    // unsafe {
                    //     cmd.pre_exec(|| {
                    //         // This runs in the child process before exec
                    //         // Set up namespaces, resource limits, etc.
                    //         Ok(())
                    //     });
                    // }
                }
            }
            SandboxType::Container => {
                // Use container runtime (e.g., Docker, Podman)
                // let container_cmd = format!(
                //     "docker run --rm -i --network=none --memory={}m --cpus={} {}",
                //     self.info.resource_limits.max_memory_bytes.unwrap_or(512) / 1024 / 1024,
                //     self.info.resource_limits.max_cpu_percentage.unwrap_or(50.0) / 100.0,
                //     self.manifest.entry_point.display()
                // );

                // *cmd = TokioCommand::new("sh");
                // cmd.arg("-c");
                // cmd.arg(container_cmd);
                warn!("Container sandboxing not implemented yet");
            }
            SandboxType::VirtualMachine => {
                // Use VM-based isolation (firecracker, etc.)
                return Err(PluginError::configuration(
                    "VM sandboxing not yet implemented".to_string()
                ));
            }
            SandboxType::Language => {
                // Language-level sandboxing
                // This is handled by the specific runtime (Rune VM, etc.)
            }
            SandboxType::None => {
                // No sandboxing
            }
        }

        Ok(())
    }

    /// Get IPC port for this instance
    fn get_ipc_port(&self) -> Option<u16> {
        // Use a hash of instance ID to generate a consistent port
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.info.instance_id.hash(&mut hasher);
        let hash = hasher.finish();

        // Use port range 9000-9999
        Some(9000 + (hash % 1000) as u16)
    }

    /// Setup IPC communication (placeholder)
    async fn setup_ipc(&mut self) -> PluginResult<()> {
        // let port = self.get_ipc_port()
        //     .ok_or_else(|| PluginError::communication("Failed to determine IPC port".to_string()))?;

        // // Create IPC client
        // self.ipc_client = Some(Arc::new(super::plugin_ipc::create_ipc_client(port).await?));

        // // Create IPC server for receiving messages
        // self.ipc_server = Some(Arc::new(super::plugin_ipc::create_ipc_server(port).await?));

        Ok(())
    }

    /// Start process monitoring
    async fn start_process_monitoring(&self) {
        let instance_id = self.info.instance_id.clone();
        let child_process = self.child_process.clone();
        let event_subscribers = self.event_subscribers.clone();

        tokio::spawn(async move {
            let mut child = child_process.lock().await;
            if let Some(ref mut process) = *child {
                match process.wait().await {
                    Ok(status) => {
                        debug!("Plugin process {} exited with status: {}", instance_id, status);

                        let event = if status.success() {
                            InstanceEvent::ProcessExited {
                                instance_id: instance_id.clone(),
                                exit_code: status.code().unwrap_or(0),
                                success: true,
                            }
                        } else {
                            InstanceEvent::ProcessExited {
                                instance_id: instance_id.clone(),
                                exit_code: status.code().unwrap_or(1),
                                success: false,
                            }
                        };

                        // Notify subscribers
                        let mut subscribers = event_subscribers.read().await;
                        for sender in subscribers.iter() {
                            let _ = sender.send(event.clone());
                        }
                    }
                    Err(e) => {
                        error!("Failed to wait for plugin process {}: {}", instance_id, e);
                    }
                }
            }
        });
    }

    /// Start health monitoring
    async fn start_health_monitoring(&self) {
        let instance_id = self.info.instance_id.clone();
        let health_check_interval = self.config.health_check_interval;
        let ipc_client = self.ipc_client.clone();
        let event_subscribers = self.event_subscribers.clone();

        tokio::spawn(async move {
            let mut interval = interval(health_check_interval);

            loop {
                interval.tick().await;

                if let Some(ref client) = ipc_client {
                    let health_msg = PluginMessage {
                        message_id: uuid::Uuid::new_v4().to_string(),
                        message_type: PluginMessageType::HealthCheck,
                        source_instance_id: None,
                        target_instance_id: Some(instance_id.clone()),
                        payload: serde_json::json!({}),
                        timestamp: SystemTime::now(),
                        correlation_id: None,
                        priority: super::types::MessagePriority::Normal,
                        timeout: Some(Duration::from_secs(5)),
                    };

                    match client.send_message(health_msg).await {
                        Ok(response) => {
                            let is_healthy = response.message_type == PluginMessageType::Response;
                            let status = if is_healthy {
                                PluginHealthStatus::Healthy
                            } else {
                                PluginHealthStatus::Unhealthy
                            };

                            let event = InstanceEvent::HealthStatusChanged {
                                instance_id: instance_id.clone(),
                                old_status: PluginHealthStatus::Unknown,
                                new_status: status,
                            };

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(event.clone());
                            }
                        }
                        Err(e) => {
                            warn!("Health check failed for instance {}: {}", instance_id, e);

                            let event = InstanceEvent::HealthStatusChanged {
                                instance_id: instance_id.clone(),
                                old_status: PluginHealthStatus::Unknown,
                                new_status: PluginHealthStatus::Unhealthy,
                            };

                            let mut subscribers = event_subscribers.read().await;
                            for sender in subscribers.iter() {
                                let _ = sender.send(event.clone());
                            }
                        }
                    }
                }
            }
        });
    }

    /// Start resource monitoring
    async fn start_resource_monitoring(&self) {
        let instance_id = self.info.instance_id.clone();
        let monitoring_interval = self.config.resource_monitoring_interval;
        let child_process = self.child_process.clone();

        tokio::spawn(async move {
            let mut interval = interval(monitoring_interval);

            loop {
                interval.tick().await;

                let mut child = child_process.lock().await;
                if let Some(ref process) = *child {
                    if let Ok(pid) = process.id() {
                        // Get resource usage for the process
                        if let Ok(usage) = get_process_resource_usage(pid) {
                            debug!("Resource usage for {}: CPU: {:.1}%, Memory: {} MB",
                                instance_id,
                                usage.cpu_percentage,
                                usage.memory_bytes / 1024 / 1024
                            );
                        }
                    }
                }
            }
        });
    }
}

#[async_trait]
impl PluginInstance for DefaultPluginInstance {
    fn instance_id(&self) -> &str {
        &self.info.instance_id
    }

    fn plugin_id(&self) -> &str {
        &self.info.plugin_id
    }

    async fn get_state(&self) -> PluginInstanceState {
        self.info.state.clone()
    }

    async fn start(&mut self) -> PluginResult<()> {
        info!("Starting plugin instance: {}", self.info.instance_id);

        if self.info.state != PluginInstanceState::Created {
            return Err(PluginError::lifecycle(format!(
                "Cannot start instance in state: {:?}",
                self.info.state
            )));
        }

        self.update_state(PluginInstanceState::Starting).await;

        // Setup IPC (placeholder - would use actual IPC in implementation)
        // if let Err(e) = self.setup_ipc().await {
        //     self.update_state(PluginInstanceState::Error(e.to_string())).await;
        //     return Err(e);
        // }

        // Build and execute command
        let mut cmd = self.build_command()?;

        debug!("Starting plugin process with command: {:?}", cmd);

        let child = cmd.spawn()
            .map_err(|e| {
                let error_msg = e.to_string();
                // Note: Can't update state here as this is not async
                PluginError::process(format!("Failed to start plugin process: {}", error_msg))
            })?;

        // Store process handle
        {
            let mut process_guard = self.child_process.lock().await;
            *process_guard = Some(child);
        }

        // Set PID
        if let Some(ref process) = *self.child_process.lock().await {
            self.info.pid = Some(process.id());
        }

        // Start monitoring
        self.start_process_monitoring().await;
        self.start_health_monitoring().await;
        self.start_resource_monitoring().await;

        // Wait for startup confirmation
        let startup_result = timeout(
            self.config.startup_timeout,
            self.wait_for_startup_confirmation()
        ).await;

        match startup_result {
            Ok(Ok(())) => {
                self.update_state(PluginInstanceState::Running).await;
                self.publish_event(InstanceEvent::Started {
                    instance_id: self.info.instance_id.clone(),
                    plugin_id: self.info.plugin_id.clone(),
                }).await;
                info!("Successfully started plugin instance: {}", self.info.instance_id);
                Ok(())
            }
            Ok(Err(e)) => {
                self.update_state(PluginInstanceState::Error(e.to_string())).await;
                Err(e)
            }
            Err(_) => {
                let error = "Startup timeout".to_string();
                self.update_state(PluginInstanceState::Error(error.clone())).await;
                Err(PluginError::timeout(error))
            }
        }
    }

    async fn stop(&mut self) -> PluginResult<()> {
        info!("Stopping plugin instance: {}", self.info.instance_id);

        if !self.info.is_running() {
            return Ok(());
        }

        self.update_state(PluginInstanceState::Stopping).await;

        // Send shutdown message (placeholder)
        // if let Some(ref client) = self.ipc_client {
        //     let shutdown_msg = PluginMessage {
        //         message_id: uuid::Uuid::new_v4().to_string(),
        //         message_type: PluginMessageType::Shutdown,
        //         source_instance_id: None,
        //         target_instance_id: Some(self.info.instance_id.clone()),
        //         payload: serde_json::json!({}),
        //         timestamp: SystemTime::now(),
        //         correlation_id: None,
        //         priority: super::types::MessagePriority::High,
        //         timeout: Some(self.config.shutdown_timeout),
        //     };

        //     let _ = client.send_message(shutdown_msg).await;
        // }

        // Wait for graceful shutdown
        let shutdown_result = timeout(
            self.config.shutdown_timeout,
            self.wait_for_process_exit()
        ).await;

        match shutdown_result {
            Ok(Ok(())) => {
                self.update_state(PluginInstanceState::Stopped).await;
                self.publish_event(InstanceEvent::Stopped {
                    instance_id: self.info.instance_id.clone(),
                    plugin_id: self.info.plugin_id.clone(),
                }).await;
                info!("Successfully stopped plugin instance: {}", self.info.instance_id);
                Ok(())
            }
            Ok(Err(e)) => {
                Err(e)
            }
            Err(_) => {
                // Force kill
                self.force_kill().await?;
                self.update_state(PluginInstanceState::Stopped).await;
                info!("Force killed plugin instance: {}", self.info.instance_id);
                Ok(())
            }
        }
    }

    async fn restart(&mut self) -> PluginResult<()> {
        info!("Restarting plugin instance: {}", self.info.instance_id);

        self.info.restart_count += 1;

        if self.info.restart_count > self.config.max_restart_attempts {
            return Err(PluginError::lifecycle(format!(
                "Maximum restart attempts ({}) exceeded for instance {}",
                self.config.max_restart_attempts,
                self.info.instance_id
            )));
        }

        // Stop if running
        if self.info.is_running() {
            self.stop().await?;
        }

        // Wait before restart
        tokio::time::sleep(self.config.restart_delay).await;

        // Start again
        self.start().await
    }

    async fn send_message(&self, _message: PluginMessage) -> PluginResult<PluginMessage> {
        if !self.info.is_running() {
            return Err(PluginError::communication(
                "Cannot send message to non-running instance".to_string()
            ));
        }

        // Placeholder implementation
        Err(PluginError::communication("IPC not implemented".to_string()))
    }

    async fn get_resource_usage(&self) -> PluginResult<ResourceUsage> {
        if let Some(pid) = self.info.pid {
            get_process_resource_usage(pid)
        } else {
            Ok(ResourceUsage::default())
        }
    }

    async fn get_health_status(&self) -> PluginResult<PluginHealthStatus> {
        if !self.info.is_running() {
            return Ok(PluginHealthStatus::Unhealthy);
        }

        // Send health check message
        let health_msg = PluginMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            message_type: PluginMessageType::HealthCheck,
            source_instance_id: None,
            target_instance_id: Some(self.info.instance_id.clone()),
            payload: serde_json::json!({}),
            timestamp: SystemTime::now(),
            correlation_id: None,
            priority: super::types::MessagePriority::Normal,
            timeout: Some(Duration::from_secs(5)),
        };

        match self.send_message(health_msg).await {
            Ok(response) => {
                if response.message_type == PluginMessageType::Response {
                    Ok(PluginHealthStatus::Healthy)
                } else {
                    Ok(PluginHealthStatus::Unhealthy)
                }
            }
            Err(_) => Ok(PluginHealthStatus::Unhealthy),
        }
    }

    async fn update_config(&mut self, config: HashMap<String, serde_json::Value>) -> PluginResult<()> {
        self.info.config = config.clone();

        // Send config update message
        if self.info.is_running() {
            let config_msg = PluginMessage {
                message_id: uuid::Uuid::new_v4().to_string(),
                message_type: PluginMessageType::ConfigUpdate,
                source_instance_id: None,
                target_instance_id: Some(self.info.instance_id.clone()),
                payload: serde_json::to_value(config)?,
                timestamp: SystemTime::now(),
                correlation_id: None,
                priority: super::types::MessagePriority::Normal,
                timeout: Some(Duration::from_secs(10)),
            };

            let _ = self.send_message(config_msg).await;
        }

        Ok(())
    }

    async fn get_execution_stats(&self) -> PluginResult<PluginExecutionStats> {
        Ok(self.info.execution_stats.clone())
    }
}

impl DefaultPluginInstance {
    /// Wait for startup confirmation
    async fn wait_for_startup_confirmation(&self) -> PluginResult<()> {
        // Send ping message and wait for response
        let ping_msg = PluginMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            message_type: PluginMessageType::HealthCheck,
            source_instance_id: None,
            target_instance_id: Some(self.info.instance_id.clone()),
            payload: serde_json::json!({"startup": true}),
            timestamp: SystemTime::now(),
            correlation_id: None,
            priority: super::types::MessagePriority::Normal,
            timeout: Some(Duration::from_secs(5)),
        };

        // Try multiple times with backoff
        for attempt in 1..=5 {
            tokio::time::sleep(Duration::from_millis(500 * attempt)).await;

            if let Some(ref client) = self.ipc_client {
                match client.send_message(ping_msg.clone()).await {
                    Ok(response) => {
                        if response.message_type == PluginMessageType::Response {
                            return Ok(());
                        }
                    }
                    Err(_) => {
                        // Continue trying
                    }
                }
            }
        }

        Err(PluginError::timeout("No startup confirmation received".to_string()))
    }

    /// Wait for process to exit
    async fn wait_for_process_exit(&self) -> PluginResult<()> {
        let mut child = self.child_process.lock().await;
        if let Some(ref mut process) = *child {
            process.wait().await
                .map_err(|e| PluginError::process(format!("Failed to wait for process: {}", e)))?;
        }
        Ok(())
    }

    /// Force kill the process
    async fn force_kill(&self) -> PluginResult<()> {
        let mut child = self.child_process.lock().await;
        if let Some(ref mut process) = *child {
            process.start_kill()
                .map_err(|e| PluginError::process(format!("Failed to kill process: {}", e)))?;
        }
        Ok(())
    }
}

/// ============================================================================
/// PROCESS SUPERVISOR
/// ============================================================================

/// Process supervisor for managing plugin processes
#[derive(Debug)]
pub struct ProcessSupervisor {
    /// Active processes being supervised
    processes: Arc<RwLock<HashMap<String, ProcessInfo>>>,
}

/// Information about a supervised process
#[derive(Debug, Clone)]
struct ProcessInfo {
    /// Process ID
    pid: u32,
    /// Instance ID
    instance_id: String,
    /// Plugin ID
    plugin_id: String,
    /// Start time
    start_time: SystemTime,
    /// Last check time
    last_check: SystemTime,
    /// Health status
    health_status: PluginHealthStatus,
}

impl ProcessSupervisor {
    /// Create a new process supervisor
    pub fn new() -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a process for supervision
    pub async fn register_process(&self, instance_id: String, plugin_id: String, pid: u32) {
        let info = ProcessInfo {
            pid,
            instance_id,
            plugin_id,
            start_time: SystemTime::now(),
            last_check: SystemTime::now(),
            health_status: PluginHealthStatus::Unknown,
        };

        let mut processes = self.processes.write().await;
        processes.insert(info.instance_id.clone(), info);
    }

    /// Unregister a process
    pub async fn unregister_process(&self, instance_id: &str) {
        let mut processes = self.processes.write().await;
        processes.remove(instance_id);
    }

    /// Get process information
    pub async fn get_process_info(&self, instance_id: &str) -> Option<ProcessInfo> {
        let processes = self.processes.read().await;
        processes.get(instance_id).cloned()
    }

    /// List all supervised processes
    pub async fn list_processes(&self) -> Vec<ProcessInfo> {
        let processes = self.processes.read().await;
        processes.values().cloned().collect()
    }

    /// Check if process is alive
    pub async fn is_process_alive(&self, pid: u32) -> bool {
        // Process check implementation
        // #[cfg(unix)]
        // {
        //     use nix::sys::signal::{kill, Signal};
        //     use nix::unistd::Pid;

        //     // Send signal 0 to check if process exists
        //     kill(Pid::from_raw(pid as i32), None).is_ok()
        // }

        // #[cfg(not(unix))]
        // {
        //     // On non-Unix platforms, use a different approach
        //     true // Placeholder
        // }

        true // Placeholder implementation
    }
}

impl Default for ProcessSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// ============================================================================
/// INSTANCE EVENTS
/// ============================================================================

/// Instance event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InstanceEvent {
    /// Instance state changed
    StateChanged {
        instance_id: String,
        old_state: PluginInstanceState,
        new_state: PluginInstanceState,
    },
    /// Instance started
    Started {
        instance_id: String,
        plugin_id: String,
    },
    /// Instance stopped
    Stopped {
        instance_id: String,
        plugin_id: String,
    },
    /// Instance crashed
    Crashed {
        instance_id: String,
        plugin_id: String,
        error: String,
    },
    /// Process exited
    ProcessExited {
        instance_id: String,
        exit_code: i32,
        success: bool,
    },
    /// Health status changed
    HealthStatusChanged {
        instance_id: String,
        old_status: PluginHealthStatus,
        new_status: PluginHealthStatus,
    },
    /// Resource limit exceeded
    ResourceLimitExceeded {
        instance_id: String,
        resource_type: String,
        current_value: f64,
        limit: f64,
    },
    /// Configuration updated
    ConfigurationUpdated {
        instance_id: String,
        config: HashMap<String, serde_json::Value>,
    },
    /// Error occurred
    Error {
        instance_id: String,
        error: String,
        context: Option<String>,
    },
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Get resource usage for a process
async fn get_process_resource_usage(pid: u32) -> PluginResult<ResourceUsage> {
    #[cfg(unix)]
    {
        use std::fs;
        use std::time::SystemTime;

        // Read from /proc/{pid}/stat
        let stat_path = format!("/proc/{}/stat", pid);
        let stat_content = fs::read_to_string(&stat_path)
            .map_err(|e| PluginError::resource(format!("Failed to read process stat: {}", e)))?;

        let parts: Vec<&str> = stat_content.split_whitespace().collect();
        if parts.len() < 24 {
            return Err(PluginError::resource("Invalid process stat format".to_string()));
        }

        // Parse relevant fields from /proc/[pid]/stat
        let utime = parts[13].parse::<u64>()
            .map_err(|e| PluginError::resource(format!("Failed to parse utime: {}", e)))?;
        let stime = parts[14].parse::<u64>()
            .map_err(|e| PluginError::resource(format!("Failed to parse stime: {}", e)))?;
        let vsize = parts[22].parse::<u64>()
            .map_err(|e| PluginError::resource(format!("Failed to parse vsize: {}", e)))?;
        let rss = parts[23].parse::<u64>()
            .map_err(|e| PluginError::resource(format!("Failed to parse rss: {}", e)))?;

        // Convert to bytes (Linux-specific)
        let memory_bytes = vsize;
        let cpu_time = (utime + stime) as f64 / 100.0; // Convert jiffies to seconds

        // Get system uptime for CPU percentage calculation
        let uptime_path = "/proc/uptime";
        let uptime_content = fs::read_to_string(uptime_path)
            .map_err(|e| PluginError::resource(format!("Failed to read system uptime: {}", e)))?;

        let system_uptime = uptime_content.split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        let cpu_percentage = if system_uptime > 0.0 {
            (cpu_time / system_uptime) * 100.0
        } else {
            0.0
        };

        Ok(ResourceUsage {
            memory_bytes,
            cpu_percentage,
            disk_bytes: 0,
            network_bytes: 0,
            open_files: 0,
            active_threads: 1,
            child_processes: 0,
            measured_at: SystemTime::now(),
        })
    }

    #[cfg(not(unix))]
    {
        // Placeholder for non-Unix platforms
        Ok(ResourceUsage::default())
    }
}

/// Create a plugin instance
pub fn create_plugin_instance(
    manifest: PluginManifest,
    config: PluginInstanceConfig,
) -> Box<dyn PluginInstance> {
    let info = PluginInstance::new(manifest.id.clone(), HashMap::new());
    Box::new(DefaultPluginInstance::new(info, manifest, config))
}