//! # IPC Error Types
//!
//! Comprehensive error handling for the plugin IPC protocol with detailed error
//! categorization and recovery strategies.

use thiserror::Error;
use std::time::Duration;

/// Comprehensive error type for IPC operations
#[derive(Error, Debug, Clone)]
pub enum IpcError {
    /// Protocol-level errors
    #[error("Protocol error: {message}")]
    Protocol {
        message: String,
        code: ProtocolErrorCode,
        #[source]
        source: Option<Box<IpcError>>,
    },

    /// Authentication and authorization errors
    #[error("Authentication error: {message}")]
    Authentication {
        message: String,
        code: AuthErrorCode,
        retry_after: Option<Duration>,
    },

    /// Connection and transport errors
    #[error("Connection error: {message}")]
    Connection {
        message: String,
        code: ConnectionErrorCode,
        endpoint: String,
        retry_count: u32,
    },

    /// Message processing errors
    #[error("Message error: {message}")]
    Message {
        message: String,
        code: MessageErrorCode,
        message_id: Option<String>,
    },

    /// Plugin execution errors
    #[error("Plugin error: {message}")]
    Plugin {
        message: String,
        code: PluginErrorCode,
        plugin_id: String,
        execution_id: Option<String>,
    },

    /// Resource and quota errors
    #[error("Resource error: {message}")]
    Resource {
        message: String,
        code: ResourceErrorCode,
        resource_type: String,
        current_usage: Option<u64>,
        limit: Option<u64>,
    },

    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration {
        message: String,
        code: ConfigErrorCode,
        config_key: Option<String>,
    },

    /// System and environmental errors
    #[error("System error: {message}")]
    System {
        message: String,
        code: SystemErrorCode,
        #[source]
        source: Option<Box<IpcError>>,
    },

    /// Timeout errors
    #[error("Timeout error: {message}")]
    Timeout {
        message: String,
        operation: String,
        timeout: Duration,
        elapsed: Duration,
    },

    /// Validation errors
    #[error("Validation error: {message}")]
    Validation {
        message: String,
        field: Option<String>,
        value: Option<String>,
        constraint: Option<String>,
    },
}

/// Protocol-specific error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ProtocolErrorCode {
    #[error("Version mismatch")]
    VersionMismatch,
    #[error("Unsupported message type")]
    UnsupportedMessageType,
    #[error("Invalid message format")]
    InvalidMessageFormat,
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    #[error("Serialization failed")]
    SerializationFailed,
    #[error("Deserialization failed")]
    DeserializationFailed,
    #[error("Message too large")]
    MessageTooLarge,
    #[error("Invalid header")]
    InvalidHeader,
    #[error("Protocol violation")]
    ProtocolViolation,
}

/// Authentication error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AuthErrorCode {
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token expired")]
    TokenExpired,
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("Account locked")]
    AccountLocked,
    #[error("Certificate invalid")]
    CertificateInvalid,
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    #[error("Challenge failed")]
    ChallengeFailed,
    #[error("Rate limited")]
    RateLimited,
    #[error("Unknown principal")]
    UnknownPrincipal,
}

/// Connection error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ConnectionErrorCode {
    #[error("Connection refused")]
    ConnectionRefused,
    #[error("Connection timed out")]
    ConnectionTimedOut,
    #[error("Connection reset")]
    ConnectionReset,
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Endpoint not found")]
    EndpointNotFound,
    #[error("Network unreachable")]
    NetworkUnreachable,
    #[error("Address in use")]
    AddressInUse,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Transport error")]
    TransportError,
    #[error("Max connections exceeded")]
    MaxConnectionsExceeded,
}

/// Message processing error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum MessageErrorCode {
    #[error("Message not found")]
    MessageNotFound,
    #[error("Duplicate message")]
    DuplicateMessage,
    #[error("Message expired")]
    MessageExpired,
    #[error("Invalid message ID")]
    InvalidMessageId,
    #[error("Payload too large")]
    PayloadTooLarge,
    #[error("Invalid encoding")]
    InvalidEncoding,
    #[error("Compression failed")]
    CompressionFailed,
    #[error("Decompression failed")]
    DecompressionFailed,
    #[error("Encryption failed")]
    EncryptionFailed,
    #[error("Decryption failed")]
    DecryptionFailed,
}

/// Plugin execution error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PluginErrorCode {
    #[error("Plugin not found")]
    PluginNotFound,
    #[error("Plugin not running")]
    PluginNotRunning,
    #[error("Plugin crashed")]
    PluginCrashed,
    #[error("Plugin timeout")]
    PluginTimeout,
    #[error("Invalid plugin state")]
    InvalidPluginState,
    #[error("Plugin initialization failed")]
    InitializationFailed,
    #[error("Plugin execution failed")]
    ExecutionFailed,
    #[error("Plugin panicked")]
    PluginPanicked,
    #[error("Resource exhausted")]
    ResourceExhausted,
    #[error("Capability denied")]
    CapabilityDenied,
}

/// Resource error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ResourceErrorCode {
    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,
    #[error("CPU limit exceeded")]
    CpuLimitExceeded,
    #[error("Disk limit exceeded")]
    DiskLimitExceeded,
    #[error("Network limit exceeded")]
    NetworkLimitExceeded,
    #[error("File limit exceeded")]
    FileLimitExceeded,
    #[error("Process limit exceeded")]
    ProcessLimitExceeded,
    #[error("Resource not available")]
    ResourceNotAvailable,
    #[error("Quota exceeded")]
    QuotaExceeded,
    #[error("Resource busy")]
    ResourceBusy,
    #[error("Invalid resource request")]
    InvalidResourceRequest,
}

/// Configuration error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ConfigErrorCode {
    #[error("Invalid configuration")]
    InvalidConfiguration,
    #[error("Missing configuration")]
    MissingConfiguration,
    #[error("Configuration parse error")]
    ParseError,
    #[error("Configuration validation failed")]
    ValidationFailed,
    #[error("Configuration access denied")]
    AccessDenied,
    #[error("Configuration version mismatch")]
    VersionMismatch,
    #[error("Immutable configuration")]
    ImmutableConfiguration,
    #[error("Configuration conflict")]
    Conflict,
    #[error("Configuration corrupted")]
    Corrupted,
    #[error("Configuration not found")]
    NotFound,
}

/// System error codes
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum SystemErrorCode {
    #[error("IO error")]
    IoError,
    #[error("System call failed")]
    SystemCallFailed,
    #[error("Permission denied")]
    PermissionDenied,
    #[error("Resource temporarily unavailable")]
    ResourceUnavailable,
    #[error("Operation not permitted")]
    OperationNotPermitted,
    #[error("Filesystem error")]
    FilesystemError,
    #[error("Network error")]
    NetworkError,
    #[error("OS error")]
    OsError,
    #[error("Internal error")]
    InternalError,
    #[error("Unknown error")]
    Unknown,
}

impl IpcError {
    /// Get the error code as a string for logging/monitoring
    pub fn error_code(&self) -> String {
        match self {
            IpcError::Protocol { code, .. } => format!("protocol:{}", code),
            IpcError::Authentication { code, .. } => format!("auth:{}", code),
            IpcError::Connection { code, .. } => format!("connection:{}", code),
            IpcError::Message { code, .. } => format!("message:{}", code),
            IpcError::Plugin { code, .. } => format!("plugin:{}", code),
            IpcError::Resource { code, .. } => format!("resource:{}", code),
            IpcError::Configuration { code, .. } => format!("config:{}", code),
            IpcError::System { code, .. } => format!("system:{}", code),
            IpcError::Timeout { .. } => "timeout".to_string(),
            IpcError::Validation { .. } => "validation".to_string(),
        }
    }

    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            IpcError::Protocol { code, .. } => matches!(
                code,
                ProtocolErrorCode::ConnectionTimedOut |
                ProtocolErrorCode::TransportError
            ),
            IpcError::Connection { code, .. } => matches!(
                code,
                ConnectionErrorCode::ConnectionTimedOut |
                ConnectionErrorCode::ConnectionRefused |
                ConnectionErrorCode::NetworkUnreachable |
                ConnectionErrorCode::TransportError
            ),
            IpcError::Timeout { .. } => true,
            IpcError::Resource { code, .. } => matches!(
                code,
                ResourceErrorCode::ResourceNotAvailable |
                ResourceErrorCode::ResourceBusy
            ),
            IpcError::System { code, .. } => matches!(
                code,
                SystemErrorCode::ResourceUnavailable |
                SystemErrorCode::NetworkError
            ),
            _ => false,
        }
    }

    /// Get suggested retry delay for retryable errors
    pub fn retry_delay(&self) -> Option<Duration> {
        if !self.is_retryable() {
            return None;
        }

        let base_delay = match self {
            IpcError::Connection { retry_count, .. } => {
                Duration::from_millis(100 * (1 << retry_count.min(6)))
            }
            IpcError::Authentication { retry_after, .. } => return *retry_after,
            IpcError::Timeout { .. } => Duration::from_millis(1000),
            _ => Duration::from_millis(500),
        };

        // Add jitter to prevent thundering herd
        let jitter = fastrand::u64(0..=base_delay.as_millis() as u64 / 4);
        Some(base_delay + Duration::from_millis(jitter))
    }

    /// Check if the error indicates a connection should be closed
    pub fn should_close_connection(&self) -> bool {
        matches!(
            self,
            IpcError::Protocol { .. } |
            IpcError::Authentication { .. } |
            IpcError::Connection { code: ConnectionErrorCode::ConnectionReset | ConnectionErrorCode::ConnectionClosed, .. } |
            IpcError::Message { code: MessageErrorCode::ProtocolViolation, .. } |
            IpcError::Plugin { code: PluginErrorCode::PluginCrashed | PluginErrorCode::PluginPanicked, .. }
        )
    }

    /// Get the severity level for logging
    pub fn severity(&self) -> log::Level {
        match self {
            IpcError::Protocol { .. } |
            IpcError::Authentication { .. } |
            IpcError::System { .. } => log::Level::Error,
            IpcError::Connection { .. } |
            IpcError::Plugin { code: PluginErrorCode::PluginCrashed | PluginErrorCode::PluginPanicked, .. } |
            IpcError::Resource { code: ResourceErrorCode::MemoryLimitExceeded | ResourceErrorCode::CpuLimitExceeded, .. } => log::Level::Warn,
            IpcError::Message { .. } |
            IpcError::Plugin { .. } |
            IpcError::Timeout { .. } |
            IpcError::Validation { .. } => log::Level::Info,
            IpcError::Configuration { .. } => log::Level::Debug,
        }
    }

    /// Convert to a standardized error response
    pub fn to_error_response(&self) -> ErrorResponse {
        ErrorResponse {
            error_code: self.error_code(),
            message: self.to_string(),
            retryable: self.is_retryable(),
            retry_after: self.retry_delay(),
            should_close: self.should_close_connection(),
            severity: match self.severity() {
                log::Level::Error => "error".to_string(),
                log::Level::Warn => "warning".to_string(),
                log::Level::Info => "info".to_string(),
                log::Level::Debug => "debug".to_string(),
                log::Level::Trace => "trace".to_string(),
            },
        }
    }
}

/// Standardized error response format
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorResponse {
    pub error_code: String,
    pub message: String,
    pub retryable: bool,
    pub retry_after: Option<Duration>,
    pub should_close: bool,
    pub severity: String,
}

impl std::fmt::Display for ProtocolErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for AuthErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for ConnectionErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for MessageErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for PluginErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for ResourceErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for ConfigErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for SystemErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

/// Result type alias for IPC operations
pub type IpcResult<T> = Result<T, IpcError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_generation() {
        let error = IpcError::Connection {
            message: "Connection failed".to_string(),
            code: ConnectionErrorCode::ConnectionRefused,
            endpoint: "localhost:8080".to_string(),
            retry_count: 0,
        };

        assert_eq!(error.error_code(), "connection:ConnectionRefused");
    }

    #[test]
    fn test_retryable_errors() {
        let retryable = IpcError::Connection {
            message: "Timeout".to_string(),
            code: ConnectionErrorCode::ConnectionTimedOut,
            endpoint: "localhost:8080".to_string(),
            retry_count: 0,
        };

        assert!(retryable.is_retryable());

        let non_retryable = IpcError::Authentication {
            message: "Invalid token".to_string(),
            code: AuthErrorCode::InvalidToken,
            retry_after: None,
        };

        assert!(!non_retryable.is_retryable());
    }

    #[test]
    fn test_retry_delay_calculation() {
        let error = IpcError::Connection {
            message: "Retry test".to_string(),
            code: ConnectionErrorCode::ConnectionRefused,
            endpoint: "localhost:8080".to_string(),
            retry_count: 2,
        };

        let delay = error.retry_delay().unwrap();
        // Should be 100ms * 2^2 = 400ms with some jitter
        assert!(delay >= Duration::from_millis(400));
        assert!(delay <= Duration::from_millis(500)); // 400ms + up to 100ms jitter
    }

    #[test]
    fn test_connection_close_detection() {
        let should_close = IpcError::Protocol {
            message: "Protocol violation".to_string(),
            code: ProtocolErrorCode::ProtocolViolation,
            source: None,
        };

        assert!(should_close.should_close_connection());

        let should_not_close = IpcError::Timeout {
            message: "Request timeout".to_string(),
            operation: "test".to_string(),
            timeout: Duration::from_secs(30),
            elapsed: Duration::from_secs(30),
        };

        assert!(!should_not_close.should_close_connection());
    }

    #[test]
    fn test_error_response_conversion() {
        let error = IpcError::Validation {
            message: "Invalid field".to_string(),
            field: Some("email".to_string()),
            value: Some("invalid".to_string()),
            constraint: Some("must be valid email".to_string()),
        };

        let response = error.to_error_response();
        assert_eq!(response.error_code, "validation");
        assert!(!response.retryable);
        assert!(!response.should_close);
        assert_eq!(response.severity, "info");
    }

    #[test]
    fn test_error_severity_levels() {
        let protocol_error = IpcError::Protocol {
            message: "Version mismatch".to_string(),
            code: ProtocolErrorCode::VersionMismatch,
            source: None,
        };

        assert_eq!(protocol_error.severity(), log::Level::Error);

        let timeout_error = IpcError::Timeout {
            message: "Request timeout".to_string(),
            operation: "test".to_string(),
            timeout: Duration::from_secs(30),
            elapsed: Duration::from_secs(30),
        };

        assert_eq!(timeout_error.severity(), log::Level::Info);
    }
}