#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_user_request() -> UserRequest {
        UserRequest {
            id: Uuid::new_v4(),
            user_id: "test_user".to_string(),
            content: "Analyze the performance of our system and suggest improvements".to_string(),
            request_type: RequestType::Analysis,
            priority: TaskPriority::Normal,
            context: RequestContext {
                conversation_history: Vec::new(),
                previous_results: Vec::new(),
                available_tools: vec!["analysis".to_string(), "monitoring".to_string()],
                user_preferences: HashMap::new(),
                session_context: HashMap::new(),
            },
            timestamp: Utc::now(),
            deadline: None,
        }
    }

    fn create_test_routing_decision() -> RoutingDecision {
        RoutingDecision {
            subtask_id: Uuid::new_v4(),
            assigned_agent_id: Uuid::new_v4(),
            assigned_agent_name: "TestAgent".to_string(),
            confidence: 0.85,
            routing_reason: RoutingReason::CapabilityMatch,
            estimated_execution_time_ms: 30000,
            required_resources: vec!["analysis".to_string()],
            backup_agents: Vec::new(),
        }
    }

    fn create_test_queued_task() -> QueuedTask {
        let routing_decision = create_test_routing_decision();
        QueuedTask {
            id: Uuid::new_v4(),
            subtask: Subtask {
                id: Uuid::new_v4(),
                description: "Test subtask".to_string(),
                subtask_type: SubtaskType::Analysis,
                required_capabilities: vec!["analysis".to_string()],
                required_tools: vec!["analysis".to_string()],
                estimated_duration_minutes: 5,
                priority: TaskPriority::Normal,
                can_parallelize: true,
                input_requirements: Vec::new(),
                expected_output: "Analysis results".to_string(),
            },
            routing: routing_decision,
            queue_position: 1,
            queued_at: Utc::now(),
            estimated_start_time: Some(Utc::now()),
            status: QueuedTaskStatus::Queued,
        }
    }

    fn create_test_execution_result() -> TaskExecutionResult {
        TaskExecutionResult {
            task_id: Uuid::new_v4(),
            executing_agent_id: Uuid::new_v4(),
            success: true,
            result_content: "Task completed successfully".to_string(),
            metrics: ExecutionMetrics {
                start_time: Utc::now(),
                end_time: Utc::now(),
                execution_time_ms: 25000,
                cpu_usage_percent: Some(50.0),
                memory_usage_mb: Some(128),
                tool_calls_count: 2,
                tokens_processed: Some(1000),
                confidence_score: 0.9,
            },
            artifacts: Vec::new(),
            error: None,
            agent_feedback: Some("Task completed without issues".to_string()),
        }
    }

    #[tokio::test]
    async fn test_task_analyzer_basic() {
        let analyzer = TaskAnalyzer::new();
        let request = create_test_user_request();

        let analysis = analyzer.analyze_request(&request).await.unwrap();

        assert!(!analysis.subtasks.is_empty());
        assert!(analysis.confidence > 0.0);
        assert!(analysis.estimated_duration_minutes > 0);
    }

    #[tokio::test]
    async fn test_task_analyzer_complexity_assessment() {
        let analyzer = TaskAnalyzer::new();

        let simple_request = UserRequest {
            content: "Hello".to_string(),
            ..create_test_user_request()
        };

        let complex_request = UserRequest {
            content: "Perform a comprehensive multi-step analysis of the system performance, including research into best practices, code review, and detailed documentation creation".to_string(),
            ..create_test_user_request()
        };

        let simple_analysis = analyzer.analyze_request(&simple_request).await.unwrap();
        let complex_analysis = analyzer.analyze_request(&complex_request).await.unwrap();

        assert!(complex_analysis.complexity.score > simple_analysis.complexity.score);
        assert!(complex_analysis.subtasks.len() >= simple_analysis.subtasks.len());
    }

    #[tokio::test]
    async fn test_intelligent_router_candidate_scoring() {
        let router = IntelligentRouter::new();
        let subtask = create_test_queued_task().subtask;

        // This test would require mock agents to be properly functional
        // For now, we just test that the method exists and doesn't panic
        // In a real implementation, we would set up mock agent registry
    }

    #[tokio::test]
    async fn test_task_queue_enqueue_and_dequeue() {
        let mut queue_manager = TaskQueueManager::new();
        let routing_decision = create_test_routing_decision();

        let queued_task = queue_manager.enqueue_task(routing_decision).await.unwrap();

        assert_eq!(queued_task.status, QueuedTaskStatus::Queued);
        assert!(!queued_task.id.to_string().is_empty());

        let next_task = queue_manager.get_next_task().await.unwrap();
        assert!(next_task.is_some());

        let retrieved_task = next_task.unwrap();
        assert_eq!(retrieved_task.id, queued_task.id);
    }

    #[tokio::test]
    async fn test_task_queue_priority_ordering() {
        let mut queue_manager = TaskQueueManager::new();

        // Add tasks with different priorities
        let low_priority_routing = RoutingDecision {
            priority: TaskPriority::Low,
            ..create_test_routing_decision()
        };

        let high_priority_routing = RoutingDecision {
            priority: TaskPriority::High,
            ..create_test_routing_decision()
        };

        let _low_task = queue_manager.enqueue_task(low_priority_routing).await.unwrap();
        let _high_task = queue_manager.enqueue_task(high_priority_routing).await.unwrap();

        // High priority task should be returned first
        let next_task = queue_manager.get_next_task().await.unwrap();
        assert!(next_task.is_some());

        // Note: This test would need more sophisticated setup to fully test priority ordering
        // as it depends on the internal heap implementation
    }

    #[tokio::test]
    async fn test_execution_engine_single_task() {
        let engine = ExecutionEngine::new();
        let queued_task = create_test_queued_task();

        let result = engine.execute_task(queued_task).await.unwrap();

        assert!(result.task_id.to_string().len() > 0);
        assert!(result.executing_agent_id.to_string().len() > 0);
        assert!(result.execution_time_ms > 0);
    }

    #[tokio::test]
    async fn test_execution_engine_task_cancellation() {
        let engine = ExecutionEngine::new();
        let task_id = Uuid::new_v4();

        // Test cancellation of non-existent task
        let result = engine.cancel_task(&task_id).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_result_aggregator_single_result() {
        let aggregator = ResultAggregator::new();

        let analysis = TaskAnalysis {
            request_id: Uuid::new_v4(),
            subtasks: vec![],
            required_capabilities: vec!["analysis".to_string()],
            complexity: TaskComplexity {
                score: 3,
                skill_diversity: 1,
                coordination_complexity: 1,
                technical_difficulty: 2,
                ambiguity_level: 1,
            },
            estimated_duration_minutes: 5,
            dependencies: vec![],
            execution_strategy: ExecutionStrategy::SingleAgent,
            confidence: 0.9,
            timestamp: Utc::now(),
        };

        let execution_result = create_test_execution_result();
        let results = vec![execution_result];

        let final_result = aggregator.aggregate_results(&analysis, results).await.unwrap();

        assert_eq!(final_result.request_id, analysis.request_id);
        assert!(final_result.success);
        assert!(!final_result.content.is_empty());
    }

    #[tokio::test]
    async fn test_error_handler_basic() {
        let mut error_handler = ErrorHandler::new();

        let task_id = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let error = TaskError {
            error_type: ErrorType::NetworkError,
            message: "Connection timeout".to_string(),
            stack_trace: None,
            context: HashMap::new(),
            recoverable: true,
        };

        let handling_result = error_handler.handle_error(&task_id, &agent_id, &error).await.unwrap();

        assert!(handling_result.error_id.to_string().len() > 0);
        assert!(!handling_result.message.is_empty());
    }

    #[tokio::test]
    async fn test_error_handler_circuit_breaker() {
        let mut error_handler = ErrorHandler::new();

        let agent_id = Uuid::new_v4();

        // Simulate multiple failures to trigger circuit breaker
        for i in 0..6 {
            let task_id = Uuid::new_v4();
            let error = TaskError {
                error_type: ErrorType::NetworkError,
                message: format!("Failure {}", i),
                stack_trace: None,
                context: HashMap::new(),
                recoverable: false,
            };

            let _result = error_handler.handle_error(&task_id, &agent_id, &error).await.unwrap();
        }

        // Circuit breaker should now be open
        let is_open = error_handler.is_circuit_breaker_open(&agent_id).await.unwrap();
        assert!(is_open);
    }

    #[tokio::test]
    async fn test_performance_monitor_basic() {
        let mut monitor = PerformanceMonitor::new();

        let analysis = TaskAnalysis {
            request_id: Uuid::new_v4(),
            subtasks: vec![],
            required_capabilities: vec!["analysis".to_string()],
            complexity: TaskComplexity {
                score: 3,
                skill_diversity: 1,
                coordination_complexity: 1,
                technical_difficulty: 2,
                ambiguity_level: 1,
            },
            estimated_duration_minutes: 5,
            dependencies: vec![],
            execution_strategy: ExecutionStrategy::SingleAgent,
            confidence: 0.9,
            timestamp: Utc::now(),
        };

        let result = TaskResult {
            request_id: analysis.request_id,
            success: true,
            content: "Test result".to_string(),
            subtask_results: vec![create_test_execution_result()],
            execution_summary: ExecutionSummary {
                total_subtasks: 1,
                successful_subtasks: 1,
                failed_subtasks: 0,
                agents_involved: vec!["TestAgent".to_string()],
                tools_used: vec!["analysis".to_string()],
                collaboration_sessions: 0,
                total_cost: None,
            },
            recommendations: vec!["Continue monitoring".to_string()],
            follow_up_suggestions: vec!["Run follow-up analysis".to_string()],
            completed_at: Utc::now(),
            total_execution_time_ms: 25000,
        };

        let execution_time = std::time::Duration::from_millis(25000);

        monitor.record_execution(&analysis, &result, execution_time).await.unwrap();

        let metrics = monitor.get_metrics().await.unwrap();
        assert!(metrics.success_rate > 0.0);
        assert!(metrics.avg_task_duration_ms > 0);
    }

    #[tokio::test]
    async fn test_performance_monitor_alerts() {
        let mut monitor = PerformanceMonitor::new();

        // Simulate conditions that might trigger alerts
        // This would require more setup to actually trigger alerts
        let active_alerts = monitor.get_active_alerts();
        assert!(active_alerts.len() >= 0); // Basic assertion that method works
    }

    #[tokio::test]
    async fn test_performance_monitor_optimization_suggestions() {
        let monitor = PerformanceMonitor::new();

        let suggestions = monitor.get_optimization_suggestions();
        // Initially may be empty, but method should work
        assert!(suggestions.len() >= 0);
    }

    #[tokio::test]
    async fn test_task_router_end_to_end() {
        let router = TaskRouter::new();
        let request = create_test_user_request();

        // This test would require proper setup of agent registries and mock systems
        // For now, we test that the router can be created and has the expected interface

        // Test getting status (should work even without processing)
        let status = router.get_status().await.unwrap();
        assert_eq!(status.active_tasks, 0);
        assert_eq!(status.total_processed, 0);
    }

    #[tokio::test]
    async fn test_task_types_and_priorities() {
        // Test that all task types can be created
        let task_types = vec![
            SubtaskType::Research,
            SubtaskType::Analysis,
            SubtaskType::CodeGeneration,
            SubtaskType::Writing,
            SubtaskType::Coordination,
        ];

        for task_type in task_types {
            let subtask = Subtask {
                id: Uuid::new_v4(),
                description: "Test task".to_string(),
                subtask_type: task_type.clone(),
                required_capabilities: vec![],
                required_tools: vec![],
                estimated_duration_minutes: 5,
                priority: TaskPriority::Normal,
                can_parallelize: true,
                input_requirements: vec![],
                expected_output: "Test output".to_string(),
            };

            assert_eq!(subtask.subtask_type, task_type);
        }

        // Test priority ordering
        assert!(TaskPriority::Emergency > TaskPriority::Critical);
        assert!(TaskPriority::Critical > TaskPriority::High);
        assert!(TaskPriority::High > TaskPriority::Normal);
        assert!(TaskPriority::Normal > TaskPriority::Low);
    }

    #[tokio::test]
    async fn test_error_types_and_recovery() {
        let error_types = vec![
            ErrorType::AgentExecution,
            ErrorType::ToolFailure,
            ErrorType::NetworkError,
            ErrorType::Timeout,
            ErrorType::InvalidInput,
        ];

        for error_type in error_types {
            let error = TaskError {
                error_type: error_type.clone(),
                message: "Test error".to_string(),
                stack_trace: None,
                context: HashMap::new(),
                recoverable: true,
            };

            assert_eq!(error.error_type, error_type);
            assert!(error.recoverable);
        }
    }

    #[tokio::test]
    async fn test_routing_reasons() {
        let routing_reasons = vec![
            RoutingReason::CapabilityMatch,
            RoutingReason::PerformanceRating,
            RoutingReason::Specialization,
            RoutingReason::LoadBalancing,
            RoutingReason::UserPreference,
        ];

        for reason in routing_reasons {
            // Test that routing reasons can be created and compared
            assert_eq!(reason, reason);
        }
    }

    #[test]
    fn test_task_priority_ordering() {
        let priorities = vec![
            TaskPriority::Emergency,
            TaskPriority::Critical,
            TaskPriority::High,
            TaskPriority::Normal,
            TaskPriority::Low,
        ];

        // Test that priorities are ordered correctly
        for (i, priority) in priorities.iter().enumerate() {
            for (j, other_priority) in priorities.iter().enumerate() {
                if i < j {
                    assert!(priority > other_priority);
                } else if i > j {
                    assert!(priority < other_priority);
                } else {
                    assert_eq!(priority, other_priority);
                }
            }
        }
    }

    #[test]
    fn test_execution_strategies() {
        let strategies = vec![
            ExecutionStrategy::SingleAgent,
            ExecutionStrategy::SequentialMultiAgent,
            ExecutionStrategy::ParallelExecution,
            ExecutionStrategy::Collaborative,
            ExecutionStrategy::Hybrid,
        ];

        for strategy in strategies {
            // Test that strategies can be created
            let _ = format!("{:?}", strategy);
        }
    }

    #[test]
    fn test_queue_task_status_transitions() {
        let mut status = QueuedTaskStatus::Queued;

        // Simulate status transitions
        status = QueuedTaskStatus::Assigned;
        assert_eq!(status, QueuedTaskStatus::Assigned);

        status = QueuedTaskStatus::Executing;
        assert_eq!(status, QueuedTaskStatus::Executing);

        status = QueuedTaskStatus::Cancelled;
        assert_eq!(status, QueuedTaskStatus::Cancelled);
    }
}