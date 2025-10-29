**Note:** The `crucible-daemon` crate has been removed; references below are archival only.

# Archived Service Architecture Tests

This directory contains test files that were archived during the architecture simplification process.

## What Was Removed

### Complex Service Architecture
- **DuckDB Integration**: Removed embedded database with vector search capabilities
- **Event Routing System**: Complex EventBus and EventRouter infrastructure
- **Microservice Architecture**: Multiple coordinated services with health monitoring
- **Service Management**: Start/stop/restart functionality with lifecycle management
- **Service Discovery**: Dynamic service registration and discovery mechanisms

### Archived Files

#### Core Service Tests
1. **daemon_event_integration_tests.rs**
   - Comprehensive daemon event system tests
   - Service discovery and health monitoring
   - Event routing and publishing tests
   - Background task management
   - Performance and load testing for event system

2. **service_integration_tests.rs**
   - CLI service management command tests
   - Service health, metrics, and listing commands
   - Migration management command tests
   - Service lifecycle operations

3. **service_management_tests.rs**
   - Service lifecycle management (start/stop/restart)
   - Health monitoring and metrics collection
   - Service log management
   - Performance tests for service operations

#### Test Utilities
4. **test_utilities.rs** (crucible-daemon (removed))
   - Mock event router and event bus implementations
   - Test coordinator builders for complex scenarios
   - Event creation and manipulation utilities

5. **test_utilities.rs** (main tests directory)
   - Integration test configuration and runners
   - Mock service implementations
   - Workload simulation utilities

6. **workload_simulator.rs**
   - Realistic workload simulation for testing
   - User behavior pattern simulation
   - Performance testing under load

## Why These Tests Were Archived

### 1. **Removed Dependencies**
These tests imported modules that no longer exist:
- `crucible_services::events` - Complex event system removed
- `crucible_services::types::ServiceHealth` - Health monitoring system simplified
- EventRouter, EventBus, and related event infrastructure

### 2. **Architecture Simplification**
The system was simplified from:
- **Before**: CLI + DuckDB + Event Routing + Microservices + MCP Gateway + LLM
- **After**: CLI + SurrealDB + MCP Gateway + LLM

### 3. **Compilation Errors**
These tests would cause compilation failures due to:
- Missing import paths
- Removed service traits and interfaces
- Deleted event handling infrastructure
- Non-existent service management commands

## What Was Preserved

### MCP Gateway Functionality
- Tests related to MCP server integration
- Tool execution through MCP gateway
- Error handling for MCP operations

### Core CLI Functionality
- Basic REPL and command processing
- File system operations
- Configuration management
- Semantic search capabilities

### Database Integration
- SurrealDB client tests
- Basic database operations
- Embedding and search functionality

## Migration Notes

If the complex service architecture needs to be restored in the future:

1. **Restore Event System**: The event routing and service discovery infrastructure would need to be reimplemented in `crucible-services`

2. **Update Imports**: Test imports would need to be updated to match the new module structure

3. **Compatibility Layer**: A compatibility layer might be needed to bridge the simplified and complex architectures

4. **Service Management**: Service lifecycle management commands would need to be restored to the CLI

## Archive Date

**Archived on**: 2025-10-25
**Reason**: Architecture simplification to focus on core functionality
**Impact**: Eliminated compilation errors from removed components
**Preservation**: Tests retained for potential future reference/implementation