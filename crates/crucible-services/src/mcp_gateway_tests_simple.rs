//! Unit tests for McpGateway service
//!
//! This module provides comprehensive unit tests for the McpGateway service,
//! covering all major functionality, edge cases, and error conditions.

use super::*;
use crate::events::mock::MockEventRouter;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Create a test MCP gateway with default configuration
async fn create_test_gateway() -> McpGateway {
    let config = McpGatewayConfig::default();
    let event_router = Arc::new(MockEventRouter::new());
    McpGateway::new(config, event_router).unwrap()
}

/// Create test client capabilities
fn create_test_client_capabilities() -> McpCapabilities {
    McpCapabilities {
        tools: Some(ToolCapabilities {
            list_tools: Some(true),
            call_tool: Some(true),
            subscribe_to_tools: Some(false),
        }),
        resources: Some(ResourceCapabilities {
            subscribe_to_resources: Some(false),
            read_resource: Some(false),
            list_resources: Some(false),
        }),
        logging: Some(LoggingCapabilities {
            set_log_level: Some(false),
            get_log_messages: Some(false),
        }),
        sampling: Some(SamplingCapabilities {
            create_message: Some(false),
        }),
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
                "input": {"type": "string"}
            }
        }),
        category: Some("test".to_string()),
        version: Some("1.0.0".to_string()),
        author: Some("test".to_string()),
        tags: vec!["test".to_string()],
        enabled: true,
        parameters: vec![],
    }
}

#[cfg(test)]
mod mcp_gateway_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_creation_default_config() {
        let gateway = create_test_gateway().await;

        // Verify initial state
        assert!(!gateway.is_running());
        assert_eq!(gateway.service_name(), "mcp_gateway");
        assert_eq!(gateway.service_version(), env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_service_lifecycle_start_stop() {
        let mut gateway = create_test_gateway().await;

        // Initially not running
        assert!(!gateway.is_running());

        // Start the service
        gateway.start().await.unwrap();
        assert!(gateway.is_running());

        // Stop the service
        gateway.stop().await.unwrap();
        assert!(!gateway.is_running());
    }

    #[tokio::test]
    async fn test_service_restart() {
        let mut gateway = create_test_gateway().await;

        // Restart when not running
        gateway.restart().await.unwrap();
        assert!(gateway.is_running());

        // Restart when running
        gateway.restart().await.unwrap();
        assert!(gateway.is_running());
    }

    #[tokio::test]
    async fn test_service_metadata() {
        let gateway = create_test_gateway().await;

        assert_eq!(gateway.service_name(), "mcp_gateway");
        assert_eq!(gateway.service_version(), env!("CARGO_PKG_VERSION"));
    }
}

#[cfg(test)]
mod mcp_gateway_configuration_tests {
    use super::*;

    #[tokio::test]
    async fn test_get_configuration() {
        let gateway = create_test_gateway().await;
        let config = gateway.get_config().await.unwrap();

        // Should return the default configuration
        assert_eq!(config.max_sessions, 100);
        assert_eq!(config.session_timeout_seconds, 3600);
        assert_eq!(config.max_request_size, 10 * 1024 * 1024);
        assert!(config.enable_compression);
        assert!(!config.enable_encryption);
        assert_eq!(config.max_concurrent_executions, 50);
    }

    #[tokio::test]
    async fn test_validate_configuration_valid() {
        let gateway = create_test_gateway().await;

        let valid_config = McpGatewayConfig {
            max_sessions: 10,
            session_timeout_seconds: 300,
            max_request_size: 1024 * 1024,
            max_concurrent_executions: 5,
            ..Default::default()
        };

        let result = gateway.validate_config(&valid_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_configuration_invalid() {
        let gateway = create_test_gateway().await;

        // Invalid: zero max sessions
        let invalid_config1 = McpGatewayConfig {
            max_sessions: 0,
            ..Default::default()
        };
        let result = gateway.validate_config(&invalid_config1).await;
        assert!(result.is_err());

        // Invalid: zero session timeout
        let invalid_config2 = McpGatewayConfig {
            session_timeout_seconds: 0,
            ..Default::default()
        };
        let result = gateway.validate_config(&invalid_config2).await;
        assert!(result.is_err());

        // Invalid: zero max request size
        let invalid_config3 = McpGatewayConfig {
            max_request_size: 0,
            ..Default::default()
        };
        let result = gateway.validate_config(&invalid_config3).await;
        assert!(result.is_err());

        // Invalid: zero concurrent executions
        let invalid_config4 = McpGatewayConfig {
            max_concurrent_executions: 0,
            ..Default::default()
        };
        let result = gateway.validate_config(&invalid_config4).await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod mcp_gateway_health_tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_not_running() {
        let gateway = create_test_gateway().await;

        let health = gateway.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Unhealthy));
        assert!(health.message.is_some());
    }

    #[tokio::test]
    async fn test_health_check_running_healthy() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let health = gateway.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));
        assert!(health.message.is_some());

        // Check expected details
        assert!(health.details.contains_key("active_sessions"));
        assert!(health.details.contains_key("active_executions"));
        assert!(health.details.contains_key("registered_tools"));
    }
}

#[cfg(test)]
mod mcp_gateway_session_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_initialize_connection() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let capabilities = create_test_client_capabilities();
        let session = gateway.initialize_connection("test_client", capabilities).await.unwrap();

        assert_eq!(session.client_id, "test_client");
        assert_eq!(session.status, McpSessionStatus::Active);
        assert!(!session.session_id.is_empty());
        assert!(session.created_at <= chrono::Utc::now());
        assert!(session.last_activity <= chrono::Utc::now());
    }

    #[tokio::test]
    async fn test_close_connection() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let capabilities = create_test_client_capabilities();
        let session = gateway.initialize_connection("test_client", capabilities).await.unwrap();

        // Close the session
        gateway.close_connection(&session.session_id).await.unwrap();

        // Verify session is closed
        let connections = gateway.list_connections().await.unwrap();
        assert!(connections.is_empty());
    }

    #[tokio::test]
    async fn test_close_nonexistent_connection() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let result = gateway.close_connection("nonexistent_session").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ValidationError(_)));
    }

    #[tokio::test]
    async fn test_list_connections_empty() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let connections = gateway.list_connections().await.unwrap();
        assert!(connections.is_empty());
    }

    #[tokio::test]
    async fn test_list_connections_with_sessions() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let capabilities = create_test_client_capabilities();
        let session1 = gateway.initialize_connection("client1", capabilities.clone()).await.unwrap();
        let session2 = gateway.initialize_connection("client2", capabilities).await.unwrap();

        let connections = gateway.list_connections().await.unwrap();
        assert_eq!(connections.len(), 2);

        let session_ids: Vec<String> = connections.iter().map(|s| s.session_id.clone()).collect();
        assert!(session_ids.contains(&session1.session_id));
        assert!(session_ids.contains(&session2.session_id));
    }
}

#[cfg(test)]
mod mcp_gateway_tool_management_tests {
    use super::*;

    #[tokio::test]
    async fn test_register_tool() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tool = create_test_tool("test_tool");
        let result = gateway.register_tool(tool.clone()).await;
        assert!(result.is_ok());

        // Verify tool is registered
        let tools = gateway.list_tools().await.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, tool.name);
    }

    #[tokio::test]
    async fn test_register_duplicate_tool() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tool = create_test_tool("duplicate_tool");
        gateway.register_tool(tool.clone()).await.unwrap();

        // Registering the same tool again should fail
        let result = gateway.register_tool(tool).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ValidationError(_)));
    }

    #[tokio::test]
    async fn test_unregister_tool() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tool = create_test_tool("unregister_test");
        gateway.register_tool(tool.clone()).await.unwrap();

        // Unregister the tool
        let result = gateway.unregister_tool(&tool.name).await;
        assert!(result.is_ok());

        // Verify tool is gone
        let tools = gateway.list_tools().await.unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_unregister_nonexistent_tool() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let result = gateway.unregister_tool("nonexistent_tool").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ServiceError::ValidationError(_)));
    }

    #[tokio::test]
    async fn test_list_tools_empty() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tools = gateway.list_tools().await.unwrap();
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn test_list_tools_with_registered() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tool1 = create_test_tool("tool1");
        let tool2 = create_test_tool("tool2");

        gateway.register_tool(tool1).await.unwrap();
        gateway.register_tool(tool2).await.unwrap();

        let tools = gateway.list_tools().await.unwrap();
        assert_eq!(tools.len(), 2);
    }

    #[tokio::test]
    async fn test_get_tool() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let tool = create_test_tool("get_test");
        gateway.register_tool(tool.clone()).await.unwrap();

        let retrieved = gateway.get_tool(&tool.name).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, tool.name);
    }

    #[tokio::test]
    async fn test_get_nonexistent_tool() {
        let gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        let retrieved = gateway.get_tool("nonexistent_tool").await.unwrap();
        assert!(retrieved.is_none());
    }
}

#[cfg(test)]
mod mcp_gateway_integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_end_to_end_workflow() {
        let mut gateway = create_test_gateway().await;

        // Start the service
        gateway.start().await.unwrap();
        assert!(gateway.is_running());

        // Check health
        let health = gateway.health_check().await.unwrap();
        assert!(matches!(health.status, ServiceStatus::Healthy));

        // Register tools
        let tool1 = create_test_tool("tool1");
        let tool2 = create_test_tool("tool2");
        gateway.register_tool(tool1).await.unwrap();
        gateway.register_tool(tool2).await.unwrap();

        // Create a session
        let capabilities = create_test_client_capabilities();
        let session = gateway.initialize_connection("test_client", capabilities).await.unwrap();
        assert_eq!(session.client_id, "test_client");
        assert_eq!(session.status, McpSessionStatus::Active);

        // Close the session
        gateway.close_connection(&session.session_id).await.unwrap();

        // Stop the service
        gateway.stop().await.unwrap();
        assert!(!gateway.is_running());
    }

    #[tokio::test]
    async fn test_graceful_shutdown_cleanup() {
        let mut gateway = create_test_gateway().await;
        gateway.start().await.unwrap();

        // Create sessions
        let capabilities = create_test_client_capabilities();
        let session1 = gateway.initialize_connection("client1", capabilities.clone()).await.unwrap();
        let session2 = gateway.initialize_connection("client2", capabilities).await.unwrap();

        // Stop the service (should clean up gracefully)
        gateway.stop().await.unwrap();
        assert!(!gateway.is_running());

        // Verify cleanup
        let connections = gateway.list_connections().await.unwrap();
        assert!(connections.is_empty());
    }
}