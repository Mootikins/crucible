# Task Routing System

A comprehensive task routing system for agent orchestration in the Crucible knowledge management system.

## Overview

The task routing system provides intelligent analysis, routing, execution, and monitoring of tasks across multiple AI agents. It consists of seven main components:

### 1. Task Analysis Engine (`analyzer.rs`)
- Analyzes user requests and breaks down complex tasks into subtasks
- Determines task complexity and dependencies
- Recommends execution strategies
- Estimates task duration and resource requirements

### 2. Intelligent Routing Algorithm (`router.rs`)
- Routes tasks to the most appropriate agents based on:
  - Capability matching
  - Performance history
  - Current load balancing
  - Specialization scores
- Learns from routing decisions to improve future performance
- Supports backup agents and fallback strategies

### 3. Task Queue Management (`queue.rs`)
- Manages task prioritization and queuing
- Supports multiple priority levels (Emergency → Critical → High → Normal → Low)
- Handles task dependencies and waitlists
- Provides task cancellation and status tracking

### 4. Execution Engine (`executor.rs`)
- Coordinates task execution across multiple agents
- Supports both single-agent and multi-agent execution
- Provides progress tracking and checkpointing
- Handles task pausing, resumption, and cancellation

### 5. Result Aggregation (`aggregator.rs`)
- Combines results from multiple agents into coherent responses
- Validates result quality and resolves conflicts
- Supports various aggregation strategies:
  - Concatenation
  - Structured merge
  - Expert synthesis
  - Hierarchical composition

### 6. Error Handling & Recovery (`error_handler.rs`)
- Comprehensive error handling and recovery strategies
- Circuit breaker pattern for failing agents
- Retry policies with exponential backoff
- Error pattern recognition and automated recovery

### 7. Performance Monitoring (`monitor.rs`)
- Real-time performance metrics and alerting
- Historical data analysis and trend detection
- Optimization suggestions based on performance data
- Performance reporting and analytics

## Key Features

### Single-Agent and Multi-Agent Support
- Automatically determines when multiple agents are needed
- Coordinates collaborative workflows
- Manages agent communication and data sharing

### Task Dependencies and Parallel Execution
- Identifies and manages task dependencies
- Executes independent tasks in parallel where possible
- Provides dependency resolution and scheduling

### Learning and Optimization
- Learns from routing decisions and execution outcomes
- Automatically optimizes routing strategies
- Provides performance-based agent recommendations

### Transparent Communication
- Provides clear feedback about routing decisions
- Shows task progress and execution status
- Offers explanations for task failures and recovery actions

### Robust Error Handling
- Multiple recovery strategies for different error types
- Circuit breaker protection for failing agents
- Graceful degradation when agents are unavailable

## Usage Example

```rust
use crucible_core::task_router::{TaskRouter, UserRequest, TaskPriority};

// Create a task router
let router = TaskRouter::new();

// Create a user request
let request = UserRequest {
    id: uuid::Uuid::new_v4(),
    user_id: "user123".to_string(),
    content: "Analyze the performance data and suggest improvements".to_string(),
    request_type: RequestType::Analysis,
    priority: TaskPriority::Normal,
    context: RequestContext::default(),
    timestamp: chrono::Utc::now(),
    deadline: None,
};

// Process the request
let result = router.process_request(request).await?;

// Get the result
println!("Task completed: {}", result.content);
println!("Agents involved: {:?}", result.execution_summary.agents_involved);
```

## Configuration

The system can be configured through various configuration objects:

- `MonitoringConfig`: Controls metrics collection and alerting
- `QueueConfig`: Manages queue behavior and limits
- `ExecutionConfig`: Configures execution parameters
- `ErrorHandlerConfig`: Sets error handling policies

## Performance Monitoring

The system provides comprehensive performance monitoring:

```rust
// Get current system status
let status = router.get_status().await?;

// Get performance metrics
let metrics = router.get_metrics().await?;

// Get task history
let history = router.get_task_history(Some(50)).await?;

// Get routing analytics
let analytics = router.get_routing_analytics().await?;
```

## Error Handling

The system includes sophisticated error handling:

```rust
// Error handling is automatic, but you can check results
match result.success {
    true => println!("Task completed successfully"),
    false => println!("Task failed: {:?}", result.recommendations),
}

// Cancel a task if needed
let cancelled = router.cancel_task(&task_id).await?;
```

## Integration with Existing Systems

The task router integrates seamlessly with:

- **Agent Registry**: Uses existing agent definitions and capabilities
- **Enhanced Chat System**: Works with multi-agent conversations
- **Collaboration Manager**: Coordinates complex multi-agent workflows
- **MCP Tools**: Provides tools for task execution and monitoring

## Architecture Benefits

1. **Scalability**: Handles complex workflows with multiple agents
2. **Reliability**: Robust error handling and recovery mechanisms
3. **Performance**: Optimized routing based on historical data
4. **Transparency**: Clear visibility into task execution
5. **Flexibility**: Configurable strategies and policies
6. **Learning**: Improves over time based on execution outcomes

## Future Enhancements

- Machine learning-based routing optimization
- Advanced agent collaboration patterns
- Real-time load balancing across distributed agents
- Custom workflow templates
- Integration with external monitoring systems