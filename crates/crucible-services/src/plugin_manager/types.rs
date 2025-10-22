//! # Core Plugin Types and Data Structures
//!
//! This module defines the fundamental types and data structures used throughout
//! the PluginManager system, including plugin manifests, instance states, and
//! capability definitions.

use super::error::{PluginError, PluginResult};
use super::config::{PluginManagerConfig, SandboxConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// ============================================================================
/// PLUGIN TYPES AND MANIFESTS
/// ============================================================================

/// Plugin type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginType {
    /// Rune script-based plugin
    Rune,
    /// Native binary executable
    Binary,
    /// WebAssembly module
    Wasm,
    /// External microservice
    Microservice,
    /// Python script
    Python,
    /// JavaScript/Node.js plugin
    JavaScript,
}

/// Plugin manifest containing metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Unique plugin identifier
    pub id: String,
    /// Plugin name
    pub name: String,
    /// Plugin description
    pub description: String,
    /// Plugin version
    pub version: String,
    /// Plugin type
    pub plugin_type: PluginType,
    /// Plugin author
    pub author: String,
    /// Plugin license
    pub license: Option<String>,
    /// Plugin homepage URL
    pub homepage: Option<String>,
    /// Plugin repository URL
    pub repository: Option<String>,
    /// Plugin tags
    pub tags: Vec<String>,
    /// Entry point (script file, binary path, etc.)
    pub entry_point: PathBuf,
    /// Plugin capabilities
    pub capabilities: Vec<PluginCapability>,
    /// Required permissions
    pub permissions: Vec<PluginPermission>,
    /// Plugin dependencies
    pub dependencies: Vec<PluginDependency>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Sandbox configuration
    pub sandbox_config: SandboxConfig,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Plugin configuration schema
    pub config_schema: Option<serde_json::Value>,
    /// Minimum required Crucible version
    pub min_crucible_version: Option<String>,
    /// Maximum supported Crucible version
    pub max_crucible_version: Option<String>,
    /// Plugin creation timestamp
    pub created_at: SystemTime,
    /// Plugin last modified timestamp
    pub modified_at: SystemTime,
}

/// Plugin capability definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginCapability {
    /// File system access
    FileSystem {
        read_paths: Vec<String>,
        write_paths: Vec<String>,
    },
    /// Network access
    Network {
        allowed_hosts: Vec<String>,
        allowed_ports: Vec<u16>,
    },
    /// System calls
    SystemCalls {
        allowed_calls: Vec<String>,
    },
    /// Database access
    Database {
        databases: Vec<String>,
        operations: Vec<String>,
    },
    /// IPC communication
    IpcCommunication,
    /// Tool execution
    ToolExecution,
    /// Script execution
    ScriptExecution,
    /// Custom capability
    Custom {
        name: String,
        config: HashMap<String, String>,
    },
}

/// Plugin permission definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginPermission {
    /// Read access to file system
    FileSystemRead,
    /// Write access to file system
    FileSystemWrite,
    /// Execute files
    FileExecute,
    /// Network access
    NetworkAccess,
    /// System calls
    SystemCalls,
    /// Process control
    ProcessControl,
    /// Hardware access
    HardwareAccess,
    /// Database access
    DatabaseAccess,
    /// IPC communication
    IpcCommunication,
    /// Custom permission
    Custom(String),
}

/// Plugin dependency definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDependency {
    /// Dependency name
    pub name: String,
    /// Required version
    pub version: Option<String>,
    /// Dependency type
    pub dependency_type: DependencyType,
    /// Optional flag
    pub optional: bool,
}

/// Dependency type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DependencyType {
    /// Plugin dependency
    Plugin,
    /// System library
    SystemLibrary,
    /// Runtime dependency
    Runtime,
    /// Development dependency
    Development,
}

/// ============================================================================
/// PLUGIN INSTANCE MANAGEMENT
/// ============================================================================

/// Plugin instance state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginInstanceState {
    /// Instance is created but not started
    Created,
    /// Instance is starting
    Starting,
    /// Instance is running
    Running,
    /// Instance is stopping
    Stopping,
    /// Instance is stopped
    Stopped,
    /// Instance encountered an error
    Error(String),
    /// Instance is crashed
    Crashed,
}

/// Plugin instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstance {
    /// Unique instance identifier
    pub instance_id: String,
    /// Plugin identifier
    pub plugin_id: String,
    /// Instance state
    pub state: PluginInstanceState,
    /// Process ID (if applicable)
    pub pid: Option<u32>,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Start timestamp
    pub started_at: Option<SystemTime>,
    /// Last activity timestamp
    pub last_activity: Option<SystemTime>,
    /// Instance configuration
    pub config: HashMap<String, serde_json::Value>,
    /// Current resource usage
    pub resource_usage: ResourceUsage,
    /// Resource limits for this instance
    pub resource_limits: ResourceLimits,
    /// Health status
    pub health_status: PluginHealthStatus,
    /// Error information (if in error state)
    pub error_info: Option<PluginErrorInfo>,
    /// Number of restarts
    pub restart_count: u32,
    /// Execution statistics
    pub execution_stats: PluginExecutionStats,
}

/// Plugin health status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginHealthStatus {
    /// Plugin is healthy
    Healthy,
    /// Plugin is degraded but functional
    Degraded,
    /// Plugin is unhealthy
    Unhealthy,
    /// Unknown health status
    Unknown,
}

/// Plugin error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginErrorInfo {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Error stack trace
    pub stack_trace: Option<String>,
    /// Error timestamp
    pub timestamp: SystemTime,
    /// Number of occurrences
    pub occurrence_count: u32,
}

/// Plugin execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExecutionStats {
    /// Total executions
    pub total_executions: u64,
    /// Successful executions
    pub successful_executions: u64,
    /// Failed executions
    pub failed_executions: u64,
    /// Average execution time
    pub average_execution_time: Duration,
    /// Total execution time
    pub total_execution_time: Duration,
    /// Peak memory usage
    pub peak_memory_usage: u64,
    /// Total CPU time
    pub total_cpu_time: Duration,
    /// Last execution timestamp
    pub last_execution: Option<SystemTime>,
    /// Executions by hour (for load analysis)
    pub executions_by_hour: HashMap<String, u64>,
}

/// ============================================================================
/// RESOURCE MANAGEMENT
/// ============================================================================

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// CPU usage percentage (0.0-100.0)
    pub cpu_percentage: f64,
    /// Disk usage in bytes
    pub disk_bytes: u64,
    /// Network usage in bytes
    pub network_bytes: u64,
    /// Number of open file descriptors
    pub open_files: u32,
    /// Number of active threads
    pub active_threads: u32,
    /// Number of child processes
    pub child_processes: u32,
    /// Measurement timestamp
    pub measured_at: SystemTime,
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    /// Maximum memory in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum CPU percentage
    pub max_cpu_percentage: Option<f64>,
    /// Maximum disk space in bytes
    pub max_disk_bytes: Option<u64>,
    /// Maximum concurrent operations
    pub max_concurrent_operations: Option<u32>,
    /// Maximum number of child processes
    pub max_child_processes: Option<u32>,
    /// Maximum number of open files
    pub max_open_files: Option<u32>,
    /// Operation timeout
    pub operation_timeout: Option<Duration>,
    /// Idle timeout
    pub idle_timeout: Option<Duration>,
}

/// ============================================================================
/// PLUGIN COMMUNICATION
/// ============================================================================

/// Plugin message types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginMessageType {
    /// Request message
    Request,
    /// Response message
    Response,
    /// Event notification
    Event,
    /// Health check
    HealthCheck,
    /// Configuration update
    ConfigUpdate,
    /// Shutdown request
    Shutdown,
    /// Custom message type
    Custom(String),
}

/// Plugin message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMessage {
    /// Unique message identifier
    pub message_id: String,
    /// Message type
    pub message_type: PluginMessageType,
    /// Source instance ID
    pub source_instance_id: Option<String>,
    /// Target instance ID
    pub target_instance_id: Option<String>,
    /// Message payload
    pub payload: serde_json::Value,
    /// Message timestamp
    pub timestamp: SystemTime,
    /// Correlation ID (for request/response matching)
    pub correlation_id: Option<String>,
    /// Message priority
    pub priority: MessagePriority,
    /// Timeout for response
    pub timeout: Option<Duration>,
}

/// Message priority
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Low priority
    Low = 1,
    /// Normal priority
    Normal = 2,
    /// High priority
    High = 3,
    /// Critical priority
    Critical = 4,
}

/// ============================================================================
/// PLUGIN REGISTRY AND DISCOVERY
/// ============================================================================

/// Plugin registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistryEntry {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Installation path
    pub install_path: PathBuf,
    /// Installation timestamp
    pub installed_at: SystemTime,
    /// Plugin status
    pub status: PluginRegistryStatus,
    /// Validation results
    pub validation_results: Option<PluginValidationResults>,
    /// Instance IDs for this plugin
    pub instance_ids: Vec<String>,
}

/// Plugin registry status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginRegistryStatus {
    /// Plugin is installed and available
    Installed,
    /// Plugin is disabled
    Disabled,
    /// Plugin has errors
    Error(String),
    /// Plugin is being installed
    Installing,
    /// Plugin is being uninstalled
    Uninstalling,
    /// Plugin needs update
    NeedsUpdate,
}

/// Plugin validation results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginValidationResults {
    /// Overall validation passed
    pub valid: bool,
    /// Security validation result
    pub security_validation: SecurityValidationResult,
    /// Dependency validation result
    pub dependency_validation: DependencyValidationResult,
    /// Compatibility validation result
    pub compatibility_validation: CompatibilityValidationResult,
    /// Validation timestamp
    pub validated_at: SystemTime,
}

/// Security validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityValidationResult {
    /// Security check passed
    pub passed: bool,
    /// Security issues found
    pub issues: Vec<SecurityIssue>,
    /// Security level assigned
    pub security_level: SecurityLevel,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Security issue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityIssue {
    /// Issue type
    pub issue_type: SecurityIssueType,
    /// Issue severity
    pub severity: SecuritySeverity,
    /// Issue description
    pub description: String,
    /// File location (if applicable)
    pub location: Option<String>,
    /// Recommendation
    pub recommendation: Option<String>,
}

/// Security issue type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityIssueType {
    /// File system access
    FileSystemAccess,
    /// Network access
    NetworkAccess,
    /// System call access
    SystemCallAccess,
    /// Code injection risk
    CodeInjection,
    /// Information disclosure
    InformationDisclosure,
    /// Resource exhaustion
    ResourceExhaustion,
    /// Privilege escalation
    PrivilegeEscalation,
    /// Denial of service
    DenialOfService,
    /// Insecure dependencies
    InsecureDependencies,
}

/// Security severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecuritySeverity {
    /// Low severity
    Low = 1,
    /// Medium severity
    Medium = 2,
    /// High severity
    High = 3,
    /// Critical severity
    Critical = 4,
}

/// Security level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityLevel {
    /// No security restrictions
    None,
    /// Basic security
    Basic,
    /// Strict security
    Strict,
    /// Maximum security
    Maximum,
}

/// Dependency validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyValidationResult {
    /// Dependencies validated
    pub passed: bool,
    /// Missing dependencies
    pub missing_dependencies: Vec<PluginDependency>,
    /// Version conflicts
    pub version_conflicts: Vec<VersionConflict>,
    /// Optional dependencies not found
    pub optional_missing: Vec<PluginDependency>,
}

/// Version conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConflict {
    /// Dependency name
    pub dependency_name: String,
    /// Required version
    pub required_version: String,
    /// Found version
    pub found_version: String,
    /// Conflict description
    pub conflict_description: String,
}

/// Compatibility validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityValidationResult {
    /// Compatibility check passed
    pub passed: bool,
    /// Crucible version compatibility
    pub crucible_version_compatible: bool,
    /// Platform compatibility
    pub platform_compatible: bool,
    /// Architecture compatibility
    pub architecture_compatible: bool,
    /// Compatibility issues
    pub issues: Vec<String>,
}

/// ============================================================================
/// PLUGIN LIFECYCLE EVENTS
/// ============================================================================

/// Plugin lifecycle event
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginLifecycleEvent {
    /// Plugin discovered
    Discovered { plugin_id: String },
    /// Plugin installed
    Installed { plugin_id: String },
    /// Plugin uninstalled
    Uninstalled { plugin_id: String },
    /// Plugin enabled
    Enabled { plugin_id: String },
    /// Plugin disabled
    Disabled { plugin_id: String },
    /// Plugin instance created
    InstanceCreated { instance_id: String, plugin_id: String },
    /// Plugin instance started
    InstanceStarted { instance_id: String, plugin_id: String },
    /// Plugin instance stopped
    InstanceStopped { instance_id: String, plugin_id: String },
    /// Plugin instance crashed
    InstanceCrashed { instance_id: String, plugin_id: String, error: String },
    /// Plugin instance restarted
    InstanceRestarted { instance_id: String, plugin_id: String },
    /// Plugin health changed
    HealthChanged { instance_id: String, old_status: PluginHealthStatus, new_status: PluginHealthStatus },
    /// Plugin error occurred
    Error { instance_id: String, plugin_id: String, error: String },
}

/// ============================================================================
/// UTILITY FUNCTIONS
/// ============================================================================

impl Default for PluginInstance {
    fn default() -> Self {
        Self {
            instance_id: Uuid::new_v4().to_string(),
            plugin_id: String::new(),
            state: PluginInstanceState::Created,
            pid: None,
            created_at: SystemTime::now(),
            started_at: None,
            last_activity: None,
            config: HashMap::new(),
            resource_usage: ResourceUsage::default(),
            resource_limits: ResourceLimits::default(),
            health_status: PluginHealthStatus::Unknown,
            error_info: None,
            restart_count: 0,
            execution_stats: PluginExecutionStats::default(),
        }
    }
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            memory_bytes: 0,
            cpu_percentage: 0.0,
            disk_bytes: 0,
            network_bytes: 0,
            open_files: 0,
            active_threads: 0,
            child_processes: 0,
            measured_at: SystemTime::now(),
        }
    }
}

impl Default for PluginExecutionStats {
    fn default() -> Self {
        Self {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            average_execution_time: Duration::ZERO,
            total_execution_time: Duration::ZERO,
            peak_memory_usage: 0,
            total_cpu_time: Duration::ZERO,
            last_execution: None,
            executions_by_hour: HashMap::new(),
        }
    }
}

impl PluginInstance {
    /// Create a new plugin instance
    pub fn new(plugin_id: String, config: HashMap<String, serde_json::Value>) -> Self {
        Self {
            plugin_id,
            config,
            ..Default::default()
        }
    }

    /// Check if the instance is running
    pub fn is_running(&self) -> bool {
        matches!(self.state, PluginInstanceState::Running)
    }

    /// Check if the instance can be restarted
    pub fn can_restart(&self) -> bool {
        matches!(self.state, PluginInstanceState::Stopped | PluginInstanceState::Error(_) | PluginInstanceState::Crashed)
    }

    /// Update the execution statistics
    pub fn update_execution_stats(&mut self, success: bool, execution_time: Duration, memory_usage: u64) {
        self.execution_stats.total_executions += 1;
        if success {
            self.execution_stats.successful_executions += 1;
        } else {
            self.execution_stats.failed_executions += 1;
        }

        self.execution_stats.total_execution_time += execution_time;
        self.execution_stats.average_execution_time =
            self.execution_stats.total_execution_time / self.execution_stats.total_executions as u32;

        if memory_usage > self.execution_stats.peak_memory_usage {
            self.execution_stats.peak_memory_usage = memory_usage;
        }

        self.last_activity = Some(SystemTime::now());
        self.execution_stats.last_execution = Some(SystemTime::now());
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.execution_stats.total_executions == 0 {
            1.0
        } else {
            self.execution_stats.successful_executions as f64 / self.execution_stats.total_executions as f64
        }
    }
}

impl PluginManifest {
    /// Validate the manifest
    pub fn validate(&self) -> PluginResult<()> {
        if self.id.is_empty() {
            return Err(PluginError::ValidationError("Plugin ID cannot be empty".to_string()));
        }

        if self.name.is_empty() {
            return Err(PluginError::ValidationError("Plugin name cannot be empty".to_string()));
        }

        if self.version.is_empty() {
            return Err(PluginError::ValidationError("Plugin version cannot be empty".to_string()));
        }

        if !self.entry_point.exists() {
            return Err(PluginError::ValidationError(format!(
                "Entry point does not exist: {:?}",
                self.entry_point
            )));
        }

        Ok(())
    }

    /// Check if the plugin is compatible with the given Crucible version
    pub fn is_compatible_with_version(&self, crucible_version: &str) -> bool {
        // Simple version compatibility check - in a real implementation,
        // this would use semantic versioning
        if let Some(min_version) = &self.min_crucible_version {
            // Compare versions (simplified)
            if min_version > crucible_version {
                return false;
            }
        }

        if let Some(max_version) = &self.max_crucible_version {
            if max_version < crucible_version {
                return false;
            }
        }

        true
    }

    /// Get plugin capabilities summary
    pub fn get_capabilities_summary(&self) -> Vec<String> {
        self.capabilities.iter().map(|cap| match cap {
            PluginCapability::FileSystem { .. } => "File System Access".to_string(),
            PluginCapability::Network { .. } => "Network Access".to_string(),
            PluginCapability::SystemCalls { .. } => "System Calls".to_string(),
            PluginCapability::Database { .. } => "Database Access".to_string(),
            PluginCapability::IpcCommunication => "IPC Communication".to_string(),
            PluginCapability::ToolExecution => "Tool Execution".to_string(),
            PluginCapability::ScriptExecution => "Script Execution".to_string(),
            PluginCapability::Custom { name, .. } => format!("Custom: {}", name),
        }).collect()
    }
}

/// Plugin execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginExecutionContext {
    /// Execution ID
    pub execution_id: String,
    /// Instance ID
    pub instance_id: String,
    /// Plugin ID
    pub plugin_id: String,
    /// Operation type
    pub operation: String,
    /// Input parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Execution timeout
    pub timeout: Option<Duration>,
    /// User context
    pub user_context: Option<UserContext>,
    /// Execution timestamp
    pub timestamp: SystemTime,
}

/// User context for plugin execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    /// User ID
    pub user_id: String,
    /// User permissions
    pub permissions: Vec<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// Request source
    pub source: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Health check strategy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckStrategy {
    /// Strategy name
    pub name: String,
    /// Strategy type
    pub strategy_type: HealthCheckType,
    /// Strategy configuration
    pub config: HashMap<String, serde_json::Value>,
    /// Enabled flag
    pub enabled: bool,
}

/// Health check type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HealthCheckType {
    /// Ping check (simple liveness)
    Ping,
    /// Process check (is process running)
    Process,
    /// Resource check (are resources within limits)
    Resource,
    /// Custom health check
    Custom,
}

/// Security context for capability checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityContext {
    /// Security level
    pub level: SecurityLevel,
    /// Instance ID (if applicable)
    pub instance_id: Option<String>,
    /// User ID (if applicable)
    pub user_id: Option<String>,
    /// Additional context
    pub metadata: HashMap<String, String>,
}

/// Security policy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    /// Policy name
    pub name: String,
    /// Policy version
    pub version: String,
    /// Default security level
    pub default_level: SecurityLevel,
    /// Security rules
    pub rules: Vec<SecurityRule>,
}

/// Security rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRule {
    /// Rule name
    pub name: String,
    /// Rule type
    pub rule_type: SecurityRuleType,
    /// Rule conditions
    pub conditions: Vec<SecurityCondition>,
    /// Rule actions
    pub actions: Vec<SecurityAction],
    /// Rule priority
    pub priority: u32,
    /// Enabled flag
    pub enabled: bool,
}

/// Security rule type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityRuleType {
    /// Allow rule
    Allow,
    /// Deny rule
    Deny,
    /// Log rule
    Log,
    /// Alert rule
    Alert,
    /// Block rule
    Block,
    /// Custom rule
    Custom(String),
}

/// Security condition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityCondition {
    /// Condition field
    pub field: String,
    /// Condition operator
    pub operator: SecurityOperator,
    /// Condition value
    pub value: serde_json::Value,
}

/// Security operator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityOperator {
    /// Equals
    Equals,
    /// Not equals
    NotEquals,
    /// Greater than
    GreaterThan,
    /// Less than
    LessThan,
    /// Contains
    Contains,
    /// Matches regex
    Matches,
    /// In list
    In,
    /// Not in list
    NotIn,
    /// Starts with
    StartsWith,
}

/// Security action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAction {
    /// Action type
    pub action_type: SecurityActionType,
    /// Action parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Security action type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecurityActionType {
    /// Block operation
    Block,
    /// Allow operation
    Allow,
    /// Log event
    Log,
    /// Send alert
    Alert,
    /// Terminate process
    Terminate,
    /// Quarantine plugin
    Quarantine,
    /// Custom action
    Custom(String),
}