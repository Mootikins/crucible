//! Test fixtures and sample data for plugin event subscription tests

use super::*;
use crate::plugin_events::types::*;
use crate::plugin_events::config::*;
use crate::events::{DaemonEvent, EventPriority, EventType, EventSource};
use std::collections::HashMap;
use uuid::Uuid;

/// Collection of test fixtures for various scenarios
pub struct TestFixtures;

impl TestFixtures {
    // ==================== SUBSCRIPTION FIXTURES ====================

    /// Create a basic real-time subscription
    pub fn basic_realtime_subscription() -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            "test-plugin-1".to_string(),
            vec![EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec!["system".to_string(), "service".to_string()],
                categories: vec![],
                sources: vec![],
                max_priority: Some(EventPriority::Normal),
            }],
        );

        SubscriptionConfig::new(
            "test-plugin-1".to_string(),
            "Basic Real-time Subscription".to_string(),
            SubscriptionType::Realtime,
            auth_context,
        )
    }

    /// Create a batched subscription
    pub fn batched_subscription() -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            "test-plugin-2".to_string(),
            vec![EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec![],
                categories: vec!["Filesystem".to_string()],
                sources: vec!["file-watcher".to_string()],
                max_priority: Some(EventPriority::High),
            }],
        );

        SubscriptionConfig::new(
            "test-plugin-2".to_string(),
            "Batched File Events".to_string(),
            SubscriptionType::Batched {
                interval_seconds: 5,
                max_batch_size: 100,
            },
            auth_context,
        )
        .with_delivery_options(DeliveryOptions {
            ack_enabled: true,
            max_retries: 5,
            retry_backoff: RetryBackoff::Exponential { base_ms: 500, max_ms: 10000 },
            compression_enabled: true,
            compression_threshold: 2048,
            encryption_enabled: false,
            max_event_size: 5 * 1024 * 1024,
            ordering: EventOrdering::Fifo,
            backpressure_handling: BackpressureHandling::Buffer { max_size: 500 },
        })
    }

    /// Create a persistent subscription with high reliability
    pub fn persistent_subscription() -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            "critical-plugin".to_string(),
            vec![EventPermission {
                scope: PermissionScope::Global,
                event_types: vec![],
                categories: vec![],
                sources: vec![],
                max_priority: None,
            }],
        );

        SubscriptionConfig::new(
            "critical-plugin".to_string(),
            "Critical Events Persistent".to_string(),
            SubscriptionType::Persistent {
                max_stored_events: 10000,
                ttl: std::time::Duration::from_secs(86400), // 24 hours
            },
            auth_context,
        )
        .with_delivery_options(DeliveryOptions {
            ack_enabled: true,
            max_retries: 10,
            retry_backoff: RetryBackoff::Exponential { base_ms: 1000, max_ms: 60000 },
            compression_enabled: true,
            compression_threshold: 1024,
            encryption_enabled: true,
            max_event_size: 20 * 1024 * 1024,
            ordering: EventOrdering::Causal,
            backpressure_handling: BackpressureHandling::ApplyBackpressure,
        })
    }

    /// Create a conditional subscription
    pub fn conditional_subscription() -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            "smart-plugin".to_string(),
            vec![EventPermission {
                scope: PermissionScope::Service { service_id: "script-engine".to_string() },
                event_types: vec!["service".to_string()],
                categories: vec![],
                sources: vec![],
                max_priority: Some(EventPriority::High),
            }],
        );

        let fallback = Box::new(SubscriptionType::Realtime);
        SubscriptionConfig::new(
            "smart-plugin".to_string(),
            "Conditional Script Events".to_string(),
            SubscriptionType::Conditional {
                condition: "event.priority == 'Critical' || event.metadata.alert == 'true'".to_string(),
                fallback,
            },
            auth_context,
        )
    }

    /// Create a priority subscription
    pub fn priority_subscription() -> SubscriptionConfig {
        let auth_context = AuthContext::new(
            "alert-plugin".to_string(),
            vec![EventPermission {
                scope: PermissionScope::Global,
                event_types: vec!["system".to_string(), "security".to_string()],
                categories: vec![],
                sources: vec![],
                max_priority: Some(EventPriority::Critical),
            }],
        );

        let delivery_method = Box::new(SubscriptionType::Realtime);
        SubscriptionConfig::new(
            "alert-plugin".to_string(),
            "Priority Alert Events".to_string(),
            SubscriptionType::Priority {
                min_priority: EventPriority::High,
                delivery_method,
            },
            auth_context,
        )
        .with_delivery_options(DeliveryOptions {
            ack_enabled: true,
            max_retries: 1,
            retry_backoff: RetryBackoff::Fixed { delay_ms: 100 },
            compression_enabled: false,
            compression_threshold: 1024,
            encryption_enabled: true,
            max_event_size: 1024 * 1024,
            ordering: EventOrdering::Priority,
            backpressure_handling: BackpressureHandling::DropNewest,
        })
    }

    // ==================== EVENT FIXTURES ====================

    /// Create a variety of test events
    pub fn test_events() -> Vec<DaemonEvent> {
        vec![
            Self::system_startup_event(),
            Self::service_started_event(),
            Self::file_created_event(),
            Self::database_query_event(),
            Self::security_alert_event(),
            Self::custom_test_event(),
            Self::resource_warning_event(),
            Self::network_error_event(),
        ]
    }

    /// System startup event
    pub fn system_startup_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::System(crate::events::SystemEvent::Startup),
            source: EventSource {
                id: "daemon".to_string(),
                name: "Crucible Daemon".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Normal,
            correlation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap()),
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("startup_time".to_string(), "2.5s".to_string());
                map.insert("components_loaded".to_string(), "12".to_string());
                map
            },
        }
    }

    /// Service started event
    pub fn service_started_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440003").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:00:05Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::Service(crate::events::ServiceEvent::Started {
                service_id: "script-engine".to_string(),
                service_type: "ScriptEngine".to_string(),
            }),
            source: EventSource {
                id: "service-manager".to_string(),
                name: "Service Manager".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Normal,
            correlation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440002").unwrap()),
            causation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap()),
            metadata: {
                let mut map = HashMap::new();
                map.insert("startup_duration".to_string(), "1.2s".to_string());
                map.insert("port".to_string(), "8080".to_string());
                map
            },
        }
    }

    /// File created event
    pub fn file_created_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440004").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:01:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::Filesystem(crate::events::FilesystemEvent::Created {
                path: "/tmp/test-file.txt".to_string(),
                size: 1024,
                file_type: "regular".to_string(),
            }),
            source: EventSource {
                id: "file-watcher".to_string(),
                name: "File System Watcher".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Low,
            correlation_id: None,
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("extension".to_string(), "txt".to_string());
                map.insert("directory".to_string(), "/tmp".to_string());
                map
            },
        }
    }

    /// Database query event
    pub fn database_query_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440005").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:01:30Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::Database(crate::events::DatabaseEvent::Query {
                query: "SELECT * FROM users WHERE active = true".to_string(),
                duration_ms: 150,
                rows_affected: 42,
            }),
            source: EventSource {
                id: "database".to_string(),
                name: "PostgreSQL Database".to_string(),
                version: "15.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Normal,
            correlation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440006").unwrap()),
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("table".to_string(), "users".to_string());
                map.insert("query_type".to_string(), "SELECT".to_string());
                map
            },
        }
    }

    /// Security alert event
    pub fn security_alert_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440007").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:02:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::System(crate::events::SystemEvent::SecurityAlert {
                alert_type: "Unauthorized Access Attempt".to_string(),
                severity: "High".to_string(),
                source_ip: "192.168.1.100".to_string(),
                target: "/admin/api".to_string(),
            }),
            source: EventSource {
                id: "security-monitor".to_string(),
                name: "Security Monitor".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Critical,
            correlation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440008").unwrap()),
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("user_agent".to_string(), "curl/7.68.0".to_string());
                map.insert("request_id".to_string(), "req-12345".to_string());
                map.insert("alert".to_string(), "true".to_string());
                map
            },
        }
    }

    /// Custom test event
    pub fn custom_test_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440009").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:03:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::Custom("test.custom.event".to_string()),
            source: EventSource {
                id: "test-source".to_string(),
                name: "Test Event Generator".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::Normal,
            correlation_id: None,
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("test_type".to_string(), "unit_test".to_string());
                map.insert("category".to_string(), "testing".to_string());
                map
            },
        }
    }

    /// Resource warning event
    pub fn resource_warning_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440010").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:04:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::System(crate::events::SystemEvent::ResourceAlert {
                resource_type: "Memory".to_string(),
                current_usage: 85.5,
                threshold: 80.0,
                unit: "percent".to_string(),
            }),
            source: EventSource {
                id: "resource-monitor".to_string(),
                name: "Resource Monitor".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::High,
            correlation_id: None,
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("process_id".to_string(), "1234".to_string());
                map.insert("memory_mb".to_string(), "8192".to_string());
                map
            },
        }
    }

    /// Network error event
    pub fn network_error_event() -> DaemonEvent {
        DaemonEvent {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440011").unwrap(),
            timestamp: chrono::DateTime::parse_from_rfc3339("2024-01-01T10:05:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            event_type: EventType::External(crate::events::ExternalEvent::Webhook {
                url: "https://api.example.com/webhook".to_string(),
                method: "POST".to_string(),
                status_code: Some(500),
            }),
            source: EventSource {
                id: "http-client".to_string(),
                name: "HTTP Client".to_string(),
                version: "1.0.0".to_string(),
                metadata: HashMap::new(),
            },
            priority: EventPriority::High,
            correlation_id: Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440012").unwrap()),
            causation_id: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("error_code".to_string(), "HTTP_500".to_string());
                map.insert("retry_count".to_string(), "3".to_string());
                map
            },
        }
    }

    // ==================== CONFIGURATION FIXTURES ====================

    /// Default system configuration for testing
    pub fn default_config() -> SubscriptionSystemConfig {
        SubscriptionSystemConfig::default()
    }

    /// High-performance configuration for testing
    pub fn performance_config() -> SubscriptionSystemConfig {
        let mut config = SubscriptionSystemConfig::default();

        // Optimize for performance
        config.manager.max_subscriptions = 10000;
        config.manager.subscription_cleanup_interval_seconds = 300;
        config.delivery.delivery_queue_size = 10000;
        config.delivery.batch_size = 1000;
        config.filtering.cache_size = 10000;
        config.filtering.compilation_cache_ttl_seconds = 3600;

        config
    }

    /// Security-focused configuration
    pub fn security_config() -> SubscriptionSystemConfig {
        let mut config = SubscriptionSystemConfig::default();

        // Enable security features
        config.security.enabled = true;
        config.security.authorization_required = true;
        config.security.encryption_required = true;
        config.security.max_event_size_bytes = 1024 * 1024; // 1MB
        config.security.rate_limit_per_minute = 1000;

        config
    }

    /// Development configuration for testing
    pub fn development_config() -> SubscriptionSystemConfig {
        let mut config = SubscriptionSystemConfig::default();

        // Development-friendly settings
        config.system.environment = "development".to_string();
        config.api.enabled = true;
        config.api.port = 9090;
        config.logging.level = "debug".to_string();
        config.logging.structured = false;
        config.metrics.enabled = true;
        config.metrics.collection_interval_seconds = 10;

        config
    }

    // ==================== PLUGIN FIXTURES ====================

    /// Create test plugin info
    pub fn test_plugin_info(plugin_id: &str, status: PluginStatus) -> PluginInfo {
        PluginInfo {
            plugin_id: plugin_id.to_string(),
            plugin_name: format!("Test Plugin {}", plugin_id),
            plugin_version: "1.0.0".to_string(),
            status,
            connected_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            capabilities: vec![
                "events".to_string(),
                "subscriptions".to_string(),
                "filters".to_string(),
            ],
            metadata: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), "true".to_string());
                map.insert("type".to_string(), "mock".to_string());
                map
            },
        }
    }

    /// Collection of test plugins with different statuses
    pub fn test_plugins() -> Vec<PluginInfo> {
        vec![
            Self::test_plugin_info("plugin-1", PluginStatus::Connected),
            Self::test_plugin_info("plugin-2", PluginStatus::Connected),
            Self::test_plugin_info("plugin-3", PluginStatus::Disconnected),
            Self::test_plugin_info("plugin-4", PluginStatus::Error {
                error: "Connection timeout".to_string(),
                last_error: chrono::Utc::now(),
            }),
        ]
    }

    // ==================== AUTHENTICATION FIXTURES ====================

    /// Create admin auth context
    pub fn admin_auth_context() -> AuthContext {
        AuthContext {
            principal: "admin".to_string(),
            permissions: vec![EventPermission {
                scope: PermissionScope::Global,
                event_types: vec![],
                categories: vec![],
                sources: vec![],
                max_priority: None,
            }],
            security_level: SecurityLevel::Critical,
            metadata: {
                let mut map = HashMap::new();
                map.insert("role".to_string(), "administrator".to_string());
                map
            },
        }
    }

    /// Create plugin auth context with limited permissions
    pub fn plugin_auth_context(plugin_id: &str) -> AuthContext {
        AuthContext {
            principal: plugin_id.to_string(),
            permissions: vec![EventPermission {
                scope: PermissionScope::Plugin,
                event_types: vec!["service".to_string(), "system".to_string()],
                categories: vec![],
                sources: vec![],
                max_priority: Some(EventPriority::Normal),
            }],
            security_level: SecurityLevel::Normal,
            metadata: {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "plugin".to_string());
                map
            },
        }
    }

    /// Create service-specific auth context
    pub fn service_auth_context(service_id: &str) -> AuthContext {
        AuthContext {
            principal: format!("service-{}", service_id),
            permissions: vec![EventPermission {
                scope: PermissionScope::Service {
                    service_id: service_id.to_string()
                },
                event_types: vec!["service".to_string()],
                categories: vec![],
                sources: vec![service_id.to_string()],
                max_priority: Some(EventPriority::High),
            }],
            security_level: SecurityLevel::High,
            metadata: {
                let mut map = HashMap::new();
                map.insert("type".to_string(), "service".to_string());
                map.insert("service_id".to_string(), service_id.to_string());
                map
            },
        }
    }
}

/// Performance test fixtures
pub struct PerformanceFixtures;

impl PerformanceFixtures {
    /// Generate large number of test events for performance testing
    pub fn generate_performance_events(count: usize) -> Vec<DaemonEvent> {
        (0..count)
            .map(|i| {
                let event_type = match i % 8 {
                    0 => EventType::System(crate::events::SystemEvent::Startup),
                    1 => EventType::Service(crate::events::ServiceEvent::Started {
                        service_id: format!("service-{}", i % 10),
                        service_type: "TestService".to_string(),
                    }),
                    2 => EventType::Filesystem(crate::events::FilesystemEvent::Created {
                        path: format!("/tmp/test-{}.txt", i),
                        size: 1024,
                        file_type: "regular".to_string(),
                    }),
                    3 => EventType::Database(crate::events::DatabaseEvent::Query {
                        query: format!("SELECT * FROM table_{} WHERE id = {}", i % 100, i),
                        duration_ms: 50 + (i % 200),
                        rows_affected: i % 50,
                    }),
                    4 => EventType::External(crate::events::ExternalEvent::Webhook {
                        url: format!("https://api.example.com/webhook/{}", i % 20),
                        method: "POST".to_string(),
                        status_code: Some(200),
                    }),
                    5 => EventType::Mcp(crate::events::McpEvent::ToolCall {
                        tool_name: format!("tool-{}", i % 15),
                        parameters: serde_json::json!({"id": i}),
                    }),
                    6 => EventType::System(crate::events::SystemEvent::ResourceAlert {
                        resource_type: "CPU".to_string(),
                        current_usage: 50.0 + (i as f64 % 50.0),
                        threshold: 80.0,
                        unit: "percent".to_string(),
                    }),
                    _ => EventType::Custom(format!("perf-event-{}", i)),
                };

                let priority = match i % 4 {
                    0 => EventPriority::Low,
                    1 => EventPriority::Normal,
                    2 => EventPriority::High,
                    _ => EventPriority::Critical,
                };

                DaemonEvent {
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now() + chrono::Duration::milliseconds(i as i64),
                    event_type,
                    source: EventSource {
                        id: format!("source-{}", i % 20),
                        name: format!("Performance Test Source {}", i % 20),
                        version: "1.0.0".to_string(),
                        metadata: HashMap::new(),
                    },
                    priority,
                    correlation_id: Some(Uuid::new_v4()),
                    causation_id: None,
                    metadata: {
                        let mut map = HashMap::new();
                        map.insert("test_id".to_string(), i.to_string());
                        map.insert("performance".to_string(), "true".to_string());
                        map
                    },
                }
            })
            .collect()
    }

    /// Generate multiple subscriptions for stress testing
    pub fn generate_performance_subscriptions(
        plugin_count: usize,
        subscriptions_per_plugin: usize,
    ) -> Vec<SubscriptionConfig> {
        let mut subscriptions = Vec::new();

        for plugin_idx in 0..plugin_count {
            let plugin_id = format!("perf-plugin-{}", plugin_idx);

            for sub_idx in 0..subscriptions_per_plugin {
                let subscription_type = match sub_idx % 4 {
                    0 => SubscriptionType::Realtime,
                    1 => SubscriptionType::Batched {
                        interval_seconds: 1 + (sub_idx % 10),
                        max_batch_size: 10 + (sub_idx % 100),
                    },
                    2 => SubscriptionType::Persistent {
                        max_stored_events: 1000 + (sub_idx * 100),
                        ttl: std::time::Duration::from_secs(3600 + (sub_idx as u64 * 600)),
                    },
                    _ => SubscriptionType::Priority {
                        min_priority: match sub_idx % 4 {
                            0 => EventPriority::Low,
                            1 => EventPriority::Normal,
                            2 => EventPriority::High,
                            _ => EventPriority::Critical,
                        },
                        delivery_method: Box::new(SubscriptionType::Realtime),
                    },
                };

                let auth_context = AuthContext::new(
                    plugin_id.clone(),
                    vec![EventPermission {
                        scope: PermissionScope::Plugin,
                        event_types: vec![],
                        categories: vec![],
                        sources: vec![],
                        max_priority: None,
                    }],
                );

                let subscription = SubscriptionConfig::new(
                    plugin_id.clone(),
                    format!("Performance Subscription {}", sub_idx),
                    subscription_type,
                    auth_context,
                );

                subscriptions.push(subscription);
            }
        }

        subscriptions
    }
}