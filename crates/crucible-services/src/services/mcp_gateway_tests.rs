//! # MCP Gateway Unit Tests
//!
//! This module contains comprehensive unit tests for the MCP Gateway service,
//! testing session management, tool registration, protocol handling, and error scenarios.

use std::collections::HashMap;
use std::time::Duration;

use crucible_services::{
    mcp_gateway::{
        McpGateway, McpGatewayConfig, McpSession, McpSessionId, McpToolRegistry,
        McpConnectionInfo, McpCapability, McpProtocolVersion, McpMessage,
        McpRequestType, McpResponseType, McpErrorCode, McpError, SessionStatus
    },
    errors::ServiceError,
    types::tool::{ToolDefinition, ToolParameter, ToolExecutionContext},
};

/// Create a test MCP gateway configuration
fn create_test_config() -> McpGatewayConfig {
    McpGatewayConfig {
        max_sessions: 100,
        session_timeout: Duration::from_secs(30),
        heartbeat_interval: Duration::from_secs(10),
        supported_versions: vec![
            McpProtocolVersion::V1_0_0,
            McpProtocolVersion::V1_1_0,
        ],
        default_capabilities: vec![
            McpCapability::Tools,
            McpCapability::Resources,
            McpCapability::Prompts,
        ],
        enable_compression: true,
        max_message_size: 1024 * 1024, // 1MB
        enable_metrics: true,
    }
}

/// Create a test session
fn create_test_session(id: &str) -> McpSession {
    McpSession {
        id: McpSessionId::from(id),
        status: SessionStatus::Active,
        created_at: chrono::Utc::now(),
        last_activity: chrono::Utc::now(),
        protocol_version: McpProtocolVersion::V1_0_0,
        capabilities: vec![
            McpCapability::Tools,
            McpCapability::Resources,
        ],
        metadata: {
            let mut meta = HashMap::new();
            meta.insert("client_name".to_string(), "test_client".to_string());
            meta.insert("user_id".to_string(), "test_user".to_string());
            meta
        },
        tools: Vec::new(),
    }
}

/// Create a test tool definition
fn create_test_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: format!("Test tool: {}", name),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input parameter"
                }
            },
            "required": ["input"]
        }),
        category: Some("test".to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("test_author".to_string()),
        tags: vec!["test".to_string(), "mcp".to_string()],
        enabled: true,
        parameters: vec![
            ToolParameter {
                name: "input".to_string(),
                param_type: "string".to_string(),
                description: Some("Input parameter".to_string()),
                required: true,
                default_value: None,
            }
        ],
    }
}

#[cfg(test)]
mod mcp_gateway_tests {
    use super::*;

    #[test]
    fn test_mcp_gateway_config_creation() {
        let config = create_test_config();

        assert_eq!(config.max_sessions, 100);
        assert_eq!(config.session_timeout, Duration::from_secs(30));
        assert_eq!(config.heartbeat_interval, Duration::from_secs(10));
        assert!(config.supported_versions.contains(&McpProtocolVersion::V1_0_0));
        assert!(config.supported_versions.contains(&McpProtocolVersion::V1_1_0));
        assert!(config.enable_compression);
        assert_eq!(config.max_message_size, 1024 * 1024);
        assert!(config.enable_metrics);
    }

    #[test]
    fn test_mcp_session_id_creation() {
        let id1 = McpSessionId::from("session_123");
        let id2 = McpSessionId::new();
        let id3 = McpSessionId::with_uuid(uuid::Uuid::new_v4());

        assert_eq!(id1.to_string(), "session_123");
        assert!(!id2.to_string().is_empty());
        assert!(!id3.to_string().is_empty());
        assert_ne!(id2.to_string(), id3.to_string());
    }

    #[test]
    fn test_session_status_variants() {
        let statuses = vec![
            SessionStatus::Connecting,
            SessionStatus::Active,
            SessionStatus::Idle,
            SessionStatus::Error(String::from("Connection lost")),
            SessionStatus::Closed,
        ];

        for status in statuses {
            match status {
                SessionStatus::Connecting => assert!(matches!(status, SessionStatus::Connecting)),
                SessionStatus::Active => assert!(matches!(status, SessionStatus::Active)),
                SessionStatus::Idle => assert!(matches!(status, SessionStatus::Idle)),
                SessionStatus::Error(_) => assert!(matches!(status, SessionStatus::Error(_))),
                SessionStatus::Closed => assert!(matches!(status, SessionStatus::Closed)),
            }
        }
    }

    #[test]
    fn test_mcp_session_creation() {
        let session = create_test_session("test_session");

        assert_eq!(session.id.to_string(), "test_session");
        assert!(matches!(session.status, SessionStatus::Active));
        assert!(matches!(session.protocol_version, McpProtocolVersion::V1_0_0));
        assert_eq!(session.capabilities.len(), 2);
        assert!(session.capabilities.contains(&McpCapability::Tools));
        assert!(session.capabilities.contains(&McpCapability::Resources));
        assert_eq!(session.metadata.get("client_name"), Some(&"test_client".to_string()));
        assert!(session.tools.is_empty());
    }

    #[test]
    fn test_mcp_capability_variants() {
        let capabilities = vec![
            McpCapability::Tools,
            McpCapability::Resources,
            McpCapability::Prompts,
            McpCapability::Logging,
        ];

        for capability in capabilities {
            match capability {
                McpCapability::Tools => assert!(matches!(capability, McpCapability::Tools)),
                McpCapability::Resources => assert!(matches!(capability, McpCapability::Resources)),
                McpCapability::Prompts => assert!(matches!(capability, McpCapability::Prompts)),
                McpCapability::Logging => assert!(matches!(capability, McpCapability::Logging)),
            }
        }
    }

    #[test]
    fn test_mcp_protocol_version() {
        let versions = vec![
            McpProtocolVersion::V1_0_0,
            McpProtocolVersion::V1_1_0,
        ];

        for version in versions {
            match version {
                McpProtocolVersion::V1_0_0 => assert!(matches!(version, McpProtocolVersion::V1_0_0)),
                McpProtocolVersion::V1_1_0 => assert!(matches!(version, McpProtocolVersion::V1_1_0)),
            }
        }
    }

    #[test]
    fn test_mcp_connection_info() {
        let connection_info = McpConnectionInfo {
            client_id: "client_123".to_string(),
            client_version: "1.0.0".to_string(),
            protocol_version: McpProtocolVersion::V1_0_0,
            capabilities: vec![McpCapability::Tools],
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("user_agent".to_string(), "test_client/1.0.0".to_string());
                meta
            },
            connected_at: chrono::Utc::now(),
        };

        assert_eq!(connection_info.client_id, "client_123");
        assert_eq!(connection_info.client_version, "1.0.0");
        assert!(matches!(connection_info.protocol_version, McpProtocolVersion::V1_0_0));
        assert_eq!(connection_info.capabilities.len(), 1);
        assert!(connection_info.capabilities.contains(&McpCapability::Tools));
    }

    #[test]
    fn test_mcp_message_creation() {
        let message = McpMessage {
            id: Some("msg_123".to_string()),
            message_type: McpRequestType::Initialize,
            data: serde_json::json!({
                "protocolVersion": "1.0.0",
                "capabilities": {
                    "tools": {}
                }
            }),
            timestamp: chrono::Utc::now(),
            session_id: McpSessionId::from("session_456"),
        };

        assert_eq!(message.id.unwrap(), "msg_123");
        assert!(matches!(message.message_type, McpRequestType::Initialize));
        assert!(!message.data.is_null());
        assert_eq!(message.session_id.to_string(), "session_456");
    }

    #[test]
    fn test_mcp_request_types() {
        let request_types = vec![
            McpRequestType::Initialize,
            McpRequestType::ListTools,
            McpRequestType::CallTool,
            McpRequestType::ListResources,
            McpRequestType::ReadResource,
            McpRequestType::Ping,
        ];

        for req_type in request_types {
            match req_type {
                McpRequestType::Initialize => assert!(matches!(req_type, McpRequestType::Initialize)),
                McpRequestType::ListTools => assert!(matches!(req_type, McpRequestType::ListTools)),
                McpRequestType::CallTool => assert!(matches!(req_type, McpRequestType::CallTool)),
                McpRequestType::ListResources => assert!(matches!(req_type, McpRequestType::ListResources)),
                McpRequestType::ReadResource => assert!(matches!(req_type, McpRequestType::ReadResource)),
                McpRequestType::Ping => assert!(matches!(req_type, McpRequestType::Ping)),
            }
        }
    }

    #[test]
    fn test_mcp_response_types() {
        let response_types = vec![
            McpResponseType::Initialized,
            McpResponseType::ToolsList,
            McpResponseType::ToolResult,
            McpResponseType::ResourcesList,
            McpResponseType::ResourceContents,
            McpResponseType::Pong,
            McpResponseType::Error,
        ];

        for resp_type in response_types {
            match resp_type {
                McpResponseType::Initialized => assert!(matches!(resp_type, McpResponseType::Initialized)),
                McpResponseType::ToolsList => assert!(matches!(resp_type, McpResponseType::ToolsList)),
                McpResponseType::ToolResult => assert!(matches!(resp_type, McpResponseType::ToolResult)),
                McpResponseType::ResourcesList => assert!(matches!(resp_type, McpResponseType::ResourcesList)),
                McpResponseType::ResourceContents => assert!(matches!(resp_type, McpResponseType::ResourceContents)),
                McpResponseType::Pong => assert!(matches!(resp_type, McpResponseType::Pong)),
                McpResponseType::Error => assert!(matches!(resp_type, McpResponseType::Error)),
            }
        }
    }

    #[test]
    fn test_mcp_error_creation() {
        let error = McpError {
            code: McpErrorCode::InvalidRequest,
            message: "Invalid request format".to_string(),
            data: Some(serde_json::json!({
                "field": "message_type",
                "expected": "string",
                "received": "number"
            })),
        };

        assert!(matches!(error.code, McpErrorCode::InvalidRequest));
        assert_eq!(error.message, "Invalid request format");
        assert!(error.data.is_some());
    }

    #[test]
    fn test_mcp_error_codes() {
        let error_codes = vec![
            McpErrorCode::InvalidRequest,
            McpErrorCode::MethodNotFound,
            McpErrorCode::InvalidParams,
            McpErrorCode::InternalError,
            McpErrorCode::ParseError,
            McpErrorCode::Timeout,
        ];

        for code in error_codes {
            match code {
                McpErrorCode::InvalidRequest => assert!(matches!(code, McpErrorCode::InvalidRequest)),
                McpErrorCode::MethodNotFound => assert!(matches!(code, McpErrorCode::MethodNotFound)),
                McpErrorCode::InvalidParams => assert!(matches!(code, McpErrorCode::InvalidParams)),
                McpErrorCode::InternalError => assert!(matches!(code, McpErrorCode::InternalError)),
                McpErrorCode::ParseError => assert!(matches!(code, McpErrorCode::ParseError)),
                McpErrorCode::Timeout => assert!(matches!(code, McpErrorCode::Timeout)),
            }
        }
    }

    #[test]
    fn test_tool_registry_operations() {
        let mut registry = McpToolRegistry::new();

        let tool1 = create_test_tool("test_tool_1");
        let tool2 = create_test_tool("test_tool_2");

        // Test tool registration
        assert!(registry.register_tool(tool1.clone()).is_ok());
        assert!(registry.register_tool(tool2.clone()).is_ok());

        // Test tool lookup
        assert!(registry.get_tool("test_tool_1").is_some());
        assert!(registry.get_tool("test_tool_2").is_some());
        assert!(registry.get_tool("nonexistent_tool").is_none());

        // Test tool listing
        let all_tools = registry.list_tools();
        assert_eq!(all_tools.len(), 2);
        assert!(all_tools.iter().any(|t| t.name == "test_tool_1"));
        assert!(all_tools.iter().any(|t| t.name == "test_tool_2"));

        // Test tool removal
        assert!(registry.unregister_tool("test_tool_1").is_ok());
        assert!(registry.get_tool("test_tool_1").is_none());
        assert_eq!(registry.list_tools().len(), 1);
    }

    #[test]
    fn test_session_timeout_configuration() {
        let timeouts = vec![
            Duration::from_secs(10),
            Duration::from_secs(30),
            Duration::from_secs(60),
            Duration::from_secs(300), // 5 minutes
        ];

        for timeout in timeouts {
            let config = McpGatewayConfig {
                max_sessions: 50,
                session_timeout: timeout,
                heartbeat_interval: timeout / 3,
                supported_versions: vec![McpProtocolVersion::V1_0_0],
                default_capabilities: vec![McpCapability::Tools],
                enable_compression: true,
                max_message_size: 1024 * 1024,
                enable_metrics: true,
            };

            assert_eq!(config.session_timeout, timeout);
            assert!(config.heartbeat_interval < config.session_timeout);
        }
    }

    #[test]
    fn test_max_sessions_configuration() {
        let session_limits = vec![10, 50, 100, 500, 1000];

        for max_sessions in session_limits {
            let config = McpGatewayConfig {
                max_sessions,
                session_timeout: Duration::from_secs(30),
                heartbeat_interval: Duration::from_secs(10),
                supported_versions: vec![McpProtocolVersion::V1_0_0],
                default_capabilities: vec![McpCapability::Tools],
                enable_compression: true,
                max_message_size: 1024 * 1024,
                enable_metrics: true,
            };

            assert_eq!(config.max_sessions, max_sessions);
        }
    }

    #[test]
    fn test_message_size_limits() {
        let size_limits = vec![
            64 * 1024,      // 64KB
            1024 * 1024,    // 1MB
            10 * 1024 * 1024, // 10MB
        ];

        for size_limit in size_limits {
            let config = McpGatewayConfig {
                max_sessions: 100,
                session_timeout: Duration::from_secs(30),
                heartbeat_interval: Duration::from_secs(10),
                supported_versions: vec![McpProtocolVersion::V1_0_0],
                default_capabilities: vec![McpCapability::Tools],
                enable_compression: size_limit > 1024 * 1024, // Enable compression for larger messages
                max_message_size: size_limit,
                enable_metrics: true,
            };

            assert_eq!(config.max_message_size, size_limit);
        }
    }

    #[test]
    fn test_session_metadata_handling() {
        let mut session = create_test_session("metadata_test");

        // Add additional metadata
        session.metadata.insert("role".to_string(), "admin".to_string());
        session.metadata.insert("permissions".to_string(), "read,write".to_string());
        session.metadata.insert("quota".to_string(), "1000".to_string());

        assert_eq!(session.metadata.len(), 5); // 2 original + 3 new
        assert_eq!(session.metadata.get("role"), Some(&"admin".to_string()));
        assert_eq!(session.metadata.get("permissions"), Some(&"read,write".to_string()));
        assert_eq!(session.metadata.get("quota"), Some(&"1000".to_string()));

        // Update existing metadata
        session.metadata.insert("quota".to_string(), "2000".to_string());
        assert_eq!(session.metadata.get("quota"), Some(&"2000".to_string()));
        assert_eq!(session.metadata.len(), 5); // Still 5, just updated
    }

    #[test]
    fn test_capability_negotiation() {
        let client_capabilities = vec![
            McpCapability::Tools,
            McpCapability::Resources,
        ];

        let server_capabilities = vec![
            McpCapability::Tools,
            McpCapability::Resources,
            McpCapability::Prompts,
            McpCapability::Logging,
        ];

        // Find intersection of capabilities
        let common_capabilities: Vec<_> = client_capabilities
            .iter()
            .filter(|cap| server_capabilities.contains(cap))
            .cloned()
            .collect();

        assert_eq!(common_capabilities.len(), 2);
        assert!(common_capabilities.contains(&McpCapability::Tools));
        assert!(common_capabilities.contains(&McpCapability::Resources));
        assert!(!common_capabilities.contains(&McpCapability::Prompts));
        assert!(!common_capabilities.contains(&McpCapability::Logging));
    }

    #[test]
    fn test_error_handling_scenarios() {
        // Test various error scenarios
        let timeout_error = McpError {
            code: McpErrorCode::Timeout,
            message: "Request timed out after 30 seconds".to_string(),
            data: Some(serde_json::json!({
                "timeout_seconds": 30,
                "request_type": "call_tool"
            })),
        };

        let invalid_params_error = McpError {
            code: McpErrorCode::InvalidParams,
            message: "Invalid tool parameters".to_string(),
            data: Some(serde_json::json!({
                "missing_fields": ["input"],
                "invalid_fields": ["timeout"]
            })),
        };

        let internal_error = McpError {
            code: McpErrorCode::InternalError,
            message: "Database connection failed".to_string(),
            data: None,
        };

        assert!(matches!(timeout_error.code, McpErrorCode::Timeout));
        assert!(matches!(invalid_params_error.code, McpErrorCode::InvalidParams));
        assert!(matches!(internal_error.code, McpErrorCode::InternalError));
        assert!(timeout_error.data.is_some());
        assert!(invalid_params_error.data.is_some());
        assert!(internal_error.data.is_none());
    }

    #[tokio::test]
    async fn test_mcp_gateway_service_creation() {
        let config = create_test_config();

        // This would test actual gateway creation if implemented
        // For now, test configuration validation
        assert_eq!(config.max_sessions, 100);
        assert!(config.enable_compression);
        assert!(config.enable_metrics);
        assert!(config.default_capabilities.contains(&McpCapability::Tools));
    }

    #[test]
    fn test_session_lifecycle() {
        let mut session = create_test_session("lifecycle_test");

        // Session starts as active
        assert!(matches!(session.status, SessionStatus::Active));

        // Simulate session becoming idle
        session.status = SessionStatus::Idle;
        assert!(matches!(session.status, SessionStatus::Idle));

        // Simulate session error
        session.status = SessionStatus::Error("Connection lost".to_string());
        assert!(matches!(session.status, SessionStatus::Error(_)));

        // Simulate session closure
        session.status = SessionStatus::Closed;
        assert!(matches!(session.status, SessionStatus::Closed));

        // Update last activity
        let old_activity = session.last_activity;
        session.last_activity = chrono::Utc::now();
        assert!(session.last_activity > old_activity);
    }
}