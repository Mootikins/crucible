//! # Plugin Manager Configuration
//!
//! This module defines configuration structures for the PluginManager system,
//! including global settings, sandbox configurations, and plugin-specific settings.

use super::error::{PluginError, PluginResult};
use super::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// ============================================================================
/// MAIN CONFIGURATION
/// ============================================================================

/// Main PluginManager configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManagerConfig {
    /// Plugin directories to scan
    pub plugin_directories: Vec<PathBuf>,
    /// Auto-discovery settings
    pub auto_discovery: AutoDiscoveryConfig,
    /// Security settings
    pub security: SecurityConfig,
    /// Resource management settings
    pub resource_management: ResourceManagementConfig,
    /// Health monitoring settings
    pub health_monitoring: HealthMonitoringConfig,
    /// Communication settings
    pub communication: CommunicationConfig,
    /// Logging settings
    pub logging: LoggingConfig,
    /// Plugin lifecycle settings
    pub lifecycle: LifecycleConfig,
    /// Performance settings
    pub performance: PerformanceConfig,
}

/// Auto-discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoDiscoveryConfig {
    /// Enable automatic plugin discovery
    pub enabled: bool,
    /// Discovery scan interval
    pub scan_interval: Duration,
    /// Plugin file patterns to look for
    pub file_patterns: Vec<String>,
    /// Watch for file system changes
    pub watch_filesystem: bool,
    /// Auto-install discovered plugins
    pub auto_install: bool,
    /// Validation settings for discovered plugins
    pub validation: DiscoveryValidationConfig,
}

/// Discovery validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryValidationConfig {
    /// Validate manifests
    pub validate_manifests: bool,
    /// Validate signatures
    pub validate_signatures: bool,
    /// Security scan
    pub security_scan: bool,
    /// Dependency validation
    pub validate_dependencies: bool,
    /// Strict validation (fail on any error)
    pub strict: bool,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Default sandbox configuration
    pub default_sandbox: SandboxConfig,
    /// Trusted plugin signatures
    pub trusted_signatures: Vec<String>,
    /// Security policy settings
    pub policies: SecurityPolicyConfig,
    /// Audit settings
    pub audit: AuditConfig,
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Enable sandboxing
    pub enabled: bool,
    /// Sandbox type
    pub sandbox_type: SandboxType,
    /// Namespace isolation
    pub namespace_isolation: bool,
    /// File system isolation
    pub filesystem_isolation: bool,
    /// Network isolation
    pub network_isolation: bool,
    /// Process isolation
    pub process_isolation: bool,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Allowed system calls
    pub allowed_syscalls: Vec<String>,
    /// Blocked system calls
    pub blocked_syscalls: Vec<String>,
    /// Mount points for container-style sandboxing
    pub mount_points: Vec<MountPoint>,
    /// Environment variables in sandbox
    pub environment: HashMap<String, String>,
}

/// Sandbox type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxType {
    /// Process isolation (default)
    Process,
    /// Container isolation (Docker-like)
    Container,
    /// Virtual machine isolation
    VirtualMachine,
    /// Language-level isolation
    Language,
    /// No sandboxing (not recommended)
    None,
}

/// Mount point for sandboxing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPoint {
    /// Source path on host
    pub source: PathBuf,
    /// Target path in sandbox
    pub target: PathBuf,
    /// Mount type
    pub mount_type: MountType,
    /// Read-only flag
    pub read_only: bool,
    /// Mount options
    pub options: Vec<String>,
}

/// Mount type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MountType {
    /// Bind mount
    Bind,
    /// Tmpfs mount
    Tmpfs,
    /// Proc mount
    Proc,
    /// Sysfs mount
    Sysfs,
    /// Devpts mount
    Devpts,
}

/// Security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicyConfig {
    /// Default security level
    pub default_level: SecurityLevel,
    /// Security level configurations
    pub level_configs: HashMap<SecurityLevel, SecurityLevelConfig>,
    /// Custom security rules
    pub custom_rules: Vec<SecurityRule>,
}

/// Security level configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityLevelConfig {
    /// Security level name
    pub name: String,
    /// Resource limits for this level
    pub resource_limits: ResourceLimits,
    /// Allowed capabilities
    pub allowed_capabilities: Vec<PluginCapability>,
    /// Blocked capabilities
    pub blocked_capabilities: Vec<PluginCapability>,
    /// Sandbox configuration
    pub sandbox_config: SandboxConfig,
    /// Time limits
    pub time_limits: TimeLimits,
}

/// Time limits for security levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeLimits {
    /// Maximum execution time
    pub max_execution_time: Option<Duration>,
    /// Maximum idle time
    pub max_idle_time: Option<Duration>,
    /// Maximum total runtime
    pub max_total_runtime: Option<Duration>,
}

/// Security rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRule {
    /// Rule name
    pub name: String,
    /// Rule type
    pub rule_type: SecurityRuleType,
    /// Rule conditions
    pub conditions: Vec<SecurityCondition>,
    /// Rule actions
    pub actions: Vec<SecurityAction>,
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

/// Audit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit logging
    pub enabled: bool,
    /// Audit log file path
    pub log_file: Option<PathBuf>,
    /// Events to audit
    pub audit_events: Vec<AuditEventType>,
    /// Audit log retention period
    pub retention_period: Option<Duration>,
    /// Real-time monitoring
    pub real_time_monitoring: bool,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Audit event type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuditEventType {
    /// Plugin installation
    PluginInstall,
    /// Plugin uninstallation
    PluginUninstall,
    /// Plugin start
    PluginStart,
    /// Plugin stop
    PluginStop,
    /// Security violation
    SecurityViolation,
    /// Resource limit exceeded
    ResourceLimitExceeded,
    /// Configuration change
    ConfigChange,
    /// Access denied
    AccessDenied,
    /// Error occurred
    Error,
    /// Custom event
    Custom(String),
}

/// Alert thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Errors per minute threshold
    pub errors_per_minute: Option<u32>,
    /// Memory usage threshold
    pub memory_usage_percent: Option<f64>,
    /// CPU usage threshold
    pub cpu_usage_percent: Option<f64>,
    /// Failed login attempts
    pub failed_login_attempts: Option<u32>,
}

/// Resource management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagementConfig {
    /// Global resource limits
    pub global_limits: ResourceLimits,
    /// Per-plugin resource limits
    pub per_plugin_limits: ResourceLimits,
    /// Resource monitoring settings
    pub monitoring: ResourceMonitoringConfig,
    /// Resource enforcement settings
    pub enforcement: ResourceEnforcementConfig,
}

/// Resource monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMonitoringConfig {
    /// Enable resource monitoring
    pub enabled: bool,
    /// Monitoring interval
    pub interval: Duration,
    /// Metrics to collect
    pub metrics: Vec<ResourceMetric>,
    /// Historical data retention
    pub retention_period: Duration,
}

/// Resource metric type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceMetric {
    /// CPU usage
    CpuUsage,
    /// Memory usage
    MemoryUsage,
    /// Disk usage
    DiskUsage,
    /// Network usage
    NetworkUsage,
    /// File descriptor count
    FileDescriptors,
    /// Process count
    ProcessCount,
    /// Custom metric
    Custom(String),
}

/// Resource enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEnforcementConfig {
    /// Enable resource enforcement
    pub enabled: bool,
    /// Enforcement strategy
    pub strategy: EnforcementStrategy,
    /// Grace period before enforcement
    pub grace_period: Duration,
    /// Action when limits exceeded
    pub limit_exceeded_action: LimitExceededAction,
}

/// Enforcement strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnforcementStrategy {
    /// Hard limits (terminate when exceeded)
    Hard,
    /// Soft limits (throttle when exceeded)
    Soft,
    /// Adaptive limits (adjust based on usage)
    Adaptive,
}

/// Action when resource limits are exceeded
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LimitExceededAction {
    /// Terminate the plugin
    Terminate,
    /// Suspend the plugin
    Suspend,
    /// Throttle the plugin
    Throttle,
    /// Send warning
    Warn,
    /// Restart the plugin
    Restart,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMonitoringConfig {
    /// Enable health monitoring
    pub enabled: bool,
    /// Health check interval
    pub check_interval: Duration,
    /// Health check timeout
    pub check_timeout: Duration,
    /// Health check strategies
    pub strategies: Vec<HealthCheckStrategy>,
    /// Unhealthy threshold (consecutive failures)
    pub unhealthy_threshold: u32,
    /// Recovery strategies
    pub recovery: RecoveryConfig,
}

/// Health check strategy
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

/// Recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Enable automatic recovery
    pub enabled: bool,
    /// Maximum restart attempts
    pub max_restart_attempts: u32,
    /// Restart delay
    pub restart_delay: Duration,
    /// Backoff strategy
    pub backoff_strategy: BackoffStrategy,
    /// Escalation settings
    pub escalation: EscalationConfig,
}

/// Backoff strategy for restarts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Fixed delay
    Fixed,
    /// Exponential backoff
    Exponential,
    /// Linear backoff
    Linear,
}

/// Escalation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationConfig {
    /// Enable escalation
    pub enabled: bool,
    /// Escalation thresholds
    pub thresholds: Vec<EscalationThreshold>,
    /// Escalation actions
    pub actions: Vec<EscalationAction>,
}

/// Escalation threshold
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationThreshold {
    /// Threshold name
    pub name: String,
    /// Condition for escalation
    pub condition: String,
    /// Number of failures before escalation
    pub failure_count: u32,
}

/// Escalation action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationAction {
    /// Action name
    pub name: String,
    /// Action type
    pub action_type: EscalationActionType,
    /// Action parameters
    pub parameters: HashMap<String, serde_json::Value>,
}

/// Escalation action type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscalationActionType {
    /// Send notification
    Notify,
    /// Disable plugin
    DisablePlugin,
    /// Restart system
    RestartSystem,
    /// Custom action
    Custom(String),
}

/// Communication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationConfig {
    /// IPC configuration
    pub ipc: IpcConfig,
    /// Message handling configuration
    pub message_handling: MessageHandlingConfig,
    /// Security configuration
    pub security: CommunicationSecurityConfig,
}

/// IPC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// IPC transport type
    pub transport_type: IpcTransportType,
    /// Socket path for Unix domain sockets
    pub socket_path: Option<PathBuf>,
    /// Port range for TCP sockets
    pub port_range: Option<std::ops::Range<u16>>,
    /// Connection timeout
    pub connection_timeout: Duration,
    /// Message size limit
    pub max_message_size: usize,
    /// Connection pool size
    pub pool_size: u32,
}

/// IPC transport type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IpcTransportType {
    /// Unix domain socket
    UnixSocket,
    /// TCP socket
    TcpSocket,
    /// Shared memory
    SharedMemory,
    /// Named pipes
    NamedPipe,
}

/// Message handling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHandlingConfig {
    /// Default message timeout
    pub default_timeout: Duration,
    /// Maximum queue size
    pub max_queue_size: u32,
    /// Message priority handling
    pub priority_handling: bool,
    /// Message persistence
    pub persistence: MessagePersistenceConfig,
}

/// Message persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePersistenceConfig {
    /// Enable message persistence
    pub enabled: bool,
    /// Persistence storage path
    pub storage_path: Option<PathBuf>,
    /// Max persisted messages
    pub max_messages: u32,
    /// Retention period
    pub retention_period: Duration,
}

/// Communication security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationSecurityConfig {
    /// Enable encryption
    pub encryption_enabled: bool,
    /// Encryption algorithm
    pub encryption_algorithm: Option<String>,
    /// Enable authentication
    pub authentication_enabled: bool,
    /// Authentication method
    pub authentication_method: Option<AuthenticationMethod>,
    /// Certificate path
    pub certificate_path: Option<PathBuf>,
    /// Private key path
    pub private_key_path: Option<PathBuf>,
}

/// Authentication method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthenticationMethod {
    /// Certificate-based authentication
    Certificate,
    /// Token-based authentication
    Token,
    /// Shared secret authentication
    SharedSecret,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: LogLevel,
    /// Log file path
    pub file_path: Option<PathBuf>,
    /// Log format
    pub format: LogFormat,
    /// Log rotation settings
    pub rotation: LogRotationConfig,
    /// Plugin-specific logging
    pub plugin_logging: PluginLoggingConfig,
}

/// Log level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Trace level
    Trace = 0,
    /// Debug level
    Debug = 1,
    /// Info level
    Info = 2,
    /// Warn level
    Warn = 3,
    /// Error level
    Error = 4,
}

/// Log format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogFormat {
    /// Plain text format
    Plain,
    /// JSON format
    Json,
    /// Structured format
    Structured,
}

/// Log rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,
    /// Maximum file size
    pub max_file_size: u64,
    /// Maximum number of files
    pub max_files: u32,
    /// Rotation interval
    pub rotation_interval: Option<Duration>,
}

/// Plugin-specific logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLoggingConfig {
    /// Capture plugin stdout
    pub capture_stdout: bool,
    /// Capture plugin stderr
    pub capture_stderr: bool,
    /// Separate log files per plugin
    pub separate_files: bool,
    /// Plugin log directory
    pub log_directory: Option<PathBuf>,
}

/// Lifecycle configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    /// Auto-start plugins
    pub auto_start: bool,
    /// Graceful shutdown timeout
    pub shutdown_timeout: Duration,
    /// Startup order configuration
    pub startup_order: Vec<String>,
    /// Shutdown order configuration
    pub shutdown_order: Vec<String>,
    /// Concurrent startup limit
    pub concurrent_startup_limit: Option<u32>,
}

/// Performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Thread pool size
    pub thread_pool_size: u32,
    /// Async runtime configuration
    pub async_runtime: AsyncRuntimeConfig,
    /// Caching configuration
    pub caching: CachingConfig,
    /// Optimization settings
    pub optimization: OptimizationConfig,
}

/// Async runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncRuntimeConfig {
    /// Worker thread count
    pub worker_threads: Option<u32>,
    /// Max blocking threads
    pub max_blocking_threads: u32,
    /// Thread stack size
    pub thread_stack_size: Option<usize>,
}

/// Caching configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachingConfig {
    /// Enable caching
    pub enabled: bool,
    /// Cache size limit
    pub max_size: u64,
    /// TTL for cached items
    pub ttl: Duration,
    /// Cache eviction policy
    pub eviction_policy: CacheEvictionPolicy,
}

/// Cache eviction policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheEvictionPolicy {
    /// Least recently used
    LRU,
    /// Least frequently used
    LFU,
    /// First in, first out
    FIFO,
}

/// Optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Enable performance optimizations
    pub enabled: bool,
    /// Memory optimization level
    pub memory_optimization: OptimizationLevel,
    /// CPU optimization level
    pub cpu_optimization: OptimizationLevel,
    /// Network optimization level
    pub network_optimization: OptimizationLevel,
}

/// Optimization level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationLevel {
    /// No optimization
    None,
    /// Basic optimization
    Basic,
    /// Aggressive optimization
    Aggressive,
    /// Maximum optimization
    Maximum,
}

/// ============================================================================
/// DEFAULT CONFIGURATIONS
/// ============================================================================

impl Default for PluginManagerConfig {
    fn default() -> Self {
        Self {
            plugin_directories: vec![
                PathBuf::from("/opt/crucible/plugins"),
                PathBuf::from("./plugins"),
                PathBuf::from("~/.crucible/plugins"),
            ],
            auto_discovery: AutoDiscoveryConfig::default(),
            security: SecurityConfig::default(),
            resource_management: ResourceManagementConfig::default(),
            health_monitoring: HealthMonitoringConfig::default(),
            communication: CommunicationConfig::default(),
            logging: LoggingConfig::default(),
            lifecycle: LifecycleConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl Default for AutoDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scan_interval: Duration::from_secs(60),
            file_patterns: vec![
                "*.json".to_string(),     // JSON manifests
                "*.yaml".to_string(),    // YAML manifests
                "*.rn".to_string(),      // Rune scripts
                "*.rune".to_string(),    // Rune scripts
                "*.py".to_string(),      // Python scripts
                "*.js".to_string(),      // JavaScript scripts
                "*.wasm".to_string(),    // WebAssembly modules
            ],
            watch_filesystem: true,
            auto_install: false,
            validation: DiscoveryValidationConfig::default(),
        }
    }
}

impl Default for DiscoveryValidationConfig {
    fn default() -> Self {
        Self {
            validate_manifests: true,
            validate_signatures: false,
            security_scan: true,
            validate_dependencies: true,
            strict: false,
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            default_sandbox: SandboxConfig::default(),
            trusted_signatures: Vec::new(),
            policies: SecurityPolicyConfig::default(),
            audit: AuditConfig::default(),
        }
    }
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sandbox_type: SandboxType::Process,
            namespace_isolation: true,
            filesystem_isolation: true,
            network_isolation: true,
            process_isolation: true,
            resource_limits: ResourceLimits::default(),
            allowed_syscalls: vec![
                "read".to_string(),
                "write".to_string(),
                "open".to_string(),
                "close".to_string(),
                "stat".to_string(),
                "fstat".to_string(),
                "lstat".to_string(),
                "poll".to_string(),
                "lseek".to_string(),
                "mmap".to_string(),
                "mprotect".to_string(),
                "munmap".to_string(),
                "brk".to_string(),
                "rt_sigaction".to_string(),
                "rt_sigprocmask".to_string(),
                "rt_sigreturn".to_string(),
                "ioctl".to_string(),
                "pread64".to_string(),
                "pwrite64".to_string(),
                "readv".to_string(),
                "writev".to_string(),
                "access".to_string(),
                "pipe".to_string(),
                "select".to_string(),
                "sched_yield".to_string(),
                "mremap".to_string(),
                "msync".to_string(),
                "mincore".to_string(),
                "madvise".to_string(),
                "shmget".to_string(),
                "shmat".to_string(),
                "shmctl".to_string(),
                "dup".to_string(),
                "dup2".to_string(),
                "pause".to_string(),
                "nanosleep".to_string(),
                "getitimer".to_string(),
                "alarm".to_string(),
                "setitimer".to_string(),
                "getpid".to_string(),
                "sendfile".to_string(),
                "socket".to_string(),
                "connect".to_string(),
                "accept".to_string(),
                "sendto".to_string(),
                "recvfrom".to_string(),
                "sendmsg".to_string(),
                "recvmsg".to_string(),
                "shutdown".to_string(),
                "bind".to_string(),
                "listen".to_string(),
                "getsockname".to_string(),
                "getpeername".to_string(),
                "socketpair".to_string(),
                "setsockopt".to_string(),
                "getsockopt".to_string(),
                "clone".to_string(),
                "fork".to_string(),
                "vfork".to_string(),
                "execve".to_string(),
                "exit".to_string(),
                "wait4".to_string(),
                "kill".to_string(),
                "uname".to_string(),
                "semget".to_string(),
                "semop".to_string(),
                "semctl".to_string(),
                "shmdt".to_string(),
                "msgget".to_string(),
                "msgsnd".to_string(),
                "msgrcv".to_string(),
                "msgctl".to_string(),
                "fcntl".to_string(),
                "flock".to_string(),
                "fsync".to_string(),
                "fdatasync".to_string(),
                "truncate".to_string(),
                "ftruncate".to_string(),
                "getdents".to_string(),
                "getcwd".to_string(),
                "chdir".to_string(),
                "fchdir".to_string(),
                "rename".to_string(),
                "mkdir".to_string(),
                "rmdir".to_string(),
                "creat".to_string(),
                "link".to_string(),
                "unlink".to_string(),
                "symlink".to_string(),
                "readlink".to_string(),
                "chmod".to_string(),
                "fchmod".to_string(),
                "chown".to_string(),
                "fchown".to_string(),
                "lchown".to_string(),
                "umask".to_string(),
                "gettimeofday".to_string(),
                "getrlimit".to_string(),
                "getrusage".to_string(),
                "sysinfo".to_string(),
                "times".to_string(),
                "ptrace".to_string(),
                "getuid".to_string(),
                "syslog".to_string(),
                "getgid".to_string(),
                "setuid".to_string(),
                "setgid".to_string(),
                "geteuid".to_string(),
                "getegid".to_string(),
                "setpgid".to_string(),
                "getppid".to_string(),
                "getpgrp".to_string(),
                "setsid".to_string(),
                "setreuid".to_string(),
                "setregid".to_string(),
                "getgroups".to_string(),
                "setgroups".to_string(),
                "setresuid".to_string(),
                "getresuid".to_string(),
                "setresgid".to_string(),
                "getresgid".to_string(),
                "getpgid".to_string(),
                "setfsuid".to_string(),
                "setfsgid".to_string(),
                "getsid".to_string(),
                "capget".to_string(),
                "capset".to_string(),
                "rt_sigpending".to_string(),
                "rt_sigtimedwait".to_string(),
                "rt_sigqueueinfo".to_string(),
                "rt_sigsuspend".to_string(),
                "sigaltstack".to_string(),
                "utime".to_string(),
                "mknod".to_string(),
                "uselib".to_string(),
                "personality".to_string(),
                "ustat".to_string(),
                "statfs".to_string(),
                "fstatfs".to_string(),
                "sysfs".to_string(),
                "getpriority".to_string(),
                "setpriority".to_string(),
                "sched_setparam".to_string(),
                "sched_getparam".to_string(),
                "sched_setscheduler".to_string(),
                "sched_getscheduler".to_string(),
                "sched_get_priority_max".to_string(),
                "sched_get_priority_min".to_string(),
                "sched_rr_get_interval".to_string(),
                "mlock".to_string(),
                "munlock".to_string(),
                "mlockall".to_string(),
                "munlockall".to_string(),
                "vhangup".to_string(),
                "modify_ldt".to_string(),
                "pivot_root".to_string(),
                "_sysctl".to_string(),
                "prctl".to_string(),
                "arch_prctl".to_string(),
                "adjtimex".to_string(),
                "setrlimit".to_string(),
                "chroot".to_string(),
                "sync".to_string(),
                "acct".to_string(),
                "settimeofday".to_string(),
                "mount".to_string(),
                "umount2".to_string(),
                "swapon".to_string(),
                "swapoff".to_string(),
                "reboot".to_string(),
                "sethostname".to_string(),
                "setdomainname".to_string(),
                "iopl".to_string(),
                "ioperm".to_string(),
                "create_module".to_string(),
                "init_module".to_string(),
                "delete_module".to_string(),
                "get_kernel_syms".to_string(),
                "query_module".to_string(),
                "quotactl".to_string(),
                "nfsservctl".to_string(),
                "getpmsg".to_string(),
                "putpmsg".to_string(),
                "afs_syscall".to_string(),
                "tuxcall".to_string(),
                "security".to_string(),
                "gettid".to_string(),
                "readahead".to_string(),
                "setxattr".to_string(),
                "lsetxattr".to_string(),
                "fsetxattr".to_string(),
                "getxattr".to_string(),
                "lgetxattr".to_string(),
                "fgetxattr".to_string(),
                "listxattr".to_string(),
                "llistxattr".to_string(),
                "flistxattr".to_string(),
                "removexattr".to_string(),
                "lremovexattr".to_string(),
                "fremovexattr".to_string(),
                "tkill".to_string(),
                "time".to_string(),
                "futex".to_string(),
                "sched_setaffinity".to_string(),
                "sched_getaffinity".to_string(),
                "set_thread_area".to_string(),
                "io_setup".to_string(),
                "io_destroy".to_string(),
                "io_getevents".to_string(),
                "io_submit".to_string(),
                "io_cancel".to_string(),
                "get_thread_area".to_string(),
                "lookup_dcookie".to_string(),
                "epoll_create".to_string(),
                "epoll_ctl_old".to_string(),
                "epoll_wait_old".to_string(),
                "remap_file_pages".to_string(),
                "getdents64".to_string(),
                "set_tid_address".to_string(),
                "restart_syscall".to_string(),
                "semtimedop".to_string(),
                "fadvise64".to_string(),
                "timer_create".to_string(),
                "timer_settime".to_string(),
                "timer_gettime".to_string(),
                "timer_getoverrun".to_string(),
                "timer_delete".to_string(),
                "clock_settime".to_string(),
                "clock_gettime".to_string(),
                "clock_getres".to_string(),
                "clock_nanosleep".to_string(),
                "exit_group".to_string(),
                "epoll_wait".to_string(),
                "epoll_ctl".to_string(),
                "tgkill".to_string(),
                "utimes".to_string(),
                "vserver".to_string(),
                "mbind".to_string(),
                "set_mempolicy".to_string(),
                "get_mempolicy".to_string(),
                "mq_open".to_string(),
                "mq_unlink".to_string(),
                "mq_timedsend".to_string(),
                "mq_timedreceive".to_string(),
                "mq_notify".to_string(),
                "mq_getsetattr".to_string(),
                "kexec_load".to_string(),
                "waitid".to_string(),
                "add_key".to_string(),
                "request_key".to_string(),
                "keyctl".to_string(),
                "ioprio_set".to_string(),
                "ioprio_get".to_string(),
                "inotify_init".to_string(),
                "inotify_add_watch".to_string(),
                "inotify_rm_watch".to_string(),
                "migrate_pages".to_string(),
                "openat".to_string(),
                "mkdirat".to_string(),
                "mknodat".to_string(),
                "fchownat".to_string(),
                "futimesat".to_string(),
                "newfstatat".to_string(),
                "unlinkat".to_string(),
                "renameat".to_string(),
                "linkat".to_string(),
                "symlinkat".to_string(),
                "readlinkat".to_string(),
                "fchmodat".to_string(),
                "faccessat".to_string(),
                "pselect6".to_string(),
                "ppoll".to_string(),
                "unshare".to_string(),
                "set_robust_list".to_string(),
                "get_robust_list".to_string(),
                "splice".to_string(),
                "tee".to_string(),
                "sync_file_range".to_string(),
                "vmsplice".to_string(),
                "move_pages".to_string(),
                "utimensat".to_string(),
                "epoll_pwait".to_string(),
                "signalfd".to_string(),
                "timerfd_create".to_string(),
                "eventfd".to_string(),
                "fallocate".to_string(),
                "timerfd_settime".to_string(),
                "timerfd_gettime".to_string(),
                "accept4".to_string(),
                "signalfd4".to_string(),
                "eventfd2".to_string(),
                "epoll_create1".to_string(),
                "dup3".to_string(),
                "pipe2".to_string(),
                "inotify_init1".to_string(),
                "preadv".to_string(),
                "pwritev".to_string(),
                "rt_tgsigqueueinfo".to_string(),
                "perf_event_open".to_string(),
                "recvmmsg".to_string(),
                "fanotify_init".to_string(),
                "fanotify_mark".to_string(),
                "prlimit64".to_string(),
                "name_to_handle_at".to_string(),
                "open_by_handle_at".to_string(),
                "clock_adjtime".to_string(),
                "syncfs".to_string(),
                "sendmmsg".to_string(),
                "setns".to_string(),
                "getcpu".to_string(),
                "process_vm_readv".to_string(),
                "process_vm_writev".to_string(),
                "kcmp".to_string(),
                "finit_module".to_string(),
                "sched_setattr".to_string(),
                "sched_getattr".to_string(),
                "renameat2".to_string(),
                "seccomp".to_string(),
                "getrandom".to_string(),
                "memfd_create".to_string(),
                "kexec_file_load".to_string(),
                "bpf".to_string(),
                "execveat".to_string(),
                "userfaultfd".to_string(),
                "membarrier".to_string(),
                "mlock2".to_string(),
                "copy_file_range".to_string(),
                "preadv2".to_string(),
                "pwritev2".to_string(),
                "pkey_mprotect".to_string(),
                "pkey_alloc".to_string(),
                "pkey_free".to_string(),
                "statx".to_string(),
                "io_pgetevents".to_string(),
                "rseq".to_string(),
            ].into_iter().collect(),
            blocked_syscalls: vec![
                "ptrace".to_string(),
                "process_vm_readv".to_string(),
                "process_vm_writev".to_string(),
                "kcmp".to_string(),
            ],
            mount_points: vec![
                MountPoint {
                    source: PathBuf::from("/proc"),
                    target: PathBuf::from("/proc"),
                    mount_type: MountType::Proc,
                    read_only: true,
                    options: vec![],
                },
                MountPoint {
                    source: PathBuf::from("/dev"),
                    target: PathBuf::from("/dev"),
                    mount_type: MountType::Devpts,
                    read_only: false,
                    options: vec!["noexec".to_string(), "nosuid".to_string()],
                },
            ],
            environment: HashMap::new(),
        }
    }
}

impl Default for SecurityPolicyConfig {
    fn default() -> Self {
        let mut level_configs = HashMap::new();

        // Basic security level
        level_configs.insert(
            SecurityLevel::Basic,
            SecurityLevelConfig {
                name: "Basic".to_string(),
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
                    max_cpu_percentage: Some(50.0),
                    max_concurrent_operations: Some(10),
                    operation_timeout: Some(Duration::from_secs(30)),
                    ..Default::default()
                },
                allowed_capabilities: vec![
                    PluginCapability::FileSystem {
                        read_paths: vec!["/tmp".to_string(), "/var/tmp".to_string()],
                        write_paths: vec!["/tmp".to_string()],
                    },
                    PluginCapability::IpcCommunication,
                ],
                blocked_capabilities: vec![
                    PluginCapability::SystemCalls {
                        allowed_calls: vec![],
                    },
                ],
                sandbox_config: SandboxConfig::default(),
                time_limits: TimeLimits {
                    max_execution_time: Some(Duration::from_secs(300)),
                    max_idle_time: Some(Duration::from_secs(600)),
                    max_total_runtime: Some(Duration::from_secs(3600)),
                },
            },
        );

        // Strict security level
        level_configs.insert(
            SecurityLevel::Strict,
            SecurityLevelConfig {
                name: "Strict".to_string(),
                resource_limits: ResourceLimits {
                    max_memory_bytes: Some(256 * 1024 * 1024), // 256MB
                    max_cpu_percentage: Some(25.0),
                    max_concurrent_operations: Some(5),
                    operation_timeout: Some(Duration::from_secs(15)),
                    ..Default::default()
                },
                allowed_capabilities: vec![PluginCapability::IpcCommunication],
                blocked_capabilities: vec![
                    PluginCapability::FileSystem {
                        read_paths: vec![],
                        write_paths: vec![],
                    },
                    PluginCapability::Network {
                        allowed_hosts: vec![],
                        allowed_ports: vec![],
                    },
                    PluginCapability::SystemCalls {
                        allowed_calls: vec![],
                    },
                ],
                sandbox_config: SandboxConfig::default(),
                time_limits: TimeLimits {
                    max_execution_time: Some(Duration::from_secs(60)),
                    max_idle_time: Some(Duration::from_secs(120)),
                    max_total_runtime: Some(Duration::from_secs(1800)),
                },
            },
        );

        Self {
            default_level: SecurityLevel::Basic,
            level_configs,
            custom_rules: vec![],
        }
    }
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_file: Some(PathBuf::from("/var/log/crucible/plugin-audit.log")),
            audit_events: vec![
                AuditEventType::PluginInstall,
                AuditEventType::PluginUninstall,
                AuditEventType::PluginStart,
                AuditEventType::PluginStop,
                AuditEventType::SecurityViolation,
                AuditEventType::ResourceLimitExceeded,
                AuditEventType::ConfigChange,
                AuditEventType::AccessDenied,
                AuditEventType::Error,
            ],
            retention_period: Some(Duration::from_secs(30 * 24 * 60 * 60)), // 30 days
            real_time_monitoring: true,
            alert_thresholds: AlertThresholds {
                errors_per_minute: Some(10),
                memory_usage_percent: Some(90.0),
                cpu_usage_percent: Some(80.0),
                failed_login_attempts: Some(5),
            },
        }
    }
}

impl Default for ResourceManagementConfig {
    fn default() -> Self {
        Self {
            global_limits: ResourceLimits {
                max_memory_bytes: Some(8 * 1024 * 1024 * 1024), // 8GB
                max_cpu_percentage: Some(80.0),
                max_concurrent_operations: Some(100),
                ..Default::default()
            },
            per_plugin_limits: ResourceLimits {
                max_memory_bytes: Some(512 * 1024 * 1024), // 512MB
                max_cpu_percentage: Some(25.0),
                max_concurrent_operations: Some(10),
                operation_timeout: Some(Duration::from_secs(60)),
                ..Default::default()
            },
            monitoring: ResourceMonitoringConfig {
                enabled: true,
                interval: Duration::from_secs(5),
                metrics: vec![
                    ResourceMetric::CpuUsage,
                    ResourceMetric::MemoryUsage,
                    ResourceMetric::DiskUsage,
                    ResourceMetric::FileDescriptors,
                    ResourceMetric::ProcessCount,
                ],
                retention_period: Duration::from_secs(60 * 60 * 24), // 24 hours
            },
            enforcement: ResourceEnforcementConfig {
                enabled: true,
                strategy: EnforcementStrategy::Soft,
                grace_period: Duration::from_secs(30),
                limit_exceeded_action: LimitExceededAction::Throttle,
            },
        }
    }
}

impl Default for HealthMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(10),
            check_timeout: Duration::from_secs(5),
            strategies: vec![
                HealthCheckStrategy {
                    name: "process".to_string(),
                    strategy_type: HealthCheckType::Process,
                    config: HashMap::new(),
                    enabled: true,
                },
                HealthCheckStrategy {
                    name: "resource".to_string(),
                    strategy_type: HealthCheckType::Resource,
                    config: HashMap::new(),
                    enabled: true,
                },
            ],
            unhealthy_threshold: 3,
            recovery: RecoveryConfig {
                enabled: true,
                max_restart_attempts: 3,
                restart_delay: Duration::from_secs(5),
                backoff_strategy: BackoffStrategy::Exponential,
                escalation: EscalationConfig {
                    enabled: true,
                    thresholds: vec![
                        EscalationThreshold {
                            name: "multiple_failures".to_string(),
                            condition: "restart_count >= 3".to_string(),
                            failure_count: 3,
                        },
                    ],
                    actions: vec![
                        EscalationAction {
                            name: "disable_plugin".to_string(),
                            action_type: EscalationActionType::DisablePlugin,
                            parameters: HashMap::new(),
                        },
                    ],
                },
            },
        }
    }
}

impl Default for CommunicationConfig {
    fn default() -> Self {
        Self {
            ipc: IpcConfig {
                transport_type: IpcTransportType::UnixSocket,
                socket_path: Some(PathBuf::from("/tmp/crucible-plugins.sock")),
                port_range: Some(9000..10000),
                connection_timeout: Duration::from_secs(5),
                max_message_size: 16 * 1024 * 1024, // 16MB
                pool_size: 10,
            },
            message_handling: MessageHandlingConfig {
                default_timeout: Duration::from_secs(30),
                max_queue_size: 1000,
                priority_handling: true,
                persistence: MessagePersistenceConfig {
                    enabled: false,
                    storage_path: Some(PathBuf::from("/var/lib/crucible/plugin-ipc")),
                    max_messages: 10000,
                    retention_period: Duration::from_secs(60 * 60), // 1 hour
                },
            },
            security: CommunicationSecurityConfig {
                encryption_enabled: false,
                encryption_algorithm: None,
                authentication_enabled: true,
                authentication_method: Some(AuthenticationMethod::Token),
                certificate_path: None,
                private_key_path: None,
            },
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            file_path: Some(PathBuf::from("/var/log/crucible/plugin-manager.log")),
            format: LogFormat::Json,
            rotation: LogRotationConfig {
                enabled: true,
                max_file_size: 100 * 1024 * 1024, // 100MB
                max_files: 10,
                rotation_interval: Some(Duration::from_secs(60 * 60 * 24)), // Daily
            },
            plugin_logging: PluginLoggingConfig {
                capture_stdout: true,
                capture_stderr: true,
                separate_files: true,
                log_directory: Some(PathBuf::from("/var/log/crucible/plugins")),
            },
        }
    }
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            shutdown_timeout: Duration::from_secs(30),
            startup_order: vec![],
            shutdown_order: vec![],
            concurrent_startup_limit: Some(5),
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            thread_pool_size: get_num_cpus() as u32,
            async_runtime: AsyncRuntimeConfig {
                worker_threads: None,
                max_blocking_threads: 512,
                thread_stack_size: Some(2 * 1024 * 1024), // 2MB
            },
            caching: CachingConfig {
                enabled: true,
                max_size: 1024 * 1024 * 1024, // 1GB
                ttl: Duration::from_secs(60 * 60), // 1 hour
                eviction_policy: CacheEvictionPolicy::LRU,
            },
            optimization: OptimizationConfig {
                enabled: true,
                memory_optimization: OptimizationLevel::Basic,
                cpu_optimization: OptimizationLevel::Basic,
                network_optimization: OptimizationLevel::Basic,
            },
        }
    }
}

/// ============================================================================
/// CONFIGURATION VALIDATION
/// ============================================================================

impl PluginManagerConfig {
    /// Validate the configuration
    pub fn validate(&self) -> PluginResult<()> {
        // Validate plugin directories
        if self.plugin_directories.is_empty() {
            return Err(PluginError::configuration(
                "At least one plugin directory must be specified".to_string(),
            ));
        }

        // Validate auto-discovery settings
        if self.auto_discovery.scan_interval < Duration::from_secs(1) {
            return Err(PluginError::configuration(
                "Auto-discovery scan interval must be at least 1 second".to_string(),
            ));
        }

        // Validate resource limits
        if let Some(global_mem) = self.resource_management.global_limits.max_memory_bytes {
            if let Some(per_plugin_mem) = self.resource_management.per_plugin_limits.max_memory_bytes {
                if per_plugin_mem > global_mem {
                    return Err(PluginError::configuration(
                        "Per-plugin memory limit cannot exceed global limit".to_string(),
                    ));
                }
            }
        }

        // Validate health monitoring settings
        if self.health_monitoring.check_interval < Duration::from_secs(1) {
            return Err(PluginError::configuration(
                "Health check interval must be at least 1 second".to_string(),
            ));
        }

        if self.health_monitoring.check_timeout >= self.health_monitoring.check_interval {
            return Err(PluginError::configuration(
                "Health check timeout must be less than check interval".to_string(),
            ));
        }

        // Validate communication settings
        if self.communication.ipc.max_message_size == 0 {
            return Err(PluginError::configuration(
                "Max message size must be greater than 0".to_string(),
            ));
        }

        // Validate performance settings
        if self.performance.thread_pool_size == 0 {
            return Err(PluginError::configuration(
                "Thread pool size must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }

    /// Load configuration from file
    pub async fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> PluginResult<Self> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| PluginError::configuration(format!("Failed to read config file: {}", e)))?;

        let config: Self = serde_json::from_str(&content)
            .map_err(|e| PluginError::configuration(format!("Failed to parse config file: {}", e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Save configuration to file
    pub async fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> PluginResult<()> {
        self.validate()?;

        let content = serde_json::to_string_pretty(self)
            .map_err(|e| PluginError::configuration(format!("Failed to serialize config: {}", e)))?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| PluginError::configuration(format!("Failed to write config file: {}", e)))?;

        Ok(())
    }

    /// Get configuration summary
    pub fn get_summary(&self) -> HashMap<String, serde_json::Value> {
        let mut summary = HashMap::new();

        summary.insert("plugin_directories".to_string(), serde_json::Value::Array(
            self.plugin_directories.iter()
                .map(|p| serde_json::Value::String(p.to_string_lossy().to_string()))
                .collect()
        ));

        summary.insert("auto_discovery_enabled".to_string(),
            serde_json::Value::Bool(self.auto_discovery.enabled));

        summary.insert("security_enabled".to_string(),
            serde_json::Value::Bool(self.security.default_sandbox.enabled));

        summary.insert("health_monitoring_enabled".to_string(),
            serde_json::Value::Bool(self.health_monitoring.enabled));

        summary.insert("thread_pool_size".to_string(),
            serde_json::Value::Number(self.performance.thread_pool_size.into()));

        summary
    }
}

/// Get the number of available CPUs
fn get_num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4) // Fallback to 4 if detection fails
}