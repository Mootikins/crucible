//! # Security Manager
//!
//! This module implements the SecurityManager which handles plugin sandboxing,
//! capability enforcement, security policy management, and audit logging.

use super::config::{SecurityConfig, SecurityPolicyConfig, SecurityRule, AuditConfig, SandboxConfig};
use super::error::{PluginError, PluginResult, ErrorContext};
use super::types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// ============================================================================
/// SECURITY MANAGER TRAIT
/// ============================================================================

#[async_trait]
pub trait SecurityManager: Send + Sync {
    /// Start security manager
    async fn start(&mut self) -> PluginResult<()>;

    /// Stop security manager
    async fn stop(&mut self) -> PluginResult<()>;

    /// Validate plugin security
    async fn validate_plugin_security(&self, manifest: &PluginManifest) -> PluginResult<SecurityValidationResult>;

    /// Create sandbox environment for plugin
    async fn create_sandbox(&self, plugin_id: &str, config: &SandboxConfig) -> PluginResult<SandboxEnvironment>;

    /// Destroy sandbox environment
    async fn destroy_sandbox(&self, sandbox_id: &str) -> PluginResult<()>;

    /// Check if capability is allowed
    async fn check_capability(&self, plugin_id: &str, capability: &PluginCapability, context: &SecurityContext) -> PluginResult<bool>;

    /// Apply security policy to plugin
    async fn apply_security_policy(&self, plugin_id: &str, policy: &SecurityPolicy) -> PluginResult<()>;

    /// Audit security event
    async fn audit_event(&self, event: SecurityAuditEvent) -> PluginResult<()>;

    /// Get security metrics
    async fn get_security_metrics(&self) -> PluginResult<SecurityMetrics>;

    /// Subscribe to security events
    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<SecurityEvent>;

    /// Update security policies
    async fn update_policies(&mut self, policies: Vec<SecurityRule>) -> PluginResult<()>;

    /// Generate security report
    async fn generate_security_report(&self, time_range: Option<(SystemTime, SystemTime)>) -> PluginResult<SecurityReport>;
}

/// ============================================================================
/// SECURITY TYPES
/// ============================================================================

/// Sandbox environment
#[derive(Debug, Clone)]
pub struct SandboxEnvironment {
    /// Unique sandbox identifier
    pub sandbox_id: String,
    /// Plugin ID
    pub plugin_id: String,
    /// Sandbox configuration
    pub config: SandboxConfig,
    /// Namespace information
    pub namespaces: SandboxNamespaces,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Created timestamp
    pub created_at: SystemTime,
    /// Process ID (if applicable)
    pub pid: Option<u32>,
    /// Network namespace
    pub network_namespace: Option<String>,
    /// Mount namespace
    pub mount_namespace: Option<String>,
}

/// Sandbox namespace information
#[derive(Debug, Clone)]
pub struct SandboxNamespaces {
    /// PID namespace
    pub pid: Option<String>,
    /// Network namespace
    pub network: Option<String>,
    /// Mount namespace
    pub mount: Option<String>,
    /// IPC namespace
    pub ipc: Option<String>,
    /// User namespace
    pub user: Option<String>,
    /// UTS namespace
    pub uts: Option<String>,
}

/// Security audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditEvent {
    /// Event ID
    pub event_id: String,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Plugin ID
    pub plugin_id: String,
    /// Instance ID (if applicable)
    pub instance_id: Option<String>,
    /// Event type
    pub event_type: SecurityAuditEventType,
    /// Event severity
    pub severity: SecuritySeverity,
    /// Event description
    pub description: String,
    /// Source location
    pub source_location: Option<String>,
    /// Additional context
    pub context: HashMap<String, String>,
    /// Action taken
    pub action_taken: Option<SecurityAction>,
}

/// Security audit event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityAuditEventType {
    /// Plugin validation completed
    PluginValidation,
    /// Sandbox created
    SandboxCreated,
    /// Sandbox destroyed
    SandboxDestroyed,
    /// Capability check
    CapabilityCheck,
    /// Security violation
    SecurityViolation,
    /// Policy applied
    PolicyApplied,
    /// Access denied
    AccessDenied,
    /// Resource limit exceeded
    ResourceLimitExceeded,
    /// Anomalous behavior detected
    AnomalousBehavior,
    /// Security configuration changed
    ConfigurationChanged,
    /// Authentication event
    Authentication,
    /// Authorization event
    Authorization,
}

/// Security action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityAction {
    /// Allow operation
    Allow,
    /// Deny operation
    Deny,
    /// Log warning
    Warn,
    /// Terminate process
    Terminate,
    /// Quarantine plugin
    Quarantine,
    /// Suspend plugin
    Suspend,
    /// Require authentication
    RequireAuth,
    /// Escalate to administrator
    Escalate,
}

/// Security metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityMetrics {
    /// Total security events
    pub total_events: u64,
    /// Events by type
    pub events_by_type: HashMap<SecurityAuditEventType, u64>,
    /// Events by severity
    pub events_by_severity: HashMap<SecuritySeverity, u64>,
    /// Security violations
    pub violations: u64,
    /// Blocks/denials
    pub blocks: u64,
    /// Sandboxes created
    pub sandboxes_created: u64,
    /// Sandboxes destroyed
    pub sandboxes_destroyed: u64,
    /// Active sandboxes
    pub active_sandboxes: u64,
    /// Most recent events
    pub recent_events: Vec<SecurityAuditEvent>,
    /// Metrics collection timestamp
    pub timestamp: SystemTime,
}

/// Security report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    /// Report ID
    pub report_id: String,
    /// Report time range
    pub time_range: (SystemTime, SystemTime),
    /// Summary statistics
    pub summary: SecurityReportSummary,
    /// Top security issues
    pub top_issues: Vec<SecurityIssueSummary>,
    /// Plugin security ratings
    pub plugin_ratings: HashMap<String, SecurityRating>,
    /// Recommendations
    pub recommendations: Vec<SecurityRecommendation>,
    /// Generated timestamp
    pub generated_at: SystemTime,
}

/// Security report summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReportSummary {
    /// Total events
    pub total_events: u64,
    /// Critical events
    pub critical_events: u64,
    /// High severity events
    pub high_events: u64,
    /// Medium severity events
    pub medium_events: u64,
    /// Low severity events
    pub low_events: u64,
    /// Security violations
    pub violations: u64,
    /// Average risk score
    pub average_risk_score: f64,
}

/// Security issue summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssueSummary {
    /// Issue type
    pub issue_type: SecurityIssueType,
    /// Count
    pub count: u64,
    /// Affected plugins
    pub affected_plugins: Vec<String>,
    /// Average severity
    pub average_severity: f64,
}

/// Security rating
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRating {
    /// Plugin ID
    pub plugin_id: String,
    /// Overall score (0-100)
    pub score: u32,
    /// Security level
    pub security_level: SecurityLevel,
    /// Issues found
    pub issues: Vec<SecurityIssue>,
    /// Last assessed
    pub last_assessed: SystemTime,
}

/// Security recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRecommendation {
    /// Recommendation ID
    pub recommendation_id: String,
    /// Recommendation type
    pub recommendation_type: RecommendationType,
    /// Priority
    pub priority: RecommendationPriority,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Affected plugins
    pub affected_plugins: Vec<String>,
    /// Suggested actions
    pub suggested_actions: Vec<String>,
}

/// Recommendation type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendationType {
    /// Security improvement
    SecurityImprovement,
    /// Configuration change
    ConfigurationChange,
    /// Plugin update
    PluginUpdate,
    /// Policy update
    PolicyUpdate,
    /// Monitoring enhancement
    MonitoringEnhancement,
}

/// Recommendation priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecommendationPriority {
    /// Low priority
    Low,
    /// Medium priority
    Medium,
    /// High priority
    High,
    /// Critical priority
    Critical,
}

/// Security event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityEvent {
    /// Security violation detected
    ViolationDetected {
        plugin_id: String,
        violation: SecurityViolation,
    },
    /// Sandbox event
    SandboxEvent {
        sandbox_id: String,
        event_type: SandboxEventType,
    },
    /// Policy evaluation
    PolicyEvaluated {
        plugin_id: String,
        capability: PluginCapability,
        allowed: bool,
        reason: Option<String>,
    },
    /// Audit log event
    AuditEvent {
        event: SecurityAuditEvent,
    },
    /// Security alert
    SecurityAlert {
        alert_type: SecurityAlertType,
        message: String,
        plugin_id: Option<String>,
    },
}

/// Security violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityViolation {
    /// Violation type
    pub violation_type: SecurityIssueType,
    /// Severity
    pub severity: SecuritySeverity,
    /// Description
    pub description: String,
    /// Plugin capability being violated
    pub capability: Option<PluginCapability>,
    /// Resource being accessed
    pub resource: Option<String>,
    /// Action attempted
    pub action: Option<String>,
    /// Timestamp
    pub timestamp: SystemTime,
}

/// Sandbox event type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxEventType {
    /// Sandbox created
    Created,
    /// Sandbox started
    Started,
    /// Sandbox stopped
    Stopped,
    /// Sandbox destroyed
    Destroyed,
    /// Process spawned in sandbox
    ProcessSpawned,
    /// Process exited in sandbox
    ProcessExited,
    /// Resource access in sandbox
    ResourceAccess,
    /// Network activity in sandbox
    NetworkActivity,
}

/// Security alert type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityAlertType {
    /// Suspicious activity
    SuspiciousActivity,
    /// Security breach
    SecurityBreach,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Unauthorized access
    UnauthorizedAccess,
    /// Anomalous behavior
    AnomalousBehavior,
    /// Policy violation
    PolicyViolation,
}

/// ============================================================================
/// DEFAULT SECURITY MANAGER
/// ============================================================================

/// Default implementation of SecurityManager
#[derive(Debug)]
pub struct DefaultSecurityManager {
    /// Configuration
    config: Arc<SecurityConfig>,
    /// Active sandboxes
    sandboxes: Arc<RwLock<HashMap<String, SandboxEnvironment>>>,
    /// Security policies
    policies: Arc<RwLock<Vec<SecurityRule>>>,
    /// Event subscribers
    event_subscribers: Arc<RwLock<Vec<mpsc::UnboundedSender<SecurityEvent>>>>,
    /// Metrics
    metrics: Arc<RwLock<SecurityMetrics>>,
    /// Audit log writer
    audit_writer: Arc<RwLock<Option<BufWriter<File>>>>,
    /// Security state
    state: Arc<RwLock<SecurityManagerState>>,
    /// Running state
    running: Arc<RwLock<bool>>,
}

/// Security manager state
#[derive(Debug, Clone)]
struct SecurityManagerState {
    /// Started timestamp
    started_at: Option<SystemTime>,
    /// Total validations performed
    total_validations: u64,
    /// Total sandboxes created
    total_sandboxes_created: u64,
    /// Total violations detected
    total_violations: u64,
    /// Last security scan
    last_security_scan: Option<SystemTime>,
}

impl DefaultSecurityManager {
    /// Create a new security manager
    pub fn new(config: SecurityConfig) -> Self {
        Self {
            config: Arc::new(config),
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            policies: Arc::new(RwLock::new(Vec::new())),
            event_subscribers: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(RwLock::new(SecurityMetrics {
                total_events: 0,
                events_by_type: HashMap::new(),
                events_by_severity: HashMap::new(),
                violations: 0,
                blocks: 0,
                sandboxes_created: 0,
                sandboxes_destroyed: 0,
                active_sandboxes: 0,
                recent_events: Vec::new(),
                timestamp: SystemTime::now(),
            })),
            audit_writer: Arc::new(RwLock::new(None)),
            state: Arc::new(RwLock::new(SecurityManagerState {
                started_at: None,
                total_validations: 0,
                total_sandboxes_created: 0,
                total_violations: 0,
                last_security_scan: None,
            })),
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Publish event to subscribers
    async fn publish_event(&self, event: SecurityEvent) {
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

    /// Initialize audit log
    async fn initialize_audit_log(&self) -> PluginResult<()> {
        if !self.config.audit.enabled {
            return Ok(());
        }

        if let Some(log_path) = &self.config.audit.log_file {
            // Create parent directory if it doesn't exist
            if let Some(parent) = log_path.parent() {
                tokio::fs::create_dir_all(parent).await
                    .map_err(|e| PluginError::security(format!("Failed to create audit log directory: {}", e)))?;
            }

            // Open log file
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .await
                .map_err(|e| PluginError::security(format!("Failed to open audit log file: {}", e)))?;

            let writer = BufWriter::new(file);
            let mut audit_writer = self.audit_writer.write().await;
            *audit_writer = Some(writer);

            info!("Audit log initialized: {:?}", log_path);
        }

        Ok(())
    }

    /// Write audit event to log
    async fn write_audit_event(&self, event: &SecurityAuditEvent) -> PluginResult<()> {
        if !self.config.audit.enabled {
            return Ok(());
        }

        let mut writer_guard = self.audit_writer.write().await;
        if let Some(writer) = writer_guard.as_mut() {
            let event_json = serde_json::to_string(event)
                .map_err(|e| PluginError::security(format!("Failed to serialize audit event: {}", e)))?;

            let log_line = format!("{}\n", event_json);
            writer.write_all(log_line.as_bytes()).await
                .map_err(|e| PluginError::security(format!("Failed to write audit event: {}", e)))?;

            writer.flush().await
                .map_err(|e| PluginError::security(format!("Failed to flush audit log: {}", e)))?;
        }

        Ok(())
    }

    /// Update security metrics
    async fn update_metrics(&self, event_type: &SecurityAuditEventType, severity: SecuritySeverity) {
        let mut metrics = self.metrics.write().await;

        metrics.total_events += 1;
        *metrics.events_by_type.entry(event_type.clone()).or_insert(0) += 1;
        *metrics.events_by_severity.entry(severity).or_insert(0) += 1;
        metrics.timestamp = SystemTime::now();

        // Update recent events (keep last 100)
        // Note: We'd need the actual event to add to recent_events
        // For now, just update the counters
    }

    /// Check sandbox capability
    async fn check_sandbox_capability(&self, capability: &PluginCapability, config: &SandboxConfig) -> PluginResult<bool> {
        if !config.enabled {
            return Ok(true); // No sandboxing, allow everything
        }

        match capability {
            PluginCapability::FileSystem { read_paths, write_paths } => {
                if config.filesystem_isolation {
                    // Check if paths are allowed
                    for path in write_paths {
                        if self.is_path_restricted(path, config) {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            PluginCapability::Network { allowed_hosts, allowed_ports } => {
                if config.network_isolation {
                    // Check network access restrictions
                    Ok(allowed_hosts.is_empty() && allowed_ports.is_empty())
                } else {
                    Ok(true)
                }
            }
            PluginCapability::SystemCalls { allowed_calls } => {
                if config.process_isolation {
                    // Check against blocked syscalls
                    for syscall in allowed_calls {
                        if config.blocked_syscalls.contains(syscall) {
                            return Ok(false);
                        }
                    }
                    // Only allow explicitly allowed syscalls
                    Ok(allowed_calls.iter().all(|call| config.allowed_syscalls.contains(call)))
                } else {
                    Ok(true)
                }
            }
            _ => Ok(true),
        }
    }

    /// Check if path is restricted by sandbox
    fn is_path_restricted(&self, path: &str, config: &SandboxConfig) -> bool {
        let restricted_paths = [
            "/etc", "/boot", "/sys", "/proc/sys", "/dev/mem",
            "/dev/kmem", "/dev/port", "/root", "/usr/src"
        ];

        for restricted in &restricted_paths {
            if path.starts_with(restricted) {
                return true;
            }
        }

        false
    }

    /// Generate sandbox ID
    fn generate_sandbox_id(&self, plugin_id: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        plugin_id.hash(&mut hasher);
        let hash = hasher.finish();

        format!("sandbox-{}-{}", plugin_id, hash)
    }

    /// Create Linux namespaces for sandbox
    #[cfg(unix)]
    async fn create_linux_namespaces(&self, config: &SandboxConfig) -> PluginResult<SandboxNamespaces> {
        let mut namespaces = SandboxNamespaces {
            pid: None,
            network: None,
            mount: None,
            ipc: None,
            user: None,
            uts: None,
        };

        if config.namespace_isolation {
            // Create namespace names (in a real implementation, you'd actually create the namespaces)
            if config.process_isolation {
                namespaces.pid = Some(format!("pid-{}", uuid::Uuid::new_v4()));
            }
            if config.network_isolation {
                namespaces.network = Some(format!("net-{}", uuid::Uuid::new_v4()));
            }
            if config.filesystem_isolation {
                namespaces.mount = Some(format!("mnt-{}", uuid::Uuid::new_v4()));
            }
        }

        Ok(namespaces)
    }

    #[cfg(not(unix))]
    async fn create_linux_namespaces(&self, _config: &SandboxConfig) -> PluginResult<SandboxNamespaces> {
        // Non-Unix platforms don't have Linux namespaces
        Ok(SandboxNamespaces {
            pid: None,
            network: None,
            mount: None,
            ipc: None,
            user: None,
            uts: None,
        })
    }
}

#[async_trait]
impl SecurityManager for DefaultSecurityManager {
    async fn start(&mut self) -> PluginResult<()> {
        info!("Starting security manager");

        {
            let mut running = self.running.write().await;
            if *running {
                return Err(PluginError::security("Security manager is already running".to_string()));
            }
            *running = true;
        }

        // Initialize audit log
        self.initialize_audit_log().await?;

        // Initialize state
        {
            let mut state = self.state.write().await;
            state.started_at = Some(SystemTime::now());
        }

        // Load default policies if none exist
        {
            let mut policies = self.policies.write().await;
            if policies.is_empty() {
                *policies = self.create_default_policies();
            }
        }

        info!("Security manager started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> PluginResult<()> {
        info!("Stopping security manager");

        {
            let mut running = self.running.write().await;
            *running = false;
        }

        // Flush audit log
        {
            let mut audit_writer = self.audit_writer.write().await;
            if let Some(writer) = audit_writer.as_mut() {
                writer.flush().await
                    .map_err(|e| PluginError::security(format!("Failed to flush audit log: {}", e)))?;
                *audit_writer = None;
            }
        }

        // Clean up active sandboxes
        {
            let mut sandboxes = self.sandboxes.write().await;
            for (sandbox_id, _) in sandboxes.drain() {
                info!("Cleaning up sandbox on shutdown: {}", sandbox_id);
                // In a real implementation, you'd actually destroy the sandbox
            }
        }

        info!("Security manager stopped");
        Ok(())
    }

    async fn validate_plugin_security(&self, manifest: &PluginManifest) -> PluginResult<SecurityValidationResult> {
        debug!("Validating security for plugin: {}", manifest.id);

        let mut issues = Vec::new();
        let mut recommendations = Vec::new();

        // Validate capabilities
        for capability in &manifest.capabilities {
            if let Err(issue) = self.validate_capability_security(capability, &manifest.sandbox_config) {
                issues.push(issue);
            }
        }

        // Validate resource limits
        if let Err(issue) = self.validate_resource_limits(&manifest.resource_limits) {
            issues.push(issue);
        }

        // Validate dependencies
        for dependency in &manifest.dependencies {
            if let Err(issue) = self.validate_dependency_security(dependency) {
                issues.push(issue);
            }
        }

        // Determine security level
        let security_level = if issues.is_empty() {
            SecurityLevel::Basic
        } else {
            let critical_count = issues.iter()
                .filter(|issue| matches!(issue.severity, SecuritySeverity::Critical))
                .count();

            if critical_count > 0 {
                SecurityLevel::Maximum
            } else {
                SecurityLevel::Strict
            }
        };

        let validation_result = SecurityValidationResult {
            passed: issues.is_empty(),
            security_level: security_level.clone(),
            issues,
            recommendations,
            validated_at: SystemTime::now(),
        };

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: manifest.id.clone(),
            instance_id: None,
            event_type: SecurityAuditEventType::PluginValidation,
            severity: if validation_result.passed {
                SecuritySeverity::Low
            } else {
                SecuritySeverity::Medium
            },
            description: format!("Security validation completed for plugin {}", manifest.id),
            source_location: None,
            context: HashMap::from([
                ("security_level".to_string(), format!("{:?}", security_level)),
                ("issues_found".to_string(), validation_result.issues.len().to_string()),
            ]),
            action_taken: if validation_result.passed {
                Some(SecurityAction::Allow)
            } else {
                Some(SecurityAction::Warn)
            },
        };

        self.audit_event(audit_event).await?;

        // Update metrics
        {
            let mut state = self.state.write().await;
            state.total_validations += 1;
        }

        self.update_metrics(&SecurityAuditEventType::PluginValidation, SecuritySeverity::Low).await;

        Ok(validation_result)
    }

    async fn create_sandbox(&self, plugin_id: &str, config: &SandboxConfig) -> PluginResult<SandboxEnvironment> {
        debug!("Creating sandbox for plugin: {}", plugin_id);

        if !config.enabled {
            // Return a minimal sandbox environment
            return Ok(SandboxEnvironment {
                sandbox_id: format!("nosandbox-{}", plugin_id),
                plugin_id: plugin_id.to_string(),
                config: config.clone(),
                namespaces: SandboxNamespaces {
                    pid: None,
                    network: None,
                    mount: None,
                    ipc: None,
                    user: None,
                    uts: None,
                },
                resource_limits: ResourceLimits::default(),
                created_at: SystemTime::now(),
                pid: None,
                network_namespace: None,
                mount_namespace: None,
            });
        }

        let sandbox_id = self.generate_sandbox_id(plugin_id);

        // Create namespaces
        let namespaces = self.create_linux_namespaces(config).await?;

        let sandbox = SandboxEnvironment {
            sandbox_id: sandbox_id.clone(),
            plugin_id: plugin_id.to_string(),
            config: config.clone(),
            namespaces,
            resource_limits: config.resource_limits.clone(),
            created_at: SystemTime::now(),
            pid: None,
            network_namespace: None,
            mount_namespace: None,
        };

        // Store sandbox
        {
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.insert(sandbox_id.clone(), sandbox.clone());
        }

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: plugin_id.to_string(),
            instance_id: None,
            event_type: SecurityAuditEventType::SandboxCreated,
            severity: SecuritySeverity::Low,
            description: format!("Created sandbox {} for plugin {}", sandbox_id, plugin_id),
            source_location: None,
            context: HashMap::from([
                ("sandbox_type".to_string(), format!("{:?}", config.sandbox_type)),
                ("namespace_isolation".to_string(), config.namespace_isolation.to_string()),
            ]),
            action_taken: Some(SecurityAction::Allow),
        };

        self.audit_event(audit_event).await?;

        // Update metrics
        {
            let mut state = self.state.write().await;
            state.total_sandboxes_created += 1;
        }

        self.update_metrics(&SecurityAuditEventType::SandboxCreated, SecuritySeverity::Low).await;

        // Publish event
        self.publish_event(SecurityEvent::SandboxEvent {
            sandbox_id: sandbox_id.clone(),
            event_type: SandboxEventType::Created,
        }).await;

        info!("Created sandbox: {} for plugin: {}", sandbox_id, plugin_id);
        Ok(sandbox)
    }

    async fn destroy_sandbox(&self, sandbox_id: &str) -> PluginResult<()> {
        debug!("Destroying sandbox: {}", sandbox_id);

        let sandbox = {
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.remove(sandbox_id)
                .ok_or_else(|| PluginError::security(format!("Sandbox {} not found", sandbox_id)))?
        };

        // In a real implementation, you'd actually destroy the sandbox resources
        // Clean up namespaces, unmount file systems, kill processes, etc.

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: sandbox.plugin_id.clone(),
            instance_id: None,
            event_type: SecurityAuditEventType::SandboxDestroyed,
            severity: SecuritySeverity::Low,
            description: format!("Destroyed sandbox {} for plugin {}", sandbox_id, sandbox.plugin_id),
            source_location: None,
            context: HashMap::new(),
            action_taken: Some(SecurityAction::Allow),
        };

        self.audit_event(audit_event).await?;

        // Update metrics
        self.update_metrics(&SecurityAuditEventType::SandboxDestroyed, SecuritySeverity::Low).await;

        // Publish event
        self.publish_event(SecurityEvent::SandboxEvent {
            sandbox_id: sandbox_id.to_string(),
            event_type: SandboxEventType::Destroyed,
        }).await;

        info!("Destroyed sandbox: {}", sandbox_id);
        Ok(())
    }

    async fn check_capability(&self, plugin_id: &str, capability: &PluginCapability, context: &SecurityContext) -> PluginResult<bool> {
        debug!("Checking capability for plugin {}: {:?}", plugin_id, capability);

        // Get sandbox configuration for plugin
        let sandbox_config = {
            let sandboxes = self.sandboxes.read().await;
            sandboxes.values()
                .find(|s| s.plugin_id == plugin_id)
                .map(|s| s.config.clone())
                .unwrap_or_else(|| self.config.default_sandbox.clone())
        };

        // Check sandbox capability restrictions
        let sandbox_allowed = self.check_sandbox_capability(capability, &sandbox_config).await?;

        // Check security policies
        let policy_allowed = self.check_security_policies(plugin_id, capability, context).await?;

        let allowed = sandbox_allowed && policy_allowed;

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: plugin_id.to_string(),
            instance_id: context.instance_id.clone(),
            event_type: SecurityAuditEventType::CapabilityCheck,
            severity: if allowed {
                SecuritySeverity::Low
            } else {
                SecuritySeverity::Medium
            },
            description: format!("Capability check: {:?} - {}", capability, if allowed { "allowed" } else { "denied" }),
            source_location: None,
            context: HashMap::from([
                ("capability".to_string(), format!("{:?}", capability)),
                ("sandbox_allowed".to_string(), sandbox_allowed.to_string()),
                ("policy_allowed".to_string(), policy_allowed.to_string()),
                ("security_level".to_string(), format!("{:?}", context.level)),
            ]),
            action_taken: Some(if allowed {
                SecurityAction::Allow
            } else {
                SecurityAction::Deny
            }),
        };

        self.audit_event(audit_event).await?;

        if !allowed {
            self.update_metrics(&SecurityAuditEventType::AccessDenied, SecuritySeverity::Medium).await;

            // Publish security event
            self.publish_event(SecurityEvent::PolicyEvaluated {
                plugin_id: plugin_id.to_string(),
                capability: capability.clone(),
                allowed: false,
                reason: Some("Capability not allowed by security policy".to_string()),
            }).await;
        }

        Ok(allowed)
    }

    async fn apply_security_policy(&self, plugin_id: &str, policy: &SecurityPolicy) -> PluginResult<()> {
        debug!("Applying security policy to plugin: {}", plugin_id);

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: plugin_id.to_string(),
            instance_id: None,
            event_type: SecurityAuditEventType::PolicyApplied,
            severity: SecuritySeverity::Low,
            description: format!("Applied security policy {} to plugin {}", policy.name, plugin_id),
            source_location: None,
            context: HashMap::from([
                ("policy_name".to_string(), policy.name.clone()),
                ("policy_version".to_string(), policy.version.clone()),
                ("default_level".to_string(), format!("{:?}", policy.default_level)),
            ]),
            action_taken: Some(SecurityAction::Allow),
        };

        self.audit_event(audit_event).await?;

        info!("Applied security policy {} to plugin {}", policy.name, plugin_id);
        Ok(())
    }

    async fn audit_event(&self, event: SecurityAuditEvent) -> PluginResult<()> {
        // Write to audit log
        self.write_audit_event(&event).await?;

        // Update metrics
        self.update_metrics(&event.event_type, event.severity.clone()).await;

        // Publish event
        self.publish_event(SecurityEvent::AuditEvent {
            event: event.clone(),
        }).await;

        // Check for real-time alerts
        if self.config.audit.real_time_monitoring {
            self.check_alert_conditions(&event).await?;
        }

        Ok(())
    }

    async fn get_security_metrics(&self) -> PluginResult<SecurityMetrics> {
        let metrics = self.metrics.read().await;
        let mut metrics_clone = metrics.clone();

        // Update active sandboxes count
        {
            let sandboxes = self.sandboxes.read().await;
            metrics_clone.active_sandboxes = sandboxes.len() as u64;
        }

        Ok(metrics_clone)
    }

    async fn subscribe(&mut self) -> mpsc::UnboundedReceiver<SecurityEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.event_subscribers.write().await;
        subscribers.push(tx);
        rx
    }

    async fn update_policies(&mut self, policies: Vec<SecurityRule>) -> PluginResult<()> {
        debug!("Updating security policies: {} rules", policies.len());

        {
            let mut current_policies = self.policies.write().await;
            *current_policies = policies;
        }

        // Create audit event
        let audit_event = SecurityAuditEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now(),
            plugin_id: "system".to_string(),
            instance_id: None,
            event_type: SecurityAuditEventType::ConfigurationChanged,
            severity: SecuritySeverity::Low,
            description: "Security policies updated".to_string(),
            source_location: None,
            context: HashMap::from([
                ("policy_count".to_string(), current_policies.read().await.len().to_string()),
            ]),
            action_taken: Some(SecurityAction::Allow),
        };

        self.audit_event(audit_event).await?;

        info!("Security policies updated");
        Ok(())
    }

    async fn generate_security_report(&self, time_range: Option<(SystemTime, SystemTime)>) -> PluginResult<SecurityReport> {
        debug!("Generating security report");

        let metrics = self.metrics.read().await;
        let state = self.state.read().await;

        // Generate summary
        let summary = SecurityReportSummary {
            total_events: metrics.total_events,
            critical_events: *metrics.events_by_severity.get(&SecuritySeverity::Critical).unwrap_or(&0),
            high_events: *metrics.events_by_severity.get(&SecuritySeverity::High).unwrap_or(&0),
            medium_events: *metrics.events_by_severity.get(&SecuritySeverity::Medium).unwrap_or(&0),
            low_events: *metrics.events_by_severity.get(&SecuritySeverity::Low).unwrap_or(&0),
            violations: metrics.violations,
            average_risk_score: self.calculate_average_risk_score().await,
        };

        // Generate top issues
        let top_issues = self.analyze_top_issues().await;

        // Generate plugin ratings
        let plugin_ratings = self.generate_plugin_ratings().await;

        // Generate recommendations
        let recommendations = self.generate_recommendations(&summary, &top_issues).await;

        let report = SecurityReport {
            report_id: uuid::Uuid::new_v4().to_string(),
            time_range: time_range.unwrap_or((
                state.started_at.unwrap_or(SystemTime::UNIX_EPOCH),
                SystemTime::now(),
            )),
            summary,
            top_issues,
            plugin_ratings,
            recommendations,
            generated_at: SystemTime::now(),
        };

        info!("Security report generated: {}", report.report_id);
        Ok(report)
    }
}

impl DefaultSecurityManager {
    /// Validate capability security
    fn validate_capability_security(&self, capability: &PluginCapability, sandbox_config: &SandboxConfig) -> Result<(), SecurityIssue> {
        match capability {
            PluginCapability::FileSystem { write_paths, .. } => {
                if !sandbox_config.filesystem_isolation && !write_paths.is_empty() {
                    return Err(SecurityIssue {
                        issue_type: SecurityIssueType::FileSystemAccess,
                        severity: SecuritySeverity::High,
                        description: "Plugin requests file system write access but sandbox filesystem isolation is disabled".to_string(),
                        location: Some("capabilities".to_string()),
                        recommendation: Some("Enable filesystem isolation in sandbox configuration".to_string()),
                    });
                }
            }
            PluginCapability::Network { .. } => {
                if !sandbox_config.network_isolation {
                    return Err(SecurityIssue {
                        issue_type: SecurityIssueType::NetworkAccess,
                        severity: SecuritySeverity::Medium,
                        description: "Plugin requests network access but sandbox network isolation is disabled".to_string(),
                        location: Some("capabilities".to_string()),
                        recommendation: Some("Enable network isolation in sandbox configuration".to_string()),
                    });
                }
            }
            PluginCapability::SystemCalls { .. } => {
                if !sandbox_config.process_isolation {
                    return Err(SecurityIssue {
                        issue_type: SecurityIssueType::SystemCallAccess,
                        severity: SecuritySeverity::Critical,
                        description: "Plugin requests system call access but sandbox process isolation is disabled".to_string(),
                        location: Some("capabilities".to_string()),
                        recommendation: Some("Enable process isolation in sandbox configuration".to_string()),
                    });
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Validate resource limits
    fn validate_resource_limits(&self, limits: &ResourceLimits) -> Result<(), SecurityIssue> {
        if let Some(max_memory) = limits.max_memory_bytes {
            if max_memory > 8 * 1024 * 1024 * 1024 { // 8GB
                return Err(SecurityIssue {
                    issue_type: SecurityIssueType::ResourceExhaustion,
                    severity: SecuritySeverity::Medium,
                    description: format!("Plugin requests high memory limit: {} MB", max_memory / 1024 / 1024),
                    location: Some("resource_limits".to_string()),
                    recommendation: Some("Consider reducing memory limit to prevent resource exhaustion".to_string()),
                });
            }
        }

        Ok(())
    }

    /// Validate dependency security
    fn validate_dependency_security(&self, dependency: &PluginDependency) -> Result<(), SecurityIssue> {
        // Check for known insecure dependencies
        let insecure_dependencies = [
            "old-crypto-lib", "deprecated-http-client", "vulnerable-parser"
        ];

        if insecure_dependencies.contains(&dependency.name.as_str()) {
            return Err(SecurityIssue {
                issue_type: SecurityIssueType::InsecureDependencies,
                severity: SecuritySeverity::High,
                description: format!("Plugin depends on potentially insecure dependency: {}", dependency.name),
                location: Some("dependencies".to_string()),
                recommendation: Some("Update to a secure version of the dependency".to_string()),
            });
        }

        Ok(())
    }

    /// Create default security policies
    fn create_default_policies(&self) -> Vec<SecurityRule> {
        vec![
            SecurityRule {
                name: "deny-etc-access".to_string(),
                rule_type: SecurityRuleType::Deny,
                conditions: vec![
                    SecurityCondition {
                        field: "capability".to_string(),
                        operator: SecurityOperator::Equals,
                        value: serde_json::Value::String("FileSystem".to_string()),
                    },
                    SecurityCondition {
                        field: "path".to_string(),
                        operator: SecurityOperator::StartsWith,
                        value: serde_json::Value::String("/etc".to_string()),
                    },
                ],
                actions: vec![
                    SecurityAction {
                        action_type: SecurityActionType::Block,
                        parameters: HashMap::new(),
                    },
                ],
                priority: 100,
                enabled: true,
            },
            SecurityRule {
                name: "limit-network-access".to_string(),
                rule_type: SecurityRuleType::Log,
                conditions: vec![
                    SecurityCondition {
                        field: "capability".to_string(),
                        operator: SecurityOperator::Equals,
                        value: serde_json::Value::String("Network".to_string()),
                    },
                ],
                actions: vec![
                    SecurityAction {
                        action_type: SecurityActionType::Log,
                        parameters: HashMap::new(),
                    },
                ],
                priority: 50,
                enabled: true,
            },
        ]
    }

    /// Check security policies for capability
    async fn check_security_policies(&self, plugin_id: &str, capability: &PluginCapability, context: &SecurityContext) -> PluginResult<bool> {
        let policies = self.policies.read().await;

        for policy in policies.iter().filter(|p| p.enabled) {
            if self.evaluate_policy(policy, capability, context).await? {
                match policy.rule_type {
                    SecurityRuleType::Deny => return Ok(false),
                    SecurityRuleType::Allow => return Ok(true),
                    SecurityRuleType::Log => {
                        // Log but continue evaluation
                    },
                    SecurityRuleType::Alert => {
                        // Generate alert but continue evaluation
                    },
                    SecurityRuleType::Block => return Ok(false),
                    SecurityRuleType::Custom(_) => {
                        // Custom policy handling
                    },
                }
            }
        }

        Ok(true) // Default allow if no policies deny
    }

    /// Evaluate a security policy
    async fn evaluate_policy(&self, policy: &SecurityRule, capability: &PluginCapability, context: &SecurityContext) -> PluginResult<bool> {
        for condition in &policy.conditions {
            if !self.evaluate_condition(condition, capability, context).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate a policy condition
    async fn evaluate_condition(&self, condition: &SecurityCondition, capability: &PluginCapability, context: &SecurityContext) -> PluginResult<bool> {
        match condition.field.as_str() {
            "capability" => {
                let capability_str = format!("{:?}", capability);
                self.evaluate_operator(&condition.operator, &capability_str, &condition.value)
            }
            "security_level" => {
                let level_str = format!("{:?}", context.level);
                self.evaluate_operator(&condition.operator, &level_str, &condition.value)
            }
            _ => Ok(false),
        }
    }

    /// Evaluate an operator
    fn evaluate_operator(&self, operator: &SecurityOperator, actual: &str, expected: &serde_json::Value) -> PluginResult<bool> {
        let expected_str = expected.as_str().unwrap_or("");

        match operator {
            SecurityOperator::Equals => Ok(actual == expected_str),
            SecurityOperator::NotEquals => Ok(actual != expected_str),
            SecurityOperator::Contains => Ok(actual.contains(expected_str)),
            SecurityOperator::Matches => {
                // Simple regex matching
                Ok(actual.contains(expected_str))
            }
            _ => Ok(false),
        }
    }

    /// Check alert conditions for audit event
    async fn check_alert_conditions(&self, event: &SecurityAuditEvent) -> PluginResult<()> {
        // Check alert thresholds
        if let Some(ref thresholds) = self.config.audit.alert_thresholds {
            // Check error rate threshold
            if let Some(max_errors) = thresholds.errors_per_minute {
                // This would require tracking errors per minute
                // For now, just log critical events
                if matches!(event.severity, SecuritySeverity::Critical) {
                    self.publish_event(SecurityEvent::SecurityAlert {
                        alert_type: SecurityAlertType::SecurityBreach,
                        message: format!("Critical security event: {}", event.description),
                        plugin_id: Some(event.plugin_id.clone()),
                    }).await;
                }
            }
        }

        Ok(())
    }

    /// Calculate average risk score
    async fn calculate_average_risk_score(&self) -> f64 {
        let metrics = self.metrics.read().await;
        let total_events = metrics.total_events;

        if total_events == 0 {
            return 0.0;
        }

        let critical_events = *metrics.events_by_severity.get(&SecuritySeverity::Critical).unwrap_or(&0);
        let high_events = *metrics.events_by_severity.get(&SecuritySeverity::High).unwrap_or(&0);
        let medium_events = *metrics.events_by_severity.get(&SecuritySeverity::Medium).unwrap_or(&0);

        // Weighted risk score (0-100)
        let risk_score = ((critical_events as f64 * 100.0) +
                         (high_events as f64 * 75.0) +
                         (medium_events as f64 * 50.0)) /
                        (total_events as f64);

        risk_score.min(100.0)
    }

    /// Analyze top security issues
    async fn analyze_top_issues(&self) -> Vec<SecurityIssueSummary> {
        // This would analyze the recent security events to identify trends
        // For now, return an empty list
        vec![]
    }

    /// Generate plugin security ratings
    async fn generate_plugin_ratings(&self) -> HashMap<String, SecurityRating> {
        // This would generate security ratings for each plugin based on their security events
        // For now, return an empty map
        HashMap::new()
    }

    /// Generate security recommendations
    async fn generate_recommendations(&self, summary: &SecurityReportSummary, top_issues: &[SecurityIssueSummary]) -> Vec<SecurityRecommendation> {
        let mut recommendations = Vec::new();

        if summary.average_risk_score > 70.0 {
            recommendations.push(SecurityRecommendation {
                recommendation_id: uuid::Uuid::new_v4().to_string(),
                recommendation_type: RecommendationType::SecurityImprovement,
                priority: RecommendationPriority::High,
                title: "High Security Risk Detected".to_string(),
                description: format!("The system has an average risk score of {:.1}, which indicates significant security concerns.", summary.average_risk_score),
                affected_plugins: vec![],
                suggested_actions: vec![
                    "Review and update security policies".to_string(),
                    "Increase sandboxing restrictions".to_string(),
                    "Monitor plugin activity more closely".to_string(),
                ],
            });
        }

        if summary.violations > 10 {
            recommendations.push(SecurityRecommendation {
                recommendation_id: uuid::Uuid::new_v4().to_string(),
                recommendation_type: RecommendationType::PolicyUpdate,
                priority: RecommendationPriority::Medium,
                title: "Multiple Security Violations".to_string(),
                description: format!("{} security violations have been detected, indicating potential policy gaps.", summary.violations),
                affected_plugins: vec![],
                suggested_actions: vec![
                    "Review and strengthen security policies".to_string(),
                    "Consider blocking high-risk capabilities".to_string(),
                    "Implement more granular access controls".to_string(),
                ],
            });
        }

        recommendations
    }
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

/// Create a default security manager
pub fn create_security_manager(config: SecurityConfig) -> Box<dyn SecurityManager> {
    Box::new(DefaultSecurityManager::new(config))
}