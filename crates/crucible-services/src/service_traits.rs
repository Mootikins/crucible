//! # Simplified Service Trait Definitions
//!
//! This module provides essential trait definitions for the core services in the simplified
//! Crucible architecture. Each service trait follows async/await patterns with essential
//! functionality without over-engineering.

use super::{
    errors::ServiceResult,
    service_types::{
        CompiledScript, ExecutionContext, ExecutionResult, ScriptExecutionStats, ScriptTool,
    },
    types::{ServiceHealth, ToolDefinition, ToolExecutionRequest, ToolExecutionResult},
};
use async_trait::async_trait;
use std::sync::Arc;

/// ============================================================================
/// ESSENTIAL SERVICE TRAITS
/// ============================================================================

/// Base trait that all services must implement
#[async_trait]
pub trait ServiceLifecycle: Send + Sync {
    /// Start the service
    async fn start(&mut self) -> ServiceResult<()>;

    /// Stop the service gracefully
    async fn stop(&mut self) -> ServiceResult<()>;

    /// Check if the service is currently running
    fn is_running(&self) -> bool;

    /// Get the service name
    fn service_name(&self) -> &str;
}

/// Trait for basic health check capabilities
#[async_trait]
pub trait HealthCheck: Send + Sync {
    /// Perform a basic health check
    async fn health_check(&self) -> ServiceResult<ServiceHealth>;
}

/// ============================================================================
/// SCRIPT ENGINE SERVICE
/// ============================================================================

/// Simplified Script Engine service for Rune script execution
#[async_trait]
pub trait ScriptEngine: ServiceLifecycle + HealthCheck {
    /// Compile a script from source code
    async fn compile_script(&mut self, source: &str) -> ServiceResult<CompiledScript>;

    /// Execute a compiled script
    async fn execute_script(
        &self,
        script_id: &str,
        context: ExecutionContext,
    ) -> ServiceResult<ExecutionResult>;

    /// Register a tool with the script engine
    async fn register_tool(&mut self, tool: ScriptTool) -> ServiceResult<()>;

    /// List available tools
    async fn list_tools(&self) -> ServiceResult<Vec<ScriptTool>>;

    /// Get execution statistics
    async fn get_execution_stats(&self) -> ServiceResult<ScriptExecutionStats>;
}

/// ============================================================================
/// TOOL SERVICE TRAIT (BACKWARD COMPATIBILITY)
/// ============================================================================

/// Basic tool service trait for backward compatibility
#[async_trait]
pub trait ToolService: Send + Sync {
    /// List all available tools
    async fn list_tools(&self) -> ServiceResult<Vec<ToolDefinition>>;

    /// Get tool definition by name
    async fn get_tool(&self, name: &str) -> ServiceResult<Option<ToolDefinition>>;

    /// Execute a tool
    async fn execute_tool(
        &self,
        request: ToolExecutionRequest,
    ) -> ServiceResult<ToolExecutionResult>;

    /// Get service health and status
    async fn service_health(&self) -> ServiceResult<ServiceHealth>;
}

/// ============================================================================
/// SERVICE REGISTRY (SIMPLIFIED)
/// ============================================================================

/// Simplified service registry for managing services
#[async_trait]
pub trait ServiceRegistry: Send + Sync {
    /// Register a service
    async fn register_service(
        &mut self,
        service_name: String,
        service: Arc<dyn ServiceLifecycle>,
    ) -> ServiceResult<()>;

    /// Get a service by name
    async fn get_service(
        &self,
        service_name: &str,
    ) -> ServiceResult<Option<Arc<dyn ServiceLifecycle>>>;

    /// List all registered services
    async fn list_services(&self) -> ServiceResult<Vec<String>>;

    /// Start all services
    async fn start_all(&mut self) -> ServiceResult<()>;

    /// Stop all services
    async fn stop_all(&mut self) -> ServiceResult<()>;
}
