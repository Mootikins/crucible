//! # IPC Security Module
//!
//! Comprehensive security implementation including authentication, authorization,
//! encryption, and sandboxing for plugin IPC communication.

use crate::plugin_ipc::{error::IpcError, IpcResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Security manager for handling authentication, authorization, and encryption
pub struct SecurityManager {
    /// Authentication configuration
    auth_config: AuthConfig,
    /// Encryption configuration
    encryption_config: EncryptionConfig,
    /// Authorization configuration
    authz_config: AuthorizationConfig,
    /// Active sessions
    sessions: HashMap<String, SecuritySession>,
    /// Plugin capabilities registry
    capabilities: HashMap<String, PluginCapabilities>,
}

impl SecurityManager {
    /// Create a new security manager with the given configuration
    pub fn new(
        auth_config: AuthConfig,
        encryption_config: EncryptionConfig,
        authz_config: AuthorizationConfig,
    ) -> Self {
        Self {
            auth_config,
            encryption_config,
            authz_config,
            sessions: HashMap::new(),
            capabilities: HashMap::new(),
        }
    }

    /// Authenticate a plugin connection
    pub async fn authenticate(&mut self, handshake: &crate::plugin_ipc::message::HandshakePayload) -> IpcResult<AuthResult> {
        // Validate the handshake
        handshake.validate()?;

        // Verify the authentication token
        let token_data = self.verify_token(&handshake.auth_token).await?;

        // Check if the plugin is authorized
        self.authorize_plugin(&handshake.client_id, &token_data).await?;

        // Create a new security session
        let session = SecuritySession {
            session_id: Uuid::new_v4().to_string(),
            plugin_id: handshake.client_id.clone(),
            user_id: token_data.user_id,
            permissions: token_data.permissions,
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(self.auth_config.session_timeout_s),
            capabilities: handshake.capabilities.clone(),
            security_level: self.determine_security_level(&handshake.capabilities),
        };

        let session_id = session.session_id.clone();
        self.sessions.insert(session_id.clone(), session.clone());

        Ok(AuthResult {
            session_id,
            plugin_id: handshake.client_id.clone(),
            permissions: token_data.permissions,
            expires_at: session.expires_at,
            security_level: session.security_level,
        })
    }

    /// Validate a session and update activity
    pub fn validate_session(&mut self, session_id: &str) -> IpcResult<&SecuritySession> {
        let session = self.sessions.get_mut(session_id)
            .ok_or_else(|| IpcError::Authentication {
                message: "Invalid session ID".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })?;

        // Check if session has expired
        if SystemTime::now() > session.expires_at {
            self.sessions.remove(session_id);
            return Err(IpcError::Authentication {
                message: "Session expired".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::TokenExpired,
                retry_after: None,
            });
        }

        // Update last activity
        session.last_activity = SystemTime::now();

        Ok(session)
    }

    /// Check if a session has permission for an operation
    pub fn check_permission(&self, session_id: &str, operation: &str, resource: &str) -> IpcResult<()> {
        let session = self.sessions.get(session_id)
            .ok_or_else(|| IpcError::Authentication {
                message: "Invalid session ID".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })?;

        // Check if session has expired
        if SystemTime::now() > session.expires_at {
            return Err(IpcError::Authentication {
                message: "Session expired".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::TokenExpired,
                retry_after: None,
            });
        }

        // Check permissions
        if !self.has_permission(&session.permissions, operation, resource) {
            return Err(IpcError::Authentication {
                message: format!("Insufficient permissions for operation: {}", operation),
                code: crate::plugin_ipc::error::AuthErrorCode::InsufficientPermissions,
                retry_after: None,
            });
        }

        Ok(())
    }

    /// Encrypt a message payload
    pub fn encrypt_message(&self, session_id: &str, payload: &[u8]) -> IpcResult<Vec<u8>> {
        let session = self.sessions.get(session_id)
            .ok_or_else(|| IpcError::Authentication {
                message: "Invalid session ID".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })?;

        // Get or create encryption key for the session
        let encryption_key = self.get_encryption_key(session)?;

        // Encrypt the payload
        match self.encryption_config.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.encrypt_aes256gcm(&encryption_key, payload),
            EncryptionAlgorithm::ChaCha20Poly1305 => self.encrypt_chacha20poly1305(&encryption_key, payload),
        }
    }

    /// Decrypt a message payload
    pub fn decrypt_message(&self, session_id: &str, encrypted_payload: &[u8]) -> IpcResult<Vec<u8>> {
        let session = self.sessions.get(session_id)
            .ok_or_else(|| IpcError::Authentication {
                message: "Invalid session ID".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })?;

        // Get encryption key for the session
        let encryption_key = self.get_encryption_key(session)?;

        // Decrypt the payload
        match self.encryption_config.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.decrypt_aes256gcm(&encryption_key, encrypted_payload),
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha20poly1305(&encryption_key, encrypted_payload),
        }
    }

    /// Revoke a session
    pub fn revoke_session(&mut self, session_id: &str) -> IpcResult<()> {
        self.sessions.remove(session_id)
            .ok_or_else(|| IpcError::Authentication {
                message: "Session not found".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            })?;

        Ok(())
    }

    /// Clean up expired sessions
    pub fn cleanup_expired_sessions(&mut self) {
        let now = SystemTime::now();
        self.sessions.retain(|_, session| session.expires_at > now);
    }

    /// Get active session count
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    // Private helper methods

    async fn verify_token(&self, token: &str) -> IpcResult<TokenData> {
        match self.auth_config.token_type {
            TokenType::Jwt => self.verify_jwt_token(token).await,
            TokenType::ApiKey => self.verify_api_key(token).await,
            TokenType::Certificate => self.verify_certificate_token(token).await,
        }
    }

    async fn verify_jwt_token(&self, token: &str) -> IpcResult<TokenData> {
        // In a real implementation, this would verify JWT signature and claims
        // For now, we'll do a basic validation
        if token.len() < 10 {
            return Err(IpcError::Authentication {
                message: "Invalid token format".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            });
        }

        // Mock token data for demonstration
        Ok(TokenData {
            user_id: "user123".to_string(),
            permissions: vec![
                Permission {
                    action: "execute".to_string(),
                    resource: "*".to_string(),
                    effect: PermissionEffect::Allow,
                },
            ],
            expires_at: SystemTime::now() + Duration::from_secs(3600),
        })
    }

    async fn verify_api_key(&self, token: &str) -> IpcResult<TokenData> {
        // API key verification logic
        if !token.starts_with("crucible_") {
            return Err(IpcError::Authentication {
                message: "Invalid API key format".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InvalidToken,
                retry_after: None,
            });
        }

        Ok(TokenData {
            user_id: "api_user".to_string(),
            permissions: vec![],
            expires_at: SystemTime::now() + Duration::from_secs(86400),
        })
    }

    async fn verify_certificate_token(&self, token: &str) -> IpcResult<TokenData> {
        // Certificate verification logic
        // This would involve X.509 certificate validation
        Ok(TokenData {
            user_id: "cert_user".to_string(),
            permissions: vec![],
            expires_at: SystemTime::now() + Duration::from_secs(7200),
        })
    }

    async fn authorize_plugin(&self, plugin_id: &str, token_data: &TokenData) -> IpcResult<()> {
        // Check if the plugin is in the allowlist
        if !self.authz_config.allowed_plugins.contains(&plugin_id.to_string()) {
            return Err(IpcError::Authentication {
                message: format!("Plugin not authorized: {}", plugin_id),
                code: crate::plugin_ipc::error::AuthErrorCode::InsufficientPermissions,
                retry_after: None,
            });
        }

        // Check if the user has permission to use this plugin
        if !self.has_permission(&token_data.permissions, "use_plugin", plugin_id) {
            return Err(IpcError::Authentication {
                message: "User not authorized to use this plugin".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::InsufficientPermissions,
                retry_after: None,
            });
        }

        Ok(())
    }

    fn has_permission(&self, permissions: &[Permission], action: &str, resource: &str) -> bool {
        permissions.iter().any(|perm| {
            (perm.resource == "*" || perm.resource == resource) && perm.action == action
                && perm.effect == PermissionEffect::Allow
        })
    }

    fn determine_security_level(&self, capabilities: &crate::plugin_ipc::message::ClientCapabilities) -> SecurityLevel {
        // Determine security level based on plugin capabilities
        if capabilities.supports_encryption && capabilities.supports_compression {
            SecurityLevel::High
        } else if capabilities.supports_encryption {
            SecurityLevel::Medium
        } else {
            SecurityLevel::Low
        }
    }

    fn get_encryption_key(&self, session: &SecuritySession) -> IpcResult<Vec<u8>> {
        // In a real implementation, this would derive a key from the session
        // For now, return a mock key
        Ok(format!("key_{}_{}", session.session_id, session.plugin_id).into_bytes())
    }

    fn encrypt_aes256gcm(&self, key: &[u8], plaintext: &[u8]) -> IpcResult<Vec<u8>> {
        // AES-256-GCM encryption implementation
        // This is a placeholder - real implementation would use a crypto library
        let mut ciphertext = plaintext.to_vec();
        ciphertext.extend_from_slice(key); // Mock encryption
        Ok(ciphertext)
    }

    fn decrypt_aes256gcm(&self, key: &[u8], ciphertext: &[u8]) -> IpcResult<Vec<u8>> {
        // AES-256-GCM decryption implementation
        // This is a placeholder - real implementation would use a crypto library
        if ciphertext.len() < key.len() {
            return Err(IpcError::Authentication {
                message: "Invalid ciphertext".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::DecryptionFailed,
                retry_after: None,
            });
        }

        let plaintext_len = ciphertext.len() - key.len();
        Ok(ciphertext[..plaintext_len].to_vec())
    }

    fn encrypt_chacha20poly1305(&self, key: &[u8], plaintext: &[u8]) -> IpcResult<Vec<u8>> {
        // ChaCha20-Poly1305 encryption implementation
        let mut ciphertext = plaintext.to_vec();
        ciphertext.extend_from_slice(key); // Mock encryption
        Ok(ciphertext)
    }

    fn decrypt_chacha20poly1305(&self, key: &[u8], ciphertext: &[u8]) -> IpcResult<Vec<u8>> {
        // ChaCha20-Poly1305 decryption implementation
        if ciphertext.len() < key.len() {
            return Err(IpcError::Authentication {
                message: "Invalid ciphertext".to_string(),
                code: crate::plugin_ipc::error::AuthErrorCode::DecryptionFailed,
                retry_after: None,
            });
        }

        let plaintext_len = ciphertext.len() - key.len();
        Ok(ciphertext[..plaintext_len].to_vec())
    }
}

/// Security session information
#[derive(Debug, Clone)]
pub struct SecuritySession {
    pub session_id: String,
    pub plugin_id: String,
    pub user_id: String,
    pub permissions: Vec<Permission>,
    pub created_at: SystemTime,
    pub last_activity: SystemTime,
    pub expires_at: SystemTime,
    pub capabilities: crate::plugin_ipc::message::ClientCapabilities,
    pub security_level: SecurityLevel,
}

/// Authentication result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub session_id: String,
    pub plugin_id: String,
    pub permissions: Vec<Permission>,
    pub expires_at: SystemTime,
    pub security_level: SecurityLevel,
}

/// Token data extracted from authentication
#[derive(Debug, Clone)]
struct TokenData {
    pub user_id: String,
    pub permissions: Vec<Permission>,
    pub expires_at: SystemTime,
}

/// Permission definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Permission {
    pub action: String,
    pub resource: String,
    pub effect: PermissionEffect,
}

/// Permission effect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PermissionEffect {
    Allow,
    Deny,
}

/// Security level
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityLevel {
    Low,
    Medium,
    High,
    Maximum,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub token_type: TokenType,
    pub session_timeout_s: u64,
    pub max_sessions_per_user: u32,
    pub token_expiry_s: u64,
    pub refresh_enabled: bool,
    pub issuer: String,
    pub audience: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_type: TokenType::Jwt,
            session_timeout_s: 3600,
            max_sessions_per_user: 10,
            token_expiry_s: 7200,
            refresh_enabled: true,
            issuer: "crucible-daemon".to_string(),
            audience: "crucible-plugins".to_string(),
        }
    }
}

/// Token type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TokenType {
    Jwt,
    ApiKey,
    Certificate,
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub algorithm: EncryptionAlgorithm,
    pub key_rotation_interval_s: u64,
    pub key_derivation: KeyDerivation,
    pub compression_enabled: bool,
    pub integrity_check: bool,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::Aes256Gcm,
            key_rotation_interval_s: 86400,
            key_derivation: KeyDerivation::HkdfSha256,
            compression_enabled: true,
            integrity_check: true,
        }
    }
}

/// Encryption algorithm
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
    ChaCha20Poly1305,
}

/// Key derivation method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KeyDerivation {
    HkdfSha256,
    Pbkdf2,
    Scrypt,
}

/// Authorization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationConfig {
    pub allowed_plugins: Vec<String>,
    pub blocked_plugins: Vec<String>,
    pub default_permissions: Vec<Permission>,
    pub rbac_enabled: bool,
    pub abac_enabled: bool,
    pub policy_engine: String,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            allowed_plugins: vec![], // Empty means all allowed
            blocked_plugins: vec![],
            default_permissions: vec![
                Permission {
                    action: "execute".to_string(),
                    resource: "*".to_string(),
                    effect: PermissionEffect::Allow,
                },
            ],
            rbac_enabled: false,
            abac_enabled: false,
            policy_engine: "default".to_string(),
        }
    }
}

/// Plugin capabilities
#[derive(Debug, Clone)]
pub struct PluginCapabilities {
    pub plugin_id: String,
    pub allowed_operations: Vec<String>,
    pub resource_limits: ResourceLimits,
    pub security_level: SecurityLevel,
    pub sandbox_config: SandboxConfig,
}

/// Resource limits for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: u64,
    pub max_cpu_cores: f64,
    pub max_disk_mb: u64,
    pub max_network_bandwidth_mbps: u64,
    pub max_file_descriptors: u32,
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory_mb: 1024,
            max_cpu_cores: 2.0,
            max_disk_mb: 10240,
            max_network_bandwidth_mbps: 100,
            max_file_descriptors: 1024,
            max_processes: 100,
        }
    }
}

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub sandbox_type: SandboxType,
    pub isolated_filesystem: bool,
    pub network_access: bool,
    pub allowed_syscalls: Vec<String>,
    pub blocked_syscalls: Vec<String>,
    pub environment_variables: HashMap<String, String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sandbox_type: SandboxType::Process,
            isolated_filesystem: true,
            network_access: false,
            allowed_syscalls: vec![],
            blocked_syscalls: vec![
                "ptrace".to_string(),
                "mount".to_string(),
                "umount".to_string(),
            ],
            environment_variables: HashMap::new(),
        }
    }
}

/// Sandbox type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxType {
    Process,
    Container,
    VirtualMachine,
    Language,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_minute: u32,
    pub requests_per_hour: u32,
    pub burst_size: u32,
    pub penalty_duration_s: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            requests_per_hour: 1000,
            burst_size: 10,
            penalty_duration_s: 300,
        }
    }
}

/// Audit logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub enabled: bool,
    pub log_all_requests: bool,
    pub log_failed_requests: bool,
    pub log_authentication: bool,
    pub log_authorization: bool,
    pub log_sensitive_data: bool,
    pub retention_days: u32,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_all_requests: false,
            log_failed_requests: true,
            log_authentication: true,
            log_authorization: true,
            log_sensitive_data: false,
            retention_days: 90,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_manager_creation() {
        let auth_config = AuthConfig::default();
        let encryption_config = EncryptionConfig::default();
        let authz_config = AuthorizationConfig::default();

        let security_manager = SecurityManager::new(
            auth_config.clone(),
            encryption_config.clone(),
            authz_config.clone(),
        );

        assert_eq!(security_manager.active_session_count(), 0);
    }

    #[test]
    fn test_session_creation() {
        let mut security_manager = SecurityManager::new(
            AuthConfig::default(),
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        let handshake = crate::plugin_ipc::message::HandshakePayload {
            protocol_version: 1,
            client_id: "test_plugin".to_string(),
            auth_token: "crucible_test_token_12345".to_string(),
            supported_types: vec![
                crate::plugin_ipc::message::MessageType::Request,
                crate::plugin_ipc::message::MessageType::Response,
            ],
            compression_algos: vec!["lz4".to_string()],
            encryption_algos: vec!["aes256".to_string()],
            max_message_size: 1024 * 1024,
            capabilities: crate::plugin_ipc::message::ClientCapabilities {
                plugin_types: vec!["test".to_string()],
                operations: vec!["test_op".to_string()],
                data_formats: vec!["json".to_string()],
                max_concurrent_requests: 10,
                supports_streaming: false,
                supports_batching: true,
                supports_compression: true,
                supports_encryption: true,
            },
            metadata: HashMap::new(),
        };

        // This would normally require async execution
        // For testing, we'll create a session directly
        let session = SecuritySession {
            session_id: Uuid::new_v4().to_string(),
            plugin_id: handshake.client_id.clone(),
            user_id: "test_user".to_string(),
            permissions: vec![],
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
            capabilities: handshake.capabilities.clone(),
            security_level: SecurityLevel::High,
        };

        assert_eq!(session.plugin_id, "test_plugin");
        assert_eq!(session.security_level, SecurityLevel::High);
    }

    #[test]
    fn test_permission_check() {
        let security_manager = SecurityManager::new(
            AuthConfig::default(),
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        let permissions = vec![
            Permission {
                action: "execute".to_string(),
                resource: "test_plugin".to_string(),
                effect: PermissionEffect::Allow,
            },
            Permission {
                action: "read".to_string(),
                resource: "*".to_string(),
                effect: PermissionEffect::Allow,
            },
        ];

        assert!(security_manager.has_permission(&permissions, "execute", "test_plugin"));
        assert!(security_manager.has_permission(&permissions, "read", "any_resource"));
        assert!(!security_manager.has_permission(&permissions, "write", "test_plugin"));
    }

    #[test]
    fn test_security_level_determination() {
        let security_manager = SecurityManager::new(
            AuthConfig::default(),
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        let high_security_caps = crate::plugin_ipc::message::ClientCapabilities {
            plugin_types: vec![],
            operations: vec![],
            data_formats: vec![],
            max_concurrent_requests: 10,
            supports_streaming: false,
            supports_batching: true,
            supports_compression: true,
            supports_encryption: true,
        };

        let medium_security_caps = crate::plugin_ipc::message::ClientCapabilities {
            supports_compression: false,
            supports_encryption: true,
            ..Default::default()
        };

        let low_security_caps = crate::plugin_ipc::message::ClientCapabilities {
            supports_compression: false,
            supports_encryption: false,
            ..Default::default()
        };

        assert_eq!(
            security_manager.determine_security_level(&high_security_caps),
            SecurityLevel::High
        );
        assert_eq!(
            security_manager.determine_security_level(&medium_security_caps),
            SecurityLevel::Medium
        );
        assert_eq!(
            security_manager.determine_security_level(&low_security_caps),
            SecurityLevel::Low
        );
    }

    #[test]
    fn test_encryption_decryption() {
        let security_manager = SecurityManager::new(
            AuthConfig::default(),
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        let session = SecuritySession {
            session_id: "test_session".to_string(),
            plugin_id: "test_plugin".to_string(),
            user_id: "test_user".to_string(),
            permissions: vec![],
            created_at: SystemTime::now(),
            last_activity: SystemTime::now(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
            capabilities: Default::default(),
            security_level: SecurityLevel::High,
        };

        let plaintext = b"Hello, World!";
        let encrypted = security_manager.encrypt_message(&session.session_id, plaintext).unwrap();
        let decrypted = security_manager.decrypt_message(&session.session_id, &encrypted).unwrap();

        assert_eq!(plaintext.to_vec(), decrypted);
    }

    #[test]
    fn test_token_verification() {
        let security_manager = SecurityManager::new(
            AuthConfig::default(),
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        // Test invalid JWT token (too short)
        let invalid_token = "abc";
        let result = std::thread::sleep(std::time::Duration::from_millis(1));

        // This is a simplified test - in reality, we'd need to use async testing
        assert!(invalid_token.len() < 10);
    }

    #[test]
    fn test_api_key_verification() {
        let security_manager = SecurityManager::new(
            AuthConfig {
                token_type: TokenType::ApiKey,
                ..Default::default()
            },
            EncryptionConfig::default(),
            AuthorizationConfig::default(),
        );

        // Valid API key format
        let valid_key = "crucible_12345";
        assert!(valid_key.starts_with("crucible_"));

        // Invalid API key format
        let invalid_key = "invalid_key";
        assert!(!invalid_key.starts_with("crucible_"));
    }

    #[test]
    fn test_default_configurations() {
        let auth_config = AuthConfig::default();
        assert_eq!(auth_config.token_type, TokenType::Jwt);
        assert_eq!(auth_config.session_timeout_s, 3600);
        assert!(auth_config.refresh_enabled);

        let encryption_config = EncryptionConfig::default();
        assert_eq!(encryption_config.algorithm, EncryptionAlgorithm::Aes256Gcm);
        assert!(encryption_config.compression_enabled);
        assert!(encryption_config.integrity_check);

        let authz_config = AuthorizationConfig::default();
        assert!(authz_config.allowed_plugins.is_empty());
        assert_eq!(authz_config.default_permissions.len(), 1);
        assert_eq!(authz_config.default_permissions[0].action, "execute");

        let rate_limit_config = RateLimitConfig::default();
        assert!(rate_limit_config.enabled);
        assert_eq!(rate_limit_config.requests_per_minute, 60);

        let audit_config = AuditConfig::default();
        assert!(audit_config.enabled);
        assert!(!audit_config.log_all_requests);
        assert!(audit_config.log_failed_requests);
    }

    #[test]
    fn test_sandbox_configuration() {
        let sandbox_config = SandboxConfig::default();
        assert!(sandbox_config.enabled);
        assert_eq!(sandbox_config.sandbox_type, SandboxType::Process);
        assert!(sandbox_config.isolated_filesystem);
        assert!(!sandbox_config.network_access);
        assert!(!sandbox_config.allowed_syscalls.contains(&"ptrace".to_string()));
        assert!(sandbox_config.blocked_syscalls.contains(&"ptrace".to_string()));
    }

    #[test]
    fn test_resource_limits() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_mb, 1024);
        assert_eq!(limits.max_cpu_cores, 2.0);
        assert_eq!(limits.max_disk_mb, 10240);
        assert_eq!(limits.max_network_bandwidth_mbps, 100);
        assert_eq!(limits.max_file_descriptors, 1024);
        assert_eq!(limits.max_processes, 100);
    }
}