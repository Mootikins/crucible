//! # Plugin Manager
//!
//! A comprehensive plugin management system for the Crucible knowledge management platform.
//! This module provides process isolation, lifecycle management, resource monitoring,
//! security enforcement, and health monitoring for plugins.

pub mod config;
pub mod error;
pub mod types;
pub mod registry;
pub mod instance;
pub mod resource_manager;
pub mod security_manager;
pub mod health_monitor;
pub mod resource_monitor;
pub mod health_checker;
pub mod manager;
pub mod lifecycle_manager;
pub mod state_machine;
pub mod dependency_resolver;
pub mod lifecycle_policy;
pub mod automation_engine;
pub mod batch_operations;

// Test modules
#[cfg(test)]
pub mod tests;

// Re-export main types and traits
pub use config::*;
pub use error::*;
pub use types::*;
pub use registry::{PluginRegistry, DefaultPluginRegistry, PluginInstaller, RegistryEvent};
pub use instance::{PluginInstance, DefaultPluginInstance, InstanceEvent};
pub use resource_manager::{ResourceManager, DefaultResourceManager, ResourceEvent};
pub use security_manager::{SecurityManager, DefaultSecurityManager, SecurityEvent};
pub use health_monitor::{HealthMonitor, DefaultHealthMonitor, HealthEvent};
pub use resource_monitor::{ResourceMonitor, ResourceMonitoringService, ResourceMonitoringEvent, ResourceUsageHistory};
pub use health_checker::{HealthChecker, HealthCheckingService, HealthCheckEvent, HealthStatusHistory, HealthStatistics};
pub use manager::{PluginManagerService, MonitoringStatistics};

// Advanced lifecycle management
pub use lifecycle_manager::{
    LifecycleManager, LifecycleManagerService, LifecycleOperation, LifecycleOperationRequest,
    LifecycleOperationResult, LifecycleEvent, BatchOperationRequest, BatchOperationResult,
    LifecyclePolicy as LifecyclePolicyConfig, AutomationRule as AutomationRuleConfig,
};
pub use state_machine::{
    PluginStateMachine, StateMachineService, StateTransition, StateTransitionResult,
    StateMachineEvent, PluginInstanceState,
};
pub use dependency_resolver::{
    DependencyResolver, DependencyGraph, DependencyResolutionResult, DependencyUpdateOperation,
    DependencyChangeNotification, DependencyAnalytics,
};
pub use lifecycle_policy::{
    LifecyclePolicyEngine, PolicyEngineService, LifecyclePolicy as PolicyRule,
    PolicyDecision, PolicyEvaluationResult, PolicyEvent, PolicyConflict,
};
pub use automation_engine::{
    AutomationEngine, AutomationEngineService, AutomationRule, AutomationEvent,
    AutomationExecutionContext, AutomationExecutionResult,
};
pub use batch_operations::{
    BatchOperationsCoordinator, BatchOperationsService, BatchOperation, BatchExecutionResult,
    BatchOperationEvent, BatchProgressUpdate, BatchTemplate,
};