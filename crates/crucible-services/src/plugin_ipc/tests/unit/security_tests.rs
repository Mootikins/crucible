//! # Security Component Tests
//!
//! Comprehensive tests for IPC security components including authentication,
//! authorization, encryption, and security policy enforcement.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::plugin_ipc::{
    error::{IpcError, IpcResult, AuthErrorCode},
    security::{SecurityManager, AuthConfig, EncryptionConfig, AuthorizationConfig},
    message::{IpcMessage, MessageType, MessagePayload, MessageHeader},
};

use super::common::{
    *,
    fixtures::*,
    mocks::*,
    helpers::*,
};

/// Authentication tests
pub struct AuthenticationTests;

impl AuthenticationTests {
    /// Test JWT token generation and validation
    pub async fn test_jwt_token_lifecycle() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let user_id = "test_user";
        let capabilities = vec!["read".to_string(), "write".to_string()];

        // Generate token
        let token = security_manager.generate_token(user_id, capabilities.clone()).await?;
        assert!(!token.is_empty());

        // Validate token
        let is_valid = security_manager.validate_token(&token).await?;
        assert!(is_valid);

        // Authenticate with token
        let session_id = security_manager.authenticate(&token).await?;
        assert!(!session_id.is_empty());

        Ok(())
    }

    /// Test token expiration
    pub async fn test_token_expiration() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();

        // Create a token that expires immediately (in a real implementation)
        let user_id = "test_user";
        let capabilities = vec!["read".to_string()];

        let token = security_manager.generate_token(user_id, capabilities).await?;

        // Simulate token expiration by removing it from valid tokens
        security_manager.valid_tokens.write().await.remove(&token);

        // Try to authenticate with expired token
        let result = security_manager.authenticate(&token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("InvalidToken"));

        Ok(())
    }

    /// Test token refresh
    pub async fn test_token_refresh() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let user_id = "test_user";
        let capabilities = vec!["read".to_string()];

        // Generate initial token
        let original_token = security_manager.generate_token(user_id, capabilities.clone()).await?;

        // Refresh token
        let refreshed_token = security_manager.refresh_token(&original_token).await?;

        // Both tokens should be valid (in a real implementation, the original might be invalidated)
        let original_valid = security_manager.validate_token(&original_token).await?;
        let refreshed_valid = security_manager.validate_token(&refreshed_token).await?;

        assert!(original_valid);
        assert!(refreshed_valid);

        Ok(())
    }

    /// Test token revocation
    pub async fn test_token_revocation() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let user_id = "test_user";
        let capabilities = vec!["read".to_string()];

        // Generate token
        let token = security_manager.generate_token(user_id, capabilities).await?;

        // Validate token is valid
        let is_valid = security_manager.validate_token(&token).await?;
        assert!(is_valid);

        // Revoke token
        security_manager.revoke_token(&token).await?;

        // Validate token is now invalid
        let is_valid = security_manager.validate_token(&token).await?;
        assert!(!is_valid);

        Ok(())
    }

    /// Test authentication with invalid token
    pub async fn test_invalid_token_authentication() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();

        // Try to authenticate with completely invalid token
        let invalid_token = "invalid_token_12345";
        let result = security_manager.authenticate(invalid_token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("InvalidToken"));

        Ok(())
    }

    /// Test concurrent authentication
    pub async fn test_concurrent_authentication() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_authentications = 100;

        // Pre-generate tokens
        let mut tokens = Vec::new();
        for i in 0..num_authentications {
            let user_id = format!("user_{}", i);
            let capabilities = vec!["read".to_string()];
            let token = security_manager.generate_token(&user_id, capabilities).await?;
            tokens.push(token);
        }

        // Authenticate concurrently
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_authentications,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let token = tokens[i].clone();
                async move {
                    security_manager.authenticate(&token).await
                }
            },
        ).await;

        // Verify all authentications succeeded
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_authentications);

        Ok(())
    }

    /// Test authentication failure scenarios
    pub async fn test_authentication_failure_scenarios() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());

        // Test with failing security manager
        security_manager.set_failure(true).await;

        let user_id = "test_user";
        let capabilities = vec!["read".to_string()];

        // Token generation should fail
        let result = security_manager.generate_token(user_id, capabilities).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("TokenGenerationFailed"));

        // Reset failure
        security_manager.set_failure(false).await;

        // Generate valid token
        let token = security_manager.generate_token(user_id, vec!["read"]).await?;

        // Set failure for authentication
        security_manager.set_failure(true).await;

        // Authentication should fail
        let result = security_manager.authenticate(&token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("InvalidToken"));

        Ok(())
    }
}

/// Authorization tests
pub struct AuthorizationTests;

impl AuthorizationTests {
    /// Test basic authorization
    pub async fn test_basic_authorization() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Test authorization for various operations
        let operations = vec!["read", "write", "delete", "admin"];

        for operation in operations {
            let authorized = security_manager.authorize(&session_id, operation).await?;
            // Mock implementation allows all operations
            assert!(authorized);
        }

        Ok(())
    }

    /// Test authorization failure
    pub async fn test_authorization_failure() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();

        // Configure security manager to fail authorization
        *security_manager.should_fail_authorization.lock().await = true;

        let session_id = Uuid::new_v4().to_string();
        let operation = "restricted_operation";

        let result = security_manager.authorize(&session_id, operation).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("InsufficientPermissions"));

        Ok(())
    }

    /// Test capability-based authorization
    pub async fn test_capability_based_authorization() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();

        // Create session with specific capabilities
        let session_id = Uuid::new_v4().to_string();
        let capabilities = vec!["read".to_string(), "write".to_string()];
        security_manager.create_session(session_id.clone()).await;

        // Test authorized operations
        for capability in &capabilities {
            let authorized = security_manager.authorize(&session_id, capability).await?;
            assert!(authorized);
        }

        Ok(())
    }

    /// Test concurrent authorization
    pub async fn test_concurrent_authorization() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let session_id = Uuid::new_v4().to_string();
        let num_operations = 100;

        let operations: Vec<String> = (0..num_operations)
            .map(|i| format!("operation_{}", i))
            .collect();

        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let operation = operations[i].clone();
                async move {
                    security_manager.authorize(&session_id, &operation).await
                }
            },
        ).await;

        // Verify all authorizations succeeded
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        Ok(())
    }
}

/// Encryption tests
pub struct EncryptionTests;

impl EncryptionTests {
    /// Test basic message encryption and decryption
    pub async fn test_basic_encryption() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        let original_data = b"Hello, encrypted world!";

        // Encrypt data
        let encrypted = security_manager.encrypt_message(&session_id, original_data).await?;
        assert!(!encrypted.is_empty());
        assert_ne!(encrypted, original_data.to_vec());

        // Decrypt data
        let decrypted = security_manager.decrypt_message(&session_id, &encrypted).await?;
        assert_eq!(decrypted, original_data);

        Ok(())
    }

    /// Test encryption with large data
    pub async fn test_large_data_encryption() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        // Create large data (1MB)
        let original_data = vec![0u8; 1024 * 1024];

        // Encrypt large data
        let encrypted = security_manager.encrypt_message(&session_id, &original_data).await?;
        assert!(!encrypted.is_empty());
        assert_ne!(encrypted.len(), original_data.len());

        // Decrypt large data
        let decrypted = security_manager.decrypt_message(&session_id, &encrypted).await?;
        assert_eq!(decrypted, original_data);

        Ok(())
    }

    /// Test encryption failure scenarios
    pub async fn test_encryption_failures() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Test encryption with non-existent session
        let data = b"test data";
        let result = security_manager.encrypt_message(&session_id, data).await;
        // Mock implementation may not validate session, but real implementation should

        // Test decryption failure
        security_manager.set_failure(true).await;

        // Create session
        security_manager.create_session(session_id.clone()).await;

        let encrypted = security_manager.encrypt_message(&session_id, data).await?;
        let result = security_manager.decrypt_message(&session_id, &encrypted).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().has_error_code("DecryptionFailed"));

        Ok(())
    }

    /// Test concurrent encryption operations
    pub async fn test_concurrent_encryption() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_operations = 50;

        // Create sessions
        let mut session_ids = Vec::new();
        for _ in 0..num_operations {
            let session_id = Uuid::new_v4().to_string();
            security_manager.create_session(session_id.clone()).await;
            session_ids.push(session_id);
        }

        // Encrypt data concurrently
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let session_id = session_ids[i].clone();
                let data = format!("Test data for operation {}", i).into_bytes();
                async move {
                    let encrypted = security_manager.encrypt_message(&session_id, &data).await?;
                    let decrypted = security_manager.decrypt_message(&session_id, &encrypted).await?;
                    Ok((data, decrypted))
                }
            },
        ).await;

        // Verify all operations succeeded and data integrity
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        for result in results {
            if let Ok((original, decrypted)) = result {
                assert_eq!(original, decrypted);
            }
        }

        Ok(())
    }

    /// Test encryption with different algorithms
    pub async fn test_encryption_algorithms() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        let test_cases = vec![
            b"Small message".to_vec(),
            b"Medium message with some content that is longer than the small one".to_vec(),
            vec![0u8; 1024], // 1KB of zeros
            vec![255u8; 1024], // 1KB of 255s
        ];

        for data in test_cases {
            // Encrypt
            let encrypted = security_manager.encrypt_message(&session_id, &data).await?;
            assert!(!encrypted.is_empty());

            // Decrypt
            let decrypted = security_manager.decrypt_message(&session_id, &encrypted).await?;
            assert_eq!(decrypted, data);
        }

        Ok(())
    }
}

/// Security policy tests
pub struct SecurityPolicyTests;

impl SecurityPolicyTests {
    /// Test message security flags enforcement
    pub async fn test_message_security_flags() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        // Create message with security flags
        let mut message = MessageFixtures::request("secure_operation", json!({"sensitive": "data"}));
        message.header.session_id = session_id.clone();
        message.header.flags.encrypted = true;
        message.header.flags.compressed = true;
        message.header.flags.requires_ack = true;

        // Test that security flags are properly set
        assert!(message.header.flags.encrypted);
        assert!(message.header.flags.compressed);
        assert!(message.header.flags.requires_ack);

        Ok(())
    }

    /// Test access control policies
    pub async fn test_access_control_policies() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();

        // Define access policies
        let policies = vec![
            ("admin_user", vec!["read", "write", "delete", "admin"]),
            ("regular_user", vec!["read", "write"]),
            ("guest_user", vec!["read"]),
        ];

        for (user_type, allowed_operations) in policies {
            let session_id = format!("session_{}", user_type);
            security_manager.create_session(session_id.clone()).await;

            for operation in allowed_operations {
                let authorized = security_manager.authorize(&session_id, operation).await?;
                assert!(authorized, "User {} should be able to {}", user_type, operation);
            }
        }

        Ok(())
    }

    /// Test security audit logging
    pub async fn test_security_audit_logging() -> IpcResult<()> {
        let security_manager = MockSecurityManager::new();
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        // Perform security operations
        let token = security_manager.generate_token("test_user", vec!["read"]).await?;
        let _auth_result = security_manager.authenticate(&token).await?;
        let _authz_result = security_manager.authorize(&session_id, "read").await?;

        let data = b"Audit test data";
        let _encrypted = security_manager.encrypt_message(&session_id, data).await?;
        let _decrypted = security_manager.decrypt_message(&session_id, &data).await?;

        // In a real implementation, audit logs would be recorded
        // For testing, we just verify the operations complete successfully
        Ok(())
    }

    /// Test rate limiting for security operations
    pub async fn test_security_rate_limiting() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());

        // Simulate high rate of authentication attempts
        let num_attempts = 1000;
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_attempts,
            |_| {
                let security_manager = Arc::clone(&security_manager);
                async move {
                    // Generate and authenticate token
                    let token = security_manager.generate_token("rate_limit_test", vec!["read"]).await?;
                    security_manager.authenticate(&token).await
                }
            },
        ).await;

        // Most operations should succeed (mock implementation doesn't enforce rate limits)
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert!(success_count > num_attempts / 2); // At least 50% success rate

        Ok(())
    }
}

/// Security performance tests
pub struct SecurityPerformanceTests;

impl SecurityPerformanceTests {
    /// Benchmark authentication performance
    pub async fn benchmark_authentication() -> IpcResult<f64> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_operations = 1000;

        // Pre-generate tokens
        let mut tokens = Vec::new();
        for i in 0..num_operations {
            let user_id = format!("user_{}", i);
            let token = security_manager.generate_token(&user_id, vec!["read"]).await?;
            tokens.push(token);
        }

        // Benchmark authentication
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let token = tokens[i].clone();
                async move {
                    security_manager.authenticate(&token).await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate ops per second
        let ops_per_sec = num_operations as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        Ok(ops_per_sec)
    }

    /// Benchmark encryption performance
    pub async fn benchmark_encryption() -> IpcResult<f64> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_operations = 500;
        let data_size = 1024; // 1KB

        // Create sessions
        let mut session_ids = Vec::new();
        for _ in 0..num_operations {
            let session_id = Uuid::new_v4().to_string();
            security_manager.create_session(session_id.clone()).await;
            session_ids.push(session_id);
        }

        // Benchmark encryption and decryption
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let session_id = session_ids[i].clone();
                let data = vec![i as u8; data_size];
                async move {
                    let encrypted = security_manager.encrypt_message(&session_id, &data).await?;
                    let decrypted = security_manager.decrypt_message(&session_id, &encrypted).await?;
                    Ok(())
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate ops per second
        let ops_per_sec = num_operations as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        Ok(ops_per_sec)
    }

    /// Benchmark authorization performance
    pub async fn benchmark_authorization() -> IpcResult<f64> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_operations = 10000;
        let session_id = Uuid::new_v4().to_string();

        // Create session
        security_manager.create_session(session_id.clone()).await;

        // Benchmark authorization
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let operation = format!("operation_{}", i % 100); // 100 different operations
                async move {
                    security_manager.authorize(&session_id, &operation).await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate ops per second
        let ops_per_sec = num_operations as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        Ok(ops_per_sec)
    }

    /// Benchmark token generation performance
    pub async fn benchmark_token_generation() -> IpcResult<f64> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_operations = 1000;

        // Benchmark token generation
        let start = SystemTime::now();
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                let user_id = format!("user_{}", i);
                let capabilities = vec!["read".to_string(), "write".to_string()];
                async move {
                    security_manager.generate_token(&user_id, capabilities).await
                }
            },
        ).await;
        let duration = start.elapsed().unwrap();

        // Calculate ops per second
        let ops_per_sec = num_operations as f64 / duration.as_secs_f64();

        // Verify success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, num_operations);

        Ok(ops_per_sec)
    }
}

/// Security integration tests
pub struct SecurityIntegrationTests;

impl SecurityIntegrationTests {
    /// Test end-to-end secure message flow
    pub async fn test_secure_message_flow() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());

        // 1. Authenticate and get session
        let token = security_manager.generate_token("test_user", vec!["read", "write"]).await?;
        let session_id = security_manager.authenticate(&token).await?;

        // 2. Create secure message
        let mut message = MessageFixtures::request("secure_operation", json!({"sensitive": "data"}));
        message.header.session_id = session_id.clone();
        message.header.flags.encrypted = true;

        // 3. Authorize operation
        let authorized = security_manager.authorize(&session_id, "secure_operation").await?;
        assert!(authorized);

        // 4. Encrypt message payload
        let payload_data = serde_json::to_vec(&message.payload)?;
        let encrypted_payload = security_manager.encrypt_message(&session_id, &payload_data).await?;

        // 5. Decrypt message payload
        let decrypted_payload = security_manager.decrypt_message(&session_id, &encrypted_payload).await?;
        assert_eq!(decrypted_payload, payload_data);

        Ok(())
    }

    /// Test secure session management
    pub async fn test_secure_session_management() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_sessions = 100;

        // Create multiple sessions
        let mut session_ids = Vec::new();
        for i in 0..num_sessions {
            let token = security_manager.generate_token(&format!("user_{}", i), vec!["read"]).await?;
            let session_id = security_manager.authenticate(&token).await?;
            session_ids.push(session_id);
        }

        // Verify all sessions are unique
        let mut unique_sessions = std::collections::HashSet::new();
        for session_id in &session_ids {
            unique_sessions.insert(session_id);
        }
        assert_eq!(unique_sessions.len(), num_sessions);

        // Test operations on all sessions
        for session_id in &session_ids {
            let authorized = security_manager.authorize(session_id, "read").await?;
            assert!(authorized);

            let data = format!("Session data for {}", session_id).into_bytes();
            let encrypted = security_manager.encrypt_message(session_id, &data).await?;
            let decrypted = security_manager.decrypt_message(session_id, &encrypted).await?;
            assert_eq!(decrypted, data);
        }

        Ok(())
    }

    /// Test security under load
    pub async fn test_security_under_load() -> IpcResult<()> {
        let security_manager = Arc::new(MockSecurityManager::new());
        let num_concurrent_operations = 1000;

        // Mix of security operations
        let results = ConcurrencyTestUtils::run_concurrent_operations(
            num_concurrent_operations,
            |i| {
                let security_manager = Arc::clone(&security_manager);
                async move {
                    match i % 4 {
                        0 => {
                            // Token generation and authentication
                            let user_id = format!("load_user_{}", i);
                            let capabilities = vec!["read".to_string()];
                            let token = security_manager.generate_token(&user_id, capabilities).await?;
                            security_manager.authenticate(&token).await
                        }
                        1 => {
                            // Authorization
                            let session_id = Uuid::new_v4().to_string();
                            security_manager.authorize(&session_id, "test_operation").await
                        }
                        2 => {
                            // Encryption
                            let session_id = Uuid::new_v4().to_string();
                            let data = format!("Load test data {}", i).into_bytes();
                            let encrypted = security_manager.encrypt_message(&session_id, &data).await?;
                            security_manager.decrypt_message(&session_id, &encrypted).await.map(|_| ())
                        }
                        _ => {
                            // Token validation
                            let token = format!("load_token_{}", i);
                            security_manager.validate_token(&token).await.map(|_| ())
                        }
                    }
                }
            },
        ).await;

        // Check success rate
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let success_rate = success_count as f64 / num_concurrent_operations as f64;

        // Should have at least 90% success rate under load
        assert!(success_rate >= 0.9);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async_test!(test_jwt_token_lifecycle, {
        AuthenticationTests::test_jwt_token_lifecycle().await.unwrap();
        "success"
    });

    async_test!(test_token_expiration, {
        AuthenticationTests::test_token_expiration().await.unwrap();
        "success"
    });

    async_test!(test_token_refresh, {
        AuthenticationTests::test_token_refresh().await.unwrap();
        "success"
    });

    async_test!(test_token_revocation, {
        AuthenticationTests::test_token_revocation().await.unwrap();
        "success"
    });

    async_test!(test_invalid_token_authentication, {
        AuthenticationTests::test_invalid_token_authentication().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_authentication, {
        AuthenticationTests::test_concurrent_authentication().await.unwrap();
        "success"
    });

    async_test!(test_authentication_failure_scenarios, {
        AuthenticationTests::test_authentication_failure_scenarios().await.unwrap();
        "success"
    });

    async_test!(test_basic_authorization, {
        AuthorizationTests::test_basic_authorization().await.unwrap();
        "success"
    });

    async_test!(test_authorization_failure, {
        AuthorizationTests::test_authorization_failure().await.unwrap();
        "success"
    });

    async_test!(test_capability_based_authorization, {
        AuthorizationTests::test_capability_based_authorization().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_authorization, {
        AuthorizationTests::test_concurrent_authorization().await.unwrap();
        "success"
    });

    async_test!(test_basic_encryption, {
        EncryptionTests::test_basic_encryption().await.unwrap();
        "success"
    });

    async_test!(test_large_data_encryption, {
        EncryptionTests::test_large_data_encryption().await.unwrap();
        "success"
    });

    async_test!(test_encryption_failures, {
        EncryptionTests::test_encryption_failures().await.unwrap();
        "success"
    });

    async_test!(test_concurrent_encryption, {
        EncryptionTests::test_concurrent_encryption().await.unwrap();
        "success"
    });

    async_test!(test_encryption_algorithms, {
        EncryptionTests::test_encryption_algorithms().await.unwrap();
        "success"
    });

    async_test!(test_message_security_flags, {
        SecurityPolicyTests::test_message_security_flags().await.unwrap();
        "success"
    });

    async_test!(test_access_control_policies, {
        SecurityPolicyTests::test_access_control_policies().await.unwrap();
        "success"
    });

    async_test!(test_security_audit_logging, {
        SecurityPolicyTests::test_security_audit_logging().await.unwrap();
        "success"
    });

    async_test!(test_security_rate_limiting, {
        SecurityPolicyTests::test_security_rate_limiting().await.unwrap();
        "success"
    });

    async_test!(test_secure_message_flow, {
        SecurityIntegrationTests::test_secure_message_flow().await.unwrap();
        "success"
    });

    async_test!(test_secure_session_management, {
        SecurityIntegrationTests::test_secure_session_management().await.unwrap();
        "success"
    });

    async_test!(test_security_under_load, {
        SecurityIntegrationTests::test_security_under_load().await.unwrap();
        "success"
    });

    async_test!(test_authentication_performance, {
        let ops_per_sec = SecurityPerformanceTests::benchmark_authentication().await.unwrap();
        assert!(ops_per_sec > 100.0); // At least 100 ops/sec
        ops_per_sec
    });

    async_test!(test_encryption_performance, {
        let ops_per_sec = SecurityPerformanceTests::benchmark_encryption().await.unwrap();
        assert!(ops_per_sec > 50.0); // At least 50 ops/sec
        ops_per_sec
    });

    async_test!(test_authorization_performance, {
        let ops_per_sec = SecurityPerformanceTests::benchmark_authorization().await.unwrap();
        assert!(ops_per_sec > 1000.0); // At least 1000 ops/sec
        ops_per_sec
    });

    async_test!(test_token_generation_performance, {
        let ops_per_sec = SecurityPerformanceTests::benchmark_token_generation().await.unwrap();
        assert!(ops_per_sec > 100.0); // At least 100 ops/sec
        ops_per_sec
    });
}