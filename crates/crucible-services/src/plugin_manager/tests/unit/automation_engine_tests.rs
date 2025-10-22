//! # Plugin Automation Engine Tests
//!
//! Comprehensive unit tests for the plugin automation engine.
//! Tests cover rule creation, trigger evaluation, action execution,
//! event-driven automation, and performance under various loads.

use super::*;
use crate::plugin_manager::automation_engine::*;
use crate::plugin_manager::lifecycle_manager::*;
use crate::plugin_manager::state_machine::*;
use crate::plugin_manager::dependency_resolver::*;
use crate::plugin_manager::lifecycle_policy::*;
use crate::plugin_manager::types::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

#[cfg(test)]
mod automation_engine_tests {
    use super::*;

    // ============================================================================
    // BASIC AUTOMATION ENGINE FUNCTIONALITY TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_automation_engine_creation() {
        let lifecycle_manager = Arc::new(create_mock_lifecycle_manager());
        let policy_engine = Arc::new(create_test_lifecycle_policy());
        let dependency_resolver = Arc::new(create_test_dependency_resolver());
        let state_machine = Arc::new(create_test_state_machine());

        let engine = AutomationEngine::new(
            lifecycle_manager.clone(),
            policy_engine.clone(),
            dependency_resolver.clone(),
            state_machine.clone(),
        );

        // Verify initial state
        let rules = engine.list_rules().await.unwrap();
        assert_eq!(rules.len(), 1); // Default auto-restart rule

        let metrics = engine.get_metrics().await.unwrap();
        assert_eq!(metrics.total_rules, 1);
        assert_eq!(metrics.total_executions, 0);
    }

    #[tokio::test]
    async fn test_automation_engine_initialization() {
        let lifecycle_manager = Arc::new(create_mock_lifecycle_manager());
        let policy_engine = Arc::new(create_test_lifecycle_policy());
        let dependency_resolver = Arc::new(create_test_dependency_resolver());
        let state_machine = Arc::new(create_test_state_machine());

        let engine = AutomationEngine::new(
            lifecycle_manager,
            policy_engine,
            dependency_resolver,
            state_machine,
        );

        // Initialize engine
        let result = engine.initialize().await;
        assert!(result.is_ok());

        // Verify initialization
        let rules = engine.list_rules().await.unwrap();
        assert!(rules.len() >= 1); // Should have default rules
    }

    // ============================================================================
    // AUTOMATION RULE CREATION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_create_automation_rule() {
        let engine = create_test_automation_engine();

        let rule = create_test_automation_rule("test-rule", "Test Rule");

        // Add trigger to rule
        let rule_with_trigger = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "health-trigger".to_string(),
                    trigger_type: TriggerType::Health,
                    config: TriggerConfig {
                        health_config: Some(HealthTriggerConfig {
                            health_status: PluginHealthStatus::Unhealthy,
                            consecutive_failures: 3,
                            time_window: Duration::from_secs(300),
                            instance_filter: None,
                        }),
                        event_config: None,
                        time_config: None,
                        state_config: None,
                        performance_config: None,
                        resource_config: None,
                        webhook_config: None,
                        custom_config: None,
                    },
                    enabled: true,
                    cooldown: Some(Duration::from_secs(60)),
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "restart-action".to_string(),
                    action_type: ActionType::RestartPlugin,
                    config: ActionConfig {
                        lifecycle_config: Some(LifecycleActionConfig {
                            target_instances: vec!["{{instance_id}}".to_string()],
                            target_plugins: vec![],
                            operation: LifecycleOperation::Restart { instance_id: "{{instance_id}}".to_string() },
                            parameters: HashMap::new(),
                        }),
                        script_config: None,
                        http_config: None,
                        notification_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(60)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        // Create rule
        let result = engine.add_rule(rule_with_trigger.clone()).await;
        assert!(result.is_ok());

        // Verify rule was created
        let retrieved_rule = engine.get_rule("test-rule").await.unwrap();
        assert!(retrieved_rule.is_some());
        assert_eq!(retrieved_rule.unwrap().name, "Test Rule");

        let metrics = engine.get_metrics().await.unwrap();
        assert_eq!(metrics.total_rules, 2); // Default rule + new rule
    }

    #[tokio::test]
    async fn test_rule_validation() {
        let engine = create_test_automation_engine();

        // Test invalid rule (empty ID)
        let invalid_rule = AutomationRule {
            id: "".to_string(), // Invalid
            name: "Invalid Rule".to_string(),
            description: "Rule with empty ID".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            priority: AutomationPriority::Normal,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            scope: AutomationScope::default(),
            schedule: None,
            limits: None,
            metadata: AutomationMetadata::default(),
        };

        let result = engine.add_rule(invalid_rule);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::automation(_)));

        // Test invalid rule (no triggers)
        let no_triggers_rule = AutomationRule {
            id: "no-triggers".to_string(),
            name: "No Triggers Rule".to_string(),
            description: "Rule with no triggers".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            priority: AutomationPriority::Normal,
            triggers: vec![], // Invalid - no triggers
            conditions: vec![],
            actions: vec![AutomationAction {
                id: "action".to_string(),
                action_type: ActionType::StopPlugin,
                config: ActionConfig::default(),
                timeout: None,
                retry_config: None,
                order: 1,
                parallel: false,
            }],
            scope: AutomationScope::default(),
            schedule: None,
            limits: None,
            metadata: AutomationMetadata::default(),
        };

        let result = engine.add_rule(no_triggers_rule);
        assert!(result.is_err());

        // Test invalid rule (no actions)
        let no_actions_rule = AutomationRule {
            id: "no-actions".to_string(),
            name: "No Actions Rule".to_string(),
            description: "Rule with no actions".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            priority: AutomationPriority::Normal,
            triggers: vec![AutomationTrigger {
                id: "trigger".to_string(),
                trigger_type: TriggerType::Event,
                config: TriggerConfig::default(),
                enabled: true,
                cooldown: None,
                last_triggered: None,
            }],
            conditions: vec![],
            actions: vec![], // Invalid - no actions
            scope: AutomationScope::default(),
            schedule: None,
            limits: None,
            metadata: AutomationMetadata::default(),
        };

        let result = engine.add_rule(no_actions_rule);
        assert!(result.is_err());
    }

    // ============================================================================
    // TRIGGER EVALUATION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_health_trigger_evaluation() {
        let engine = create_test_automation_engine();

        // Create rule with health trigger
        let rule = create_test_automation_rule("health-rule", "Health Rule");
        let rule_with_health_trigger = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "health-trigger".to_string(),
                    trigger_type: TriggerType::Health,
                    config: TriggerConfig {
                        health_config: Some(HealthTriggerConfig {
                            health_status: PluginHealthStatus::Unhealthy,
                            consecutive_failures: 3,
                            time_window: Duration::from_secs(300),
                            instance_filter: Some("web-server-*".to_string()),
                        }),
                        event_config: None,
                        time_config: None,
                        state_config: None,
                        performance_config: None,
                        resource_config: None,
                        webhook_config: None,
                        custom_config: None,
                    },
                    enabled: true,
                    cooldown: Some(Duration::from_secs(300)),
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "restart-action".to_string(),
                    action_type: ActionType::RestartPlugin,
                    config: ActionConfig {
                        lifecycle_config: Some(LifecycleActionConfig {
                            target_instances: vec!["{{instance_id}}".to_string()],
                            target_plugins: vec![],
                            operation: LifecycleOperation::Restart { instance_id: "{{instance_id}}".to_string() },
                            parameters: HashMap::new(),
                        }),
                        script_config: None,
                        http_config: None,
                        notification_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(60)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_health_trigger).await.unwrap();

        // Create health event
        let health_event = AutomationEvent {
            event_id: "health-event-1".to_string(),
            event_type: "health_status_change".to_string(),
            source: "health_monitor".to_string(),
            timestamp: SystemTime::now(),
            data: HashMap::from([
                ("instance_id".to_string(), serde_json::Value::String("web-server-1".to_string())),
                ("old_status".to_string(), serde_json::Value::String("Healthy".to_string())),
                ("new_status".to_string(), serde_json::Value::String("Unhealthy".to_string())),
                ("consecutive_failures".to_string(), serde_json::Value::Number(3.into())),
            ]),
            severity: AutomationEventSeverity::High,
        };

        // Process event
        let result = engine.process_event(health_event).await;
        assert!(result.is_ok());

        // Wait for rule evaluation (async processing)
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check execution history
        let history = engine.get_execution_history(Some("health-rule"), Some(10)).await.unwrap();
        // Note: This test depends on the actual trigger implementation
    }

    #[tokio::test]
    async fn test_state_trigger_evaluation() {
        let engine = create_test_automation_engine();

        // Create rule with state trigger
        let rule = create_test_automation_rule("state-rule", "State Rule");
        let rule_with_state_trigger = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "state-trigger".to_string(),
                    trigger_type: TriggerType::State,
                    config: TriggerConfig {
                        state_config: Some(StateTriggerConfig {
                            target_states: vec![PluginInstanceState::Error],
                            instance_filter: Some("critical-*".to_string()),
                            plugin_filter: None,
                            duration_threshold: Some(Duration::from_secs(30)),
                        }),
                        event_config: None,
                        time_config: None,
                        health_config: None,
                        performance_config: None,
                        resource_config: None,
                        webhook_config: None,
                        custom_config: None,
                    },
                    enabled: true,
                    cooldown: Some(Duration::from_secs(600)),
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "notify-action".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Slack,
                            channels: vec!["#alerts".to_string()],
                            message_template: "Plugin {{plugin_id}} entered error state".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::High,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_state_trigger).await.unwrap();

        // Create state change event
        let state_event = AutomationEvent {
            event_id: "state-event-1".to_string(),
            event_type: "state_change".to_string(),
            source: "state_machine".to_string(),
            timestamp: SystemTime::now(),
            data: HashMap::from([
                ("instance_id".to_string(), serde_json::Value::String("critical-service-1".to_string())),
                ("old_state".to_string(), serde_json::Value::String("Running".to_string())),
                ("new_state".to_string(), serde_json::Value::String("Error".to_string())),
                ("error_message".to_string(), serde_json::Value::String("Connection timeout".to_string())),
            ]),
            severity: AutomationEventSeverity::Critical,
        };

        // Process event
        let result = engine.process_event(state_event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_performance_trigger_evaluation() {
        let engine = create_test_automation_engine();

        // Create rule with performance trigger
        let rule = create_test_automation_rule("performance-rule", "Performance Rule");
        let rule_with_perf_trigger = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "perf-trigger".to_string(),
                    trigger_type: TriggerType::Performance,
                    config: TriggerConfig {
                        performance_config: Some(PerformanceTriggerConfig {
                            metric_name: "cpu_usage".to_string(),
                            operator: ComparisonOperator::GreaterThan,
                            threshold: 80.0,
                            duration: Duration::from_secs(300),
                            aggregation: AggregationType::Average,
                        }),
                        event_config: None,
                        time_config: None,
                        state_config: None,
                        health_config: None,
                        resource_config: None,
                        webhook_config: None,
                        custom_config: None,
                    },
                    enabled: true,
                    cooldown: Some(Duration::from_secs(900)),
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "scale-action".to_string(),
                    action_type: ActionType::ScalePlugin,
                    config: ActionConfig {
                        lifecycle_config: Some(LifecycleActionConfig {
                            target_instances: vec![],
                            target_plugins: vec!["{{plugin_id}}".to_string()],
                            operation: LifecycleOperation::Scale {
                                plugin_id: "{{plugin_id}}".to_string(),
                                target_instances: 3,
                            },
                            parameters: HashMap::new(),
                        }),
                        script_config: None,
                        http_config: None,
                        notification_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(120)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_perf_trigger).await.unwrap();

        // Create performance event
        let perf_event = AutomationEvent {
            event_id: "perf-event-1".to_string(),
            event_type: "performance_alert".to_string(),
            source: "metrics_collector".to_string(),
            timestamp: SystemTime::now(),
            data: HashMap::from([
                ("instance_id".to_string(), serde_json::Value::String("web-server-1".to_string())),
                ("plugin_id".to_string(), serde_json::Value::String("web-server".to_string())),
                ("metric_name".to_string(), serde_json::Value::String("cpu_usage".to_string())),
                ("metric_value".to_string(), serde_json::Value::Number(85.5.into())),
                ("threshold".to_string(), serde_json::Value::Number(80.0.into())),
                ("duration".to_string(), serde_json::Value::Number(300.into())),
            ]),
            severity: AutomationEventSeverity::High,
        };

        // Process event
        let result = engine.process_event(perf_event).await;
        assert!(result.is_ok());
    }

    // ============================================================================
    // RULE EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_rule_manual_trigger() {
        let engine = create_test_automation_engine();

        // Create simple rule
        let rule = create_test_automation_rule("manual-rule", "Manual Rule");
        let rule_with_action = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "manual-trigger".to_string(),
                    trigger_type: TriggerType::Manual,
                    config: TriggerConfig::default(),
                    enabled: true,
                    cooldown: None,
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "test-action".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Email,
                            channels: vec!["admin@example.com".to_string()],
                            message_template: "Manual rule triggered".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::Normal,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_action).await.unwrap();

        // Manually trigger rule
        let trigger_data = HashMap::from([
            ("reason".to_string(), serde_json::Value::String("Manual test".to_string())),
            ("user".to_string(), serde_json::Value::String("test-user".to_string())),
        ]);

        let execution_id = engine.trigger_rule("manual-rule", trigger_data).await.unwrap();

        // Verify execution was started
        assert!(!execution_id.is_empty());

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check execution history
        let history = engine.get_execution_history(Some("manual-rule"), Some(10)).await.unwrap();
        assert!(!history.is_empty());
    }

    #[tokio::test]
    async fn test_rule_condition_evaluation() {
        let engine = create_test_automation_engine();

        // Create rule with conditions
        let rule = create_test_automation_rule("conditional-rule", "Conditional Rule");
        let rule_with_conditions = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "event-trigger".to_string(),
                    trigger_type: TriggerType::Event,
                    config: TriggerConfig {
                        event_config: Some(EventTriggerConfig {
                            event_types: vec!["error".to_string()],
                            source_filter: None,
                            data_filters: HashMap::new(),
                        }),
                        event_config: None,
                        time_config: None,
                        state_config: None,
                        health_config: None,
                        performance_config: None,
                        resource_config: None,
                        webhook_config: None,
                        custom_config: None,
                    },
                    enabled: true,
                    cooldown: None,
                    last_triggered: None,
                },
            ],
            conditions: vec![
                crate::plugin_manager::automation_engine::AutomationCondition {
                    id: "business-hours-condition".to_string(),
                    condition_type: ConditionType::TimeWindow,
                    config: crate::plugin_manager::automation_engine::ConditionConfig {
                        expression_config: Some(crate::plugin_manager::automation_engine::ExpressionConditionConfig {
                            language: crate::plugin_manager::automation_engine::ExpressionLanguage::Cel,
                            expression: "hour(event.timestamp) >= 9 && hour(event.timestamp) <= 17".to_string(),
                            context_variables: HashMap::new(),
                        }),
                        script_config: None,
                        api_config: None,
                        custom_config: None,
                    },
                    negate: false,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "conditional-action".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Slack,
                            channels: vec!["#alerts".to_string()],
                            message_template: "Conditional action executed".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::Normal,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_conditions).await.unwrap();

        // Create event outside business hours
        let off_hours_event = AutomationEvent {
            event_id: "off-hours-event".to_string(),
            event_type: "error".to_string(),
            source: "application".to_string(),
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(86400 - 3600), // 11 PM
            data: HashMap::from([
                ("error_type".to_string(), serde_json::Value::String("connection_error".to_string())),
                ("severity".to_string(), serde_json::Value::String("high".to_string())),
            ]),
            severity: AutomationEventSeverity::High,
        };

        // Process event
        let result = engine.process_event(off_hours_event).await;
        assert!(result.is_ok());
    }

    // ============================================================================
    // PARALLEL ACTION EXECUTION TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_parallel_action_execution() {
        let engine = create_test_automation_engine();

        // Create rule with parallel actions
        let rule = create_test_automation_rule("parallel-rule", "Parallel Rule");
        let rule_with_parallel_actions = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "parallel-trigger".to_string(),
                    trigger_type: TriggerType::Manual,
                    config: TriggerConfig::default(),
                    enabled: true,
                    cooldown: None,
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "action-1".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Email,
                            channels: vec!["admin1@example.com".to_string()],
                            message_template: "Parallel action 1".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::Normal,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: true, // Execute in parallel
                },
                AutomationAction {
                    id: "action-2".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Email,
                            channels: vec!["admin2@example.com".to_string()],
                            message_template: "Parallel action 2".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::Normal,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: true, // Execute in parallel
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_parallel_actions).await.unwrap();

        // Trigger rule
        let trigger_data = HashMap::new();
        let execution_id = engine.trigger_rule("parallel-rule", trigger_data).await.unwrap();

        // Wait for execution to complete
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify execution
        let history = engine.get_execution_history(Some("parallel-rule"), Some(10)).await.unwrap();
        assert!(!history.is_empty());

        if let Some(execution) = history.first() {
            assert_eq!(execution.actions_executed.len(), 2);
        }
    }

    // ============================================================================
    // RULE LIMITS AND THROTTLING TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_rule_rate_limiting() {
        let engine = create_test_automation_engine();

        // Create rule with rate limits
        let rule = create_test_automation_rule("rate-limited-rule", "Rate Limited Rule");
        let rule_with_limits = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "rate-limit-trigger".to_string(),
                    trigger_type: TriggerType::Manual,
                    config: TriggerConfig::default(),
                    enabled: true,
                    cooldown: Some(Duration::from_millis(100)), // Short cooldown for testing
                    last_triggered: None,
                },
            ],
            limits: Some(AutomationLimits {
                max_executions_per_window: Some(2),
                execution_window: Duration::from_secs(1),
                max_concurrent_executions: Some(1),
                rate_limit: Some(RateLimit {
                    max_requests: 2,
                    period: Duration::from_secs(1),
                }),
            }),
            actions: vec![
                AutomationAction {
                    id: "limited-action".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig {
                        notification_config: Some(NotificationActionConfig {
                            notification_type: NotificationType::Email,
                            channels: vec!["admin@example.com".to_string()],
                            message_template: "Rate limited action".to_string(),
                            message_data: HashMap::new(),
                            priority: NotificationPriority::Normal,
                        }),
                        lifecycle_config: None,
                        script_config: None,
                        http_config: None,
                        custom_config: None,
                    },
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule_with_limits).await.unwrap();

        // Trigger rule multiple times rapidly
        let mut execution_ids = Vec::new();
        for _ in 0..5 {
            let trigger_data = HashMap::new();
            if let Ok(execution_id) = engine.trigger_rule("rate-limited-rule", trigger_data).await {
                execution_ids.push(execution_id);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Should have executed at most 2 times due to rate limiting
        let history = engine.get_execution_history(Some("rate-limited-rule"), Some(10)).await.unwrap();
        assert!(history.len() <= 2);
    }

    // ============================================================================
    // EVENT SYSTEM TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_automation_events() {
        let engine = create_test_automation_engine();

        // Subscribe to automation events
        let mut event_receiver = engine.subscribe_events().await;

        // Create and trigger a rule
        let rule = create_test_automation_rule("event-test-rule", "Event Test Rule");
        let rule_with_trigger = AutomationRule {
            triggers: vec![
                AutomationTrigger {
                    id: "event-test-trigger".to_string(),
                    trigger_type: TriggerType::Manual,
                    config: TriggerConfig::default(),
                    enabled: true,
                    cooldown: None,
                    last_triggered: None,
                },
            ],
            actions: vec![
                AutomationAction {
                    id: "event-test-action".to_string(),
                    action_type: ActionType::SendNotification,
                    config: ActionConfig::default(),
                    timeout: Some(Duration::from_secs(30)),
                    retry_config: None,
                    order: 1,
                    parallel: false,
                },
            ],
            ..rule
        };

        engine.add_rule(rule).await.unwrap();

        // Trigger rule
        let trigger_data = HashMap::new();
        let _execution_id = engine.trigger_rule("event-test-rule", trigger_data).await.unwrap();

        // Wait for events
        let mut events_received = Vec::new();
        for _ in 0..3 {
            if let Ok(event) = tokio::time::timeout(Duration::from_millis(100), event_receiver.recv()).await {
                if let Ok(event) = event {
                    events_received.push(event);
                }
            }
        }

        // Verify we received events
        assert!(!events_received.is_empty());

        // Check for expected event types
        let event_types: HashSet<String> = events_received.iter()
            .map(|e| format!("{:?}", e))
            .collect();

        // Should have execution started and possibly completion events
        assert!(event_types.iter().any(|e| e.contains("ExecutionStarted")));
    }

    // ============================================================================
    // PERFORMANCE TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_rule_evaluation_performance() {
        let engine = create_test_automation_engine();

        // Create many rules
        for i in 0..50 {
            let rule = create_test_automation_rule(&format!("perf-rule-{}", i), &format!("Performance Rule {}", i));
            let rule_with_trigger = AutomationRule {
                triggers: vec![
                    AutomationTrigger {
                        id: format!("trigger-{}", i),
                        trigger_type: TriggerType::Event,
                        config: TriggerConfig {
                            event_config: Some(EventTriggerConfig {
                                event_types: vec![format!("event-{}", i)],
                                source_filter: None,
                                data_filters: HashMap::new(),
                            }),
                            event_config: None,
                            time_config: None,
                            state_config: None,
                            health_config: None,
                            performance_config: None,
                            resource_config: None,
                            webhook_config: None,
                            custom_config: None,
                        },
                        enabled: true,
                        cooldown: None,
                        last_triggered: None,
                    },
                ],
                actions: vec![
                    AutomationAction {
                        id: format!("action-{}", i),
                        action_type: ActionType::SendNotification,
                        config: ActionConfig::default(),
                        timeout: Some(Duration::from_secs(30)),
                        retry_config: None,
                        order: 1,
                        parallel: false,
                    },
                ],
                ..rule
            };

            engine.add_rule(rule_with_trigger).await.unwrap();
        }

        // Benchmark event processing
        let test_event = AutomationEvent {
            event_id: "perf-test-event".to_string(),
            event_type: "test".to_string(),
            source: "performance-test".to_string(),
            timestamp: SystemTime::now(),
            data: HashMap::new(),
            severity: AutomationEventSeverity::Normal,
        };

        let (average_time, _) = benchmark_operation("event_processing", || async {
            let event = AutomationEvent {
                event_id: format!("event-{}", uuid::Uuid::new_v4()),
                event_type: "test".to_string(),
                source: "performance-test".to_string(),
                timestamp: SystemTime::now(),
                data: HashMap::new(),
                severity: AutomationEventSeverity::Normal,
            };
            let _ = engine.process_event(event).await;
        }, 100).await;

        // Verify performance targets (should be under 50ms per event with 50 rules)
        assert!(average_time < Duration::from_millis(50), "Event processing too slow: {:?}", average_time);
    }

    #[tokio::test]
    async fn test_concurrent_rule_executions() {
        let engine = Arc::new(create_test_automation_engine());

        // Create rules
        for i in 0..10 {
            let rule = create_test_automation_rule(&format!("concurrent-rule-{}", i), &format!("Concurrent Rule {}", i));
            let rule_with_trigger = AutomationRule {
                triggers: vec![
                    AutomationTrigger {
                        id: format!("trigger-{}", i),
                        trigger_type: TriggerType::Manual,
                        config: TriggerConfig::default(),
                        enabled: true,
                        cooldown: None,
                        last_triggered: None,
                    },
                ],
                actions: vec![
                    AutomationAction {
                        id: format!("action-{}", i),
                        action_type: ActionType::SendNotification,
                        config: ActionConfig::default(),
                        timeout: Some(Duration::from_secs(30)),
                        retry_config: None,
                        order: 1,
                        parallel: false,
                    },
                ],
                ..rule
            };

            engine.add_rule(rule_with_trigger).await.unwrap();
        }

        let mut handles = Vec::new();

        // Trigger multiple rules concurrently
        for i in 0..10 {
            let engine_clone = engine.clone();
            let handle = tokio::spawn(async move {
                let rule_id = format!("concurrent-rule-{}", i);
                let trigger_data = HashMap::new();
                let _ = engine_clone.trigger_rule(&rule_id, trigger_data).await;
            });
            handles.push(handle);
        }

        // Wait for all executions to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify executions
        let metrics = engine.get_metrics().await.unwrap();
        assert!(metrics.total_executions >= 10);
    }

    // ============================================================================
    // ERROR HANDLING TESTS
    // ============================================================================

    #[tokio::test]
    async fn test_automation_engine_error_handling() {
        let engine = create_test_automation_engine();

        // Test operations on non-existent rule
        let result = engine.get_rule("non-existent").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        let result = engine.trigger_rule("non-existent", HashMap::new()).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PluginError::automation(_)));

        let result = engine.remove_rule("non-existent").await;
        assert!(!result.unwrap());

        // Test invalid rule updates
        let valid_rule = create_test_automation_rule("valid-rule", "Valid Rule");
        engine.add_rule(valid_rule.clone()).await.unwrap();

        let invalid_update = AutomationRule {
            id: "valid-rule".to_string(),
            name: "".to_string(), // Invalid
            description: "Invalid update".to_string(),
            version: "1.0.0".to_string(),
            enabled: true,
            priority: AutomationPriority::Normal,
            triggers: vec![],
            conditions: vec![],
            actions: vec![],
            scope: AutomationScope::default(),
            schedule: None,
            limits: None,
            metadata: AutomationMetadata::default(),
        };

        let result = engine.update_rule("valid-rule", invalid_update).await;
        assert!(result.is_err());
    }

    // ============================================================================
    // MOCK HELPERS
    // ============================================================================

    fn create_test_automation_engine() -> AutomationEngine {
        let lifecycle_manager = Arc::new(create_mock_lifecycle_manager());
        let policy_engine = Arc::new(create_test_lifecycle_policy());
        let dependency_resolver = Arc::new(create_test_dependency_resolver());
        let state_machine = Arc::new(create_test_state_machine());

        AutomationEngine::new(
            lifecycle_manager,
            policy_engine,
            dependency_resolver,
            state_machine,
        )
    }

    fn create_mock_lifecycle_manager() -> MockLifecycleManager {
        MockLifecycleManager::new()
    }

    // Mock lifecycle manager for testing
    struct MockLifecycleManager {
        // Mock implementation would go here
    }

    impl MockLifecycleManager {
        fn new() -> Self {
            Self {}
        }
    }

    #[async_trait]
    impl LifecycleManagerService for MockLifecycleManager {
        async fn queue_operation(&self, _request: LifecycleOperationRequest) -> PluginResult<String> {
            Ok(uuid::Uuid::new_v4().to_string())
        }

        async fn get_operation_status(&self, _operation_id: &str) -> PluginResult<Option<LifecycleOperationResult>> {
            Ok(None)
        }

        async fn cancel_operation(&self, _operation_id: &str) -> PluginResult<bool> {
            Ok(false)
        }

        async fn execute_batch(&self, _request: BatchOperationRequest) -> PluginResult<String> {
            Ok(uuid::Uuid::new_v4().to_string())
        }

        async fn get_batch_status(&self, _batch_id: &str) -> PluginResult<Option<BatchOperationResult>> {
            Ok(None)
        }

        async fn cancel_batch(&self, _batch_id: &str) -> PluginResult<bool> {
            Ok(false)
        }

        async fn start_instance_with_dependencies(&self, _instance_id: &str) -> PluginResult<()> {
            Ok(())
        }

        async fn stop_instance_gracefully(&self, _instance_id: &str, _drain_period: Option<Duration>) -> PluginResult<()> {
            Ok(())
        }

        async fn restart_instance_zero_downtime(&self, _instance_id: &str) -> PluginResult<()> {
            Ok(())
        }

        async fn scale_plugin(&self, _plugin_id: &str, _target_instances: u32) -> PluginResult<Vec<String>> {
            Ok(vec![])
        }

        async fn rolling_update(&self, _plugin_id: &str, _target_version: String, _strategy: RollingUpdateStrategy) -> PluginResult<()> {
            Ok(())
        }

        async fn subscribe_events(&self) -> tokio::sync::mpsc::UnboundedReceiver<LifecycleEvent> {
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            rx
        }

        async fn get_metrics(&self) -> PluginResult<LifecycleManagerMetrics> {
            Ok(LifecycleManagerMetrics::default())
        }
    }
}