//! # Mock Implementations
//!
//! Mock implementations of PluginManager components for isolated testing.

use super::*;
use crate::plugin_manager::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::{mpsc, RwLock};

/// ============================================================================
/// MOCK PLUGIN REGISTRY
/// ============================================================================

/// Mock plugin registry for testing
#[derive(Debug)]
pub struct MockPluginRegistry {
    plugins: Arc<RwLock<HashMap<String, PluginRegistryEntry>>>,
    events: Option<mpsc::UnboundedSender<RegistryEvent>>,
    discovery_count: Arc<AtomicU64>,
    should_fail_discovery: Arc<AtomicBool>,
    should_fail_registration: Arc<AtomicBool>,
}

impl MockPluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            events: None,
            discovery_count: Arc::new(AtomicU64::new(0)),
            should_fail_discovery: Arc::new(AtomicBool::new(false)),
            should_fail_registration: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_plugins(plugins: HashMap<String, PluginRegistryEntry>) -> Self {
        let registry = Self::new();
        let mut plugins_guard = block_on(registry.plugins.write());
        *plugins_guard = plugins;
        registry
    }

    pub fn set_discovery_failure(&self, should_fail: bool) {
        self.should_fail_discovery.store(should_fail, Ordering::SeqCst);
    }

    pub fn set_registration_failure(&self, should_fail: bool) {
        self.should_fail_registration.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_discovery_count(&self) -> u64 {
        self.discovery_count.load(Ordering::SeqCst)
    }

    pub async fn add_plugin(&self, entry: PluginRegistryEntry) {
        let mut plugins = self.plugins.write().await;
        plugins.insert(entry.manifest.id.clone(), entry);
    }
}

#[async_trait]
impl PluginRegistry for MockPluginRegistry {
    async fn discover_plugins(&self) -> PluginResult<Vec<PluginManifest>> {
        self.discovery_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_discovery.load(Ordering::SeqCst) {
            return Err(PluginError::discovery("Mock discovery failure"));
        }

        let plugins = self.plugins.read().await;
        Ok(plugins.values().map(|entry| entry.manifest.clone()).collect())
    }

    async fn register_plugin(&self, manifest: PluginManifest) -> PluginResult<String> {
        if self.should_fail_registration.load(Ordering::SeqCst) {
            return Err(PluginError::registry("Mock registration failure"));
        }

        let plugin_id = manifest.id.clone();
        let entry = PluginRegistryEntry {
            manifest: manifest.clone(),
            install_path: std::path::PathBuf::from("/tmp/mock-plugins").join(&plugin_id),
            installed_at: SystemTime::now(),
            status: PluginRegistryStatus::Installed,
            validation_results: None,
            instance_ids: vec![],
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.clone(), entry);

        // Send event if channel exists
        if let Some(events) = &self.events {
            let _ = events.send(RegistryEvent::PluginRegistered { plugin_id: plugin_id.clone() });
        }

        Ok(plugin_id)
    }

    async fn unregister_plugin(&self, plugin_id: &str) -> PluginResult<()> {
        let mut plugins = self.plugins.write().await;
        plugins.remove(plugin_id);

        // Send event if channel exists
        if let Some(events) = &self.events {
            let _ = events.send(RegistryEvent::PluginUnregistered { plugin_id: plugin_id.to_string() });
        }

        Ok(())
    }

    async fn get_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginManifest>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.get(plugin_id).map(|entry| entry.manifest.clone()))
    }

    async fn list_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        let plugins = self.plugins.read().await;
        Ok(plugins.values().cloned().collect())
    }

    async fn list_enabled_plugins(&self) -> PluginResult<Vec<PluginRegistryEntry>> {
        let plugins = self.plugins.read().await;
        Ok(plugins
            .values()
            .filter(|entry| matches!(entry.status, PluginRegistryStatus::Installed))
            .cloned()
            .collect())
    }

    async fn update_plugin_status(&self, plugin_id: &str, status: PluginRegistryStatus) -> PluginResult<()> {
        let mut plugins = self.plugins.write().await;
        if let Some(entry) = plugins.get_mut(plugin_id) {
            entry.status = status;
        }
        Ok(())
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<RegistryEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.events = Some(tx);
        rx
    }
}

/// ============================================================================
/// MOCK PLUGIN INSTANCE
/// ============================================================================

/// Mock plugin instance for testing
#[derive(Debug)]
pub struct MockPluginInstance {
    instance_id: String,
    plugin_id: String,
    state: Arc<RwLock<PluginInstanceState>>,
    pid: Arc<RwLock<Option<u32>>>,
    start_count: Arc<AtomicU64>,
    stop_count: Arc<AtomicU64>,
    should_fail_start: Arc<AtomicBool>,
    should_fail_stop: Arc<AtomicBool>,
    resource_usage: Arc<RwLock<ResourceUsage>>,
    events: Option<mpsc::UnboundedSender<InstanceEvent>>,
}

impl MockPluginInstance {
    pub fn new(instance_id: String, plugin_id: String) -> Self {
        Self {
            instance_id,
            plugin_id,
            state: Arc::new(RwLock::new(PluginInstanceState::Created)),
            pid: Arc::new(RwLock::new(None)),
            start_count: Arc::new(AtomicU64::new(0)),
            stop_count: Arc::new(AtomicU64::new(0)),
            should_fail_start: Arc::new(AtomicBool::new(false)),
            should_fail_stop: Arc::new(AtomicBool::new(false)),
            resource_usage: Arc::new(RwLock::new(ResourceUsage::default())),
            events: None,
        }
    }

    pub fn set_start_failure(&self, should_fail: bool) {
        self.should_fail_start.store(should_fail, Ordering::SeqCst);
    }

    pub fn set_stop_failure(&self, should_fail: bool) {
        self.should_fail_stop.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_start_count(&self) -> u64 {
        self.start_count.load(Ordering::SeqCst)
    }

    pub fn get_stop_count(&self) -> u64 {
        self.stop_count.load(Ordering::SeqCst)
    }

    pub async fn set_resource_usage(&self, usage: ResourceUsage) {
        let mut resource_usage = self.resource_usage.write().await;
        *resource_usage = usage;
    }

    async fn send_event(&self, event: InstanceEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

#[async_trait]
impl PluginInstance for MockPluginInstance {
    fn instance_id(&self) -> &str {
        &self.instance_id
    }

    fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    async fn start(&mut self) -> PluginResult<()> {
        self.start_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_start.load(Ordering::SeqCst) {
            let mut state = self.state.write().await;
            *state = PluginInstanceState::Error("Mock start failure".to_string());
            return Err(PluginError::process("Mock start failure"));
        }

        let mut state = self.state.write().await;
        *state = PluginInstanceState::Running;
        drop(state);

        let mut pid = self.pid.write().await;
        *pid = Some(12345); // Mock PID
        drop(pid);

        self.send_event(InstanceEvent::InstanceStarted {
            instance_id: self.instance_id.clone(),
            plugin_id: self.plugin_id.clone(),
        }).await;

        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        self.stop_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_stop.load(Ordering::SeqCst) {
            return Err(PluginError::process("Mock stop failure"));
        }

        let mut state = self.state.write().await;
        *state = PluginInstanceState::Stopped;
        drop(state);

        let mut pid = self.pid.write().await;
        *pid = None;
        drop(pid);

        self.send_event(InstanceEvent::InstanceStopped {
            instance_id: self.instance_id.clone(),
            plugin_id: self.plugin_id.clone(),
        }).await;

        Ok(())
    }

    async fn restart(&mut self) -> PluginResult<()> {
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    async fn get_state(&self) -> PluginResult<PluginInstanceState> {
        let state = self.state.read().await;
        Ok(state.clone())
    }

    async fn get_pid(&self) -> PluginResult<Option<u32>> {
        let pid = self.pid.read().await;
        Ok(*pid)
    }

    async fn get_resource_usage(&self) -> PluginResult<ResourceUsage> {
        let resource_usage = self.resource_usage.read().await;
        Ok(resource_usage.clone())
    }

    async fn send_message(&mut self, message: PluginMessage) -> PluginResult<PluginMessage> {
        // Mock message handling
        let response = PluginMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            message_type: PluginMessageType::Response,
            source_instance_id: Some(self.instance_id.clone()),
            target_instance_id: message.source_instance_id,
            payload: serde_json::json!({"status": "ok", "mock": true}),
            timestamp: SystemTime::now(),
            correlation_id: Some(message.message_id),
            priority: message.priority,
            timeout: message.timeout,
        };
        Ok(response)
    }

    async fn subscribe_events(&mut self) -> mpsc::UnboundedReceiver<InstanceEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.events = Some(tx);
        rx
    }
}

/// ============================================================================
/// MOCK RESOURCE MANAGER
/// ============================================================================

/// Mock resource manager for testing
#[derive(Debug)]
pub struct MockResourceManager {
    instances: Arc<RwLock<HashMap<String, ResourceLimits>>>,
    global_usage: Arc<RwLock<ResourceUsage>>,
    instance_usage: Arc<RwLock<HashMap<String, ResourceUsage>>>,
    monitoring_enabled: Arc<AtomicBool>,
    violation_count: Arc<AtomicU64>,
    should_fail_registration: Arc<AtomicBool>,
    events: Option<mpsc::UnboundedSender<ResourceEvent>>,
}

impl MockResourceManager {
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            global_usage: Arc::new(RwLock::new(ResourceUsage::default())),
            instance_usage: Arc::new(RwLock::new(HashMap::new())),
            monitoring_enabled: Arc::new(AtomicBool::new(false)),
            violation_count: Arc::new(AtomicU64::new(0)),
            should_fail_registration: Arc::new(AtomicBool::new(false)),
            events: None,
        }
    }

    pub fn set_registration_failure(&self, should_fail: bool) {
        self.should_fail_registration.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_violation_count(&self) -> u64 {
        self.violation_count.load(Ordering::SeqCst)
    }

    pub async fn set_instance_usage(&self, instance_id: &str, usage: ResourceUsage) {
        let mut instance_usage = self.instance_usage.write().await;
        instance_usage.insert(instance_id.to_string(), usage);
    }

    pub async fn simulate_violation(&self, instance_id: &str, resource_type: &str) {
        self.violation_count.fetch_add(1, Ordering::SeqCst);

        if let Some(events) = &self.events {
            let _ = events.send(ResourceEvent::ResourceViolation {
                instance_id: instance_id.to_string(),
                resource_type: resource_type.to_string(),
                current_value: 150.0,
                limit: 100.0,
            });
        }
    }

    async fn send_event(&self, event: ResourceEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

#[async_trait]
impl ResourceManager for MockResourceManager {
    async fn start(&mut self) -> PluginResult<()> {
        self.monitoring_enabled.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        self.monitoring_enabled.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn register_instance(&mut self, instance_id: String, limits: ResourceLimits) -> PluginResult<()> {
        if self.should_fail_registration.load(Ordering::SeqCst) {
            return Err(PluginError::resource("Mock registration failure"));
        }

        let mut instances = self.instances.write().await;
        instances.insert(instance_id, limits);
        Ok(())
    }

    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        let mut instances = self.instances.write().await;
        instances.remove(instance_id);

        let mut instance_usage = self.instance_usage.write().await;
        instance_usage.remove(instance_id);

        Ok(())
    }

    async fn get_instance_usage(&self, instance_id: &str) -> PluginResult<ResourceUsage> {
        let instance_usage = self.instance_usage.read().await;
        Ok(instance_usage
            .get(instance_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_global_usage(&self) -> PluginResult<ResourceUsage> {
        let global_usage = self.global_usage.read().await;
        Ok(global_usage.clone())
    }

    async fn get_metrics(&self) -> PluginResult<ResourceMetrics> {
        let instance_usage = self.instance_usage.read().await;
        let total_usage = instance_usage.values().fold(
            ResourceUsage::default(),
            |mut acc, usage| {
                acc.memory_bytes += usage.memory_bytes;
                acc.cpu_percentage += usage.cpu_percentage;
                acc.disk_bytes += usage.disk_bytes;
                acc.network_bytes += usage.network_bytes;
                acc
            },
        );

        Ok(ResourceMetrics {
            total_usage,
            per_instance_usage: instance_usage.clone(),
            violations_count: self.violation_count.load(Ordering::SeqCst),
            last_updated: SystemTime::now(),
        })
    }

    async fn enforce_limits(&self, instance_id: &str) -> PluginResult<bool> {
        let instance_usage = self.instance_usage.read().await;
        let instances = self.instances.read().await;

        if let (Some(usage), Some(limits)) = (
            instance_usage.get(instance_id),
            instances.get(instance_id),
        ) {
            let mut has_violation = false;

            if let Some(max_memory) = limits.max_memory_bytes {
                if usage.memory_bytes > max_memory {
                    self.simulate_violation(instance_id, "memory").await;
                    has_violation = true;
                }
            }

            if let Some(max_cpu) = limits.max_cpu_percentage {
                if usage.cpu_percentage > max_cpu {
                    self.simulate_violation(instance_id, "cpu").await;
                    has_violation = true;
                }
            }

            Ok(has_violation)
        } else {
            Ok(false)
        }
    }

    async fn liveness_check(&self) -> PluginResult<bool> {
        Ok(self.monitoring_enabled.load(Ordering::SeqCst))
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<ResourceEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.events = Some(tx);
        rx
    }
}

/// ============================================================================
/// MOCK SECURITY MANAGER
/// ============================================================================

/// Mock security manager for testing
#[derive(Debug)]
pub struct MockSecurityManager {
    enabled: Arc<AtomicBool>,
    validation_count: Arc<AtomicU64>,
    violation_count: Arc<AtomicU64>,
    should_fail_validation: Arc<AtomicBool>,
    security_level: Arc<RwLock<SecurityLevel>>,
    events: Option<mpsc::UnboundedSender<SecurityEvent>>,
}

impl MockSecurityManager {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            validation_count: Arc::new(AtomicU64::new(0)),
            violation_count: Arc::new(AtomicU64::new(0)),
            should_fail_validation: Arc::new(AtomicBool::new(false)),
            security_level: Arc::new(RwLock::new(SecurityLevel::Basic)),
            events: None,
        }
    }

    pub fn set_validation_failure(&self, should_fail: bool) {
        self.should_fail_validation.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_validation_count(&self) -> u64 {
        self.validation_count.load(Ordering::SeqCst)
    }

    pub fn get_violation_count(&self) -> u64 {
        self.violation_count.load(Ordering::SeqCst)
    }

    pub async fn set_security_level(&self, level: SecurityLevel) {
        let mut security_level = self.security_level.write().await;
        *security_level = level;
    }

    pub async fn simulate_violation(&self, plugin_id: &str, violation: &str) {
        self.violation_count.fetch_add(1, Ordering::SeqCst);

        if let Some(events) = &self.events {
            let _ = events.send(SecurityEvent::SecurityViolation {
                plugin_id: plugin_id.to_string(),
                violation: violation.to_string(),
                severity: SecuritySeverity::High,
            });
        }
    }

    async fn send_event(&self, event: SecurityEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

#[async_trait]
impl SecurityManager for MockSecurityManager {
    async fn start(&mut self) -> PluginResult<()> {
        self.enabled.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        self.enabled.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn create_sandbox(&self, plugin_id: &str, config: &SandboxConfig) -> PluginResult<String> {
        Ok(format!("sandbox_{}", plugin_id))
    }

    async fn destroy_sandbox(&self, sandbox_id: &str) -> PluginResult<()> {
        // Mock sandbox destruction
        Ok(())
    }

    async fn validate_plugin_security(&self, manifest: &PluginManifest) -> PluginResult<SecurityValidationResult> {
        self.validation_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_validation.load(Ordering::SeqCst) {
            return Err(PluginError::security("Mock validation failure"));
        }

        Ok(SecurityValidationResult {
            passed: true,
            issues: vec![],
            security_level: SecurityLevel::Basic,
            recommendations: vec![],
        })
    }

    async fn check_permission(&self, plugin_id: &str, permission: &PluginPermission) -> PluginResult<bool> {
        let security_level = self.security_level.read().await;
        match *security_level {
            SecurityLevel::None => Ok(true),
            SecurityLevel::Basic => {
                matches!(permission, PluginPermission::IpcCommunication | PluginPermission::FileSystemRead)
            }
            SecurityLevel::Strict => Ok(false),
            SecurityLevel::Maximum => Ok(false),
        }
    }

    async fn enforce_security_policy(&self, plugin_id: &str, operation: &str) -> PluginResult<bool> {
        let security_level = self.security_level.read().await;
        match *security_level {
            SecurityLevel::None | SecurityLevel::Basic => Ok(true),
            SecurityLevel::Strict => {
                // Allow only safe operations
                matches!(operation, "read" | "write" | "ipc")
            }
            SecurityLevel::Maximum => {
                // Very restrictive
                matches!(operation, "read")
            }
        }
    }

    async fn get_security_metrics(&self) -> PluginResult<SecurityMetrics> {
        Ok(SecurityMetrics {
            violations_count: self.violation_count.load(Ordering::SeqCst),
            blocked_operations_count: 0,
            security_level: *self.security_level.try_read().unwrap_or(&SecurityLevel::Basic),
            last_violation: None,
            active_sandboxes: 0,
        })
    }

    async fn liveness_check(&self) -> PluginResult<bool> {
        Ok(self.enabled.load(Ordering::SeqCst))
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<SecurityEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.events = Some(tx);
        rx
    }
}

/// ============================================================================
/// MOCK HEALTH MONITOR
/// ============================================================================

/// Mock health monitor for testing
#[derive(Debug)]
pub struct MockHealthMonitor {
    enabled: Arc<AtomicBool>,
    instances: Arc<RwLock<HashMap<String, PluginHealthStatus>>>,
    check_count: Arc<AtomicU64>,
    recovery_count: Arc<AtomicU64>,
    should_fail_health_check: Arc<AtomicBool>,
    events: Option<mpsc::UnboundedSender<HealthEvent>>,
}

impl MockHealthMonitor {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            instances: Arc::new(RwLock::new(HashMap::new())),
            check_count: Arc::new(AtomicU64::new(0)),
            recovery_count: Arc::new(AtomicU64::new(0)),
            should_fail_health_check: Arc::new(AtomicBool::new(false)),
            events: None,
        }
    }

    pub fn set_health_check_failure(&self, should_fail: bool) {
        self.should_fail_health_check.store(should_fail, Ordering::SeqCst);
    }

    pub fn get_check_count(&self) -> u64 {
        self.check_count.load(Ordering::SeqCst)
    }

    pub fn get_recovery_count(&self) -> u64 {
        self.recovery_count.load(Ordering::SeqCst)
    }

    pub async fn set_instance_health(&self, instance_id: &str, status: PluginHealthStatus) {
        let mut instances = self.instances.write().await;
        instances.insert(instance_id.to_string(), status);
    }

    pub async fn simulate_health_change(&self, instance_id: &str, plugin_id: &str, status: PluginHealthStatus) {
        self.set_instance_health(instance_id, status).await;

        if let Some(events) = &self.events {
            let _ = events.send(HealthEvent::HealthStatusChanged {
                instance_id: instance_id.to_string(),
                plugin_id: plugin_id.to_string(),
                old_status: PluginHealthStatus::Unknown,
                new_status: status,
            });
        }
    }

    async fn send_event(&self, event: HealthEvent) {
        if let Some(events) = &self.events {
            let _ = events.send(event);
        }
    }
}

#[async_trait]
impl HealthMonitor for MockHealthMonitor {
    async fn start(&mut self) -> PluginResult<()> {
        self.enabled.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        self.enabled.store(false, Ordering::SeqCst);
        Ok(())
    }

    async fn register_instance(&mut self, instance_id: String, plugin_id: String, config: HealthCheckConfig) -> PluginResult<()> {
        let mut instances = self.instances.write().await;
        instances.insert(instance_id, PluginHealthStatus::Healthy);
        Ok(())
    }

    async fn unregister_instance(&mut self, instance_id: &str) -> PluginResult<()> {
        let mut instances = self.instances.write().await;
        instances.remove(instance_id);
        Ok(())
    }

    async fn perform_health_check(&self, instance_id: &str) -> PluginResult<HealthCheckResult> {
        self.check_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail_health_check.load(Ordering::SeqCst) {
            return Err(PluginError::health_monitoring("Mock health check failure"));
        }

        let instances = self.instances.read().await;
        let status = instances.get(instance_id).copied().unwrap_or(PluginHealthStatus::Unknown);

        Ok(HealthCheckResult {
            instance_id: instance_id.to_string(),
            status,
            timestamp: SystemTime::now(),
            details: HashMap::from([
                ("check_type".to_string(), "mock".to_string()),
                ("duration_ms".to_string(), "10".to_string()),
            ]),
        })
    }

    async fn get_instance_health(&self, instance_id: &str) -> PluginResult<PluginHealthStatus> {
        let instances = self.instances.read().await;
        Ok(instances.get(instance_id).copied().unwrap_or(PluginHealthStatus::Unknown))
    }

    async fn get_system_health(&self) -> PluginResult<SystemHealthSummary> {
        let instances = self.instances.read().await;
        let total_instances = instances.len();
        let healthy_instances = instances.values().filter(|&&status| status == PluginHealthStatus::Healthy).count();

        Ok(SystemHealthSummary {
            overall_status: if healthy_instances == total_instances {
                ServiceStatus::Healthy
            } else if healthy_instances > 0 {
                ServiceStatus::Degraded
            } else {
                ServiceStatus::Unhealthy
            },
            total_instances,
            healthy_instances,
            unhealthy_instances: total_instances - healthy_instances,
            last_check: SystemTime::now(),
            issues: vec![],
        })
    }

    async fn attempt_recovery(&self, instance_id: &str) -> PluginResult<bool> {
        self.recovery_count.fetch_add(1, Ordering::SeqCst);

        // Mock recovery - set to healthy
        self.set_instance_health(instance_id, PluginHealthStatus::Healthy).await;
        Ok(true)
    }

    async fn liveness_check(&self) -> PluginResult<bool> {
        Ok(self.enabled.load(Ordering::SeqCst))
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<HealthEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.events = Some(tx);
        rx
    }
}

// Helper function to block on async in tests
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use tokio::runtime::Runtime;
    let rt = Runtime::new().unwrap();
    rt.block_on(future)
}