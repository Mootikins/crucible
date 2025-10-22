# CLI Service Integration Guide

This document describes the enhanced CLI integration with the new ScriptEngine service architecture for Phase 5.3.

## Overview

The Crucible CLI has been enhanced to support the new service-based architecture, providing:

- **Service Management**: Commands for managing and monitoring Crucible services
- **Migration Management**: Commands for migrating Rune tools to ScriptEngine service
- **Backward Compatibility**: Seamless integration with existing Rune tools
- **Enhanced Configuration**: New configuration options for services and migration

## New CLI Commands

### Service Commands

The `service` command group provides comprehensive service management capabilities:

```bash
# Show health status of all services
crucible service health

# Show health of specific service
crucible service health crucible-script-engine

# Show detailed health information
crucible service health --detailed

# Show service metrics
crucible service metrics

# Show real-time metrics monitoring
crucible service metrics --real-time

# List all services
crucible service list

# List services with status
crucible service list --status

# Show service logs
crucible service logs

# Show logs for specific service
crucible service logs crucible-script-engine

# Follow logs in real-time
crucible service logs --follow

# Show only error logs
crucible service logs --errors
```

#### Service Lifecycle Management

```bash
# Start a service
crucible service start crucible-script-engine

# Start service and wait for it to be ready
crucible service start crucible-script-engine --wait

# Stop a service
crucible service stop crucible-script-engine

# Force stop a service
crucible service stop crucible-script-engine --force

# Restart a service
crucible service restart crucible-script-engine

# Restart service and wait for it to be ready
crucible service restart crucible-script-engine --wait
```

### Migration Commands

The `migration` command group provides comprehensive migration management:

```bash
# Show migration status
crucible migration status

# Show detailed migration information
crucible migration status --detailed

# Validate migration integrity
crucible migration status --validate

# List migrated tools
crucible migration list

# Show migration metadata
crucible migration list --metadata

# Show only active tools
crucible migration list --active

# Show only inactive tools
crucible migration list --inactive
```

#### Migration Operations

```bash
# Migrate all tools (dry run)
crucible migration migrate --dry-run

# Migrate all tools
crucible migration migrate

# Migrate specific tool
crucible migration migrate --tool search-tool

# Force migration
crucible migration migrate --force

# Set security level for migrated tools
crucible migration migrate --security-level production

# Validate migration integrity
crucible migration validate

# Validate specific tool
crucible migration validate --tool search-tool

# Auto-fix validation issues
crucible migration validate --auto-fix
```

#### Migration Rollback

```bash
# Rollback all tools
crucible migration rollback

# Rollback specific tool
crucible migration rollback --tool search-tool

# Confirm rollback without prompt
crucible migration rollback --confirm

# Keep backup during rollback
crucible migration rollback --backup
```

#### Maintenance Commands

```bash
# Reload migrated tool from source
crucible migration reload search-tool

# Force reload
crucible migration reload search-tool --force

# Clean up migration artifacts
crucible migration cleanup

# Remove inactive migrations
crucible migration cleanup --inactive

# Remove failed migrations
crucible migration cleanup --failed

# Confirm cleanup without prompt
crucible migration cleanup --confirm
```

## Enhanced Rune Command

The `run` command has been enhanced to use the ScriptEngine service when available:

```bash
# Execute Rune script (will try ScriptEngine first)
crucible run my-script.rn

# Execute with arguments
crucible run my-script.rn --args '{"param": "value"}'

# Execute script by name (searches standard locations)
crucible run my-script
```

The command now follows this execution strategy:

1. **ScriptEngine Service**: If migration is enabled, tries to execute using the migration bridge
2. **Fallback**: If ScriptEngine execution fails, falls back to legacy Rune service
3. **Error Reporting**: Provides clear feedback about which execution method was used

## Configuration

The CLI configuration has been enhanced with new sections for services and migration:

### Services Configuration

```toml
[services]
# ScriptEngine service configuration
[services.script_engine]
enabled = true
security_level = "safe"
max_source_size = 1048576  # 1MB
default_timeout_secs = 30
enable_caching = true
max_cache_size = 1000
max_memory_mb = 100
max_cpu_percentage = 80.0
max_concurrent_operations = 50

# Service discovery configuration
[services.discovery]
enabled = true
endpoints = ["localhost:8080"]
timeout_secs = 5
refresh_interval_secs = 30

# Service health monitoring configuration
[services.health]
enabled = true
check_interval_secs = 10
timeout_secs = 5
failure_threshold = 3
auto_recovery = true
```

### Migration Configuration

```toml
[migration]
enabled = true
default_security_level = "safe"
auto_migrate = false
enable_caching = true
max_cache_size = 500
preserve_tool_ids = true
backup_originals = true

# Migration validation settings
[migration.validation]
auto_validate = true
strict = false
validate_functionality = true
validate_performance = false
max_performance_degradation = 20.0
```

### Security Levels

Available security levels for script execution:

- **`safe`**: Sandbox mode with limited capabilities (default)
- **`development`**: Full capabilities for development
- **`production`**: Balanced security and functionality for production

## Output Formats

Most commands support multiple output formats:

```bash
# Table format (default)
crucible service health --format table

# JSON format
crucible service health --format json

# Compact JSON
crucible migration status --format json
```

## Environment Variables

The CLI supports environment variables for configuration:

```bash
# Migration settings
export CRUCIBLE_MIGRATION_ENABLED=true
export CRUCIBLE_MIGRATION_SECURITY_LEVEL=production

# Service settings
export CRUCIBLE_SERVICE_DISCOVERY_ENDPOINTS=localhost:8080,localhost:8081

# Test mode (skip user config loading)
export CRUCIBLE_TEST_MODE=1
```

## Error Handling

The CLI provides comprehensive error handling:

- **Graceful Degradation**: Falls back to legacy services when new services are unavailable
- **Clear Error Messages**: Provides specific error information and suggestions
- **Validation**: Validates configuration and provides helpful error messages
- **Recovery**: Attempts automatic recovery when possible

## Migration Workflow

A typical migration workflow:

1. **Check Current Status**
   ```bash
   crucible migration status
   ```

2. **Preview Migration**
   ```bash
   crucible migration migrate --dry-run
   ```

3. **Execute Migration**
   ```bash
   crucible migration migrate
   ```

4. **Validate Migration**
   ```bash
   crucible migration validate
   ```

5. **Monitor Services**
   ```bash
   crucible service health
   crucible service metrics
   ```

6. **Test Integration**
   ```bash
   crucible run my-tool.rn
   ```

## Service Management Workflow

A typical service management workflow:

1. **List Services**
   ```bash
   crucible service list --status
   ```

2. **Check Health**
   ```bash
   crucible service health --detailed
   ```

3. **Monitor Metrics**
   ```bash
   crucible service metrics
   crucible service metrics --real-time
   ```

4. **Manage Services**
   ```bash
   crucible service start crucible-script-engine
   crucible service restart crucible-rune-service
   ```

5. **View Logs**
   ```bash
   crucible service logs --follow
   crucible service logs crucible-script-engine --errors
   ```

## Troubleshooting

### Common Issues

1. **Migration Disabled**
   ```
   Error: Migration is disabled in configuration
   ```
   **Solution**: Enable migration in config:
   ```toml
   [migration]
   enabled = true
   ```

2. **Service Not Found**
   ```
   Error: Service not found: non-existent-service
   ```
   **Solution**: Check available services:
   ```bash
   crucible service list
   ```

3. **Script Not Found**
   ```
   Error: Script not found: my-script.rn
   ```
   **Solution**: Check script location or use full path:
   ```bash
   crucible run /path/to/my-script.rn
   ```

### Debug Mode

Enable debug logging for troubleshooting:

```bash
RUST_LOG=debug crucible service health
RUST_LOG=debug crucible migration status
```

### Test Mode

Use test mode to avoid loading user configuration:

```bash
CRUCIBLE_TEST_MODE=1 crucible service list
```

## Integration with Existing Workflows

The new CLI integration is designed to be backward compatible:

- **Existing Commands**: All existing CLI commands continue to work
- **Gradual Migration**: Can enable migration features incrementally
- **Fallback Behavior**: Automatic fallback to legacy services when needed
- **Configuration Migration**: Existing configuration files continue to work

## Performance Considerations

- **Service Caching**: ScriptEngine caching improves performance for repeated executions
- **Async Operations**: All service operations are asynchronous and non-blocking
- **Resource Limits**: Configurable limits prevent resource exhaustion
- **Health Monitoring**: Proactive health checks prevent service degradation

## Security Considerations

- **Security Levels**: Multiple security levels for different use cases
- **Sandboxing**: Script execution is sandboxed by default
- **Validation**: Comprehensive validation of scripts and configurations
- **Access Control**: Configurable access controls and permissions

## Future Enhancements

Planned enhancements for the CLI service integration:

- **Service Templates**: Predefined service configurations
- **Advanced Metrics**: More detailed performance metrics
- **Alerting**: Configurable alerts for service issues
- **Backup/Restore**: Service state backup and restore capabilities
- **Multi-Environment**: Support for multiple deployment environments

## Examples

### Example 1: Complete Migration

```bash
# 1. Check current state
crucible migration status --detailed

# 2. Preview migration
crucible migration migrate --dry-run --security-level production

# 3. Execute migration
crucible migration migrate --security-level production

# 4. Validate results
crucible migration validate --auto-fix

# 5. Test migrated tools
crucible run search-tool --args '{"query": "test"}'

# 6. Monitor service health
crucible service health --detailed
```

### Example 2: Service Monitoring

```bash
# 1. List all services
crucible service list --status --detailed

# 2. Check health
crucible service health

# 3. Monitor metrics
crucible service metrics --format json

# 4. Real-time monitoring (run in background)
crucible service metrics --real-time &

# 5. Check logs if issues found
crucible service logs --errors --follow
```

### Example 3: Development Workflow

```bash
# 1. Use development security level
crucible migration migrate --security-level development

# 2. Test new tool
crucible run my-new-tool.rn --args '{"test": true}'

# 3. Monitor in real-time
crucible service metrics --real-time

# 4. Check logs for debugging
crucible service logs --lines 50 --follow

# 5. Reload tool after changes
crucible migration reload my-new-tool --force
```

## API Reference

For detailed API documentation and advanced usage, see the Rust documentation:

```bash
cargo doc --package crucible-cli --open
```

## Contributing

To contribute to the CLI service integration:

1. **Code Structure**: See `src/commands/service.rs` and `src/commands/migration.rs`
2. **Testing**: Add tests to `tests/service_integration_tests.rs`
3. **Documentation**: Update this file and command help text
4. **Configuration**: Extend configuration in `src/config.rs`

## Support

For issues and questions:

1. **GitHub Issues**: Report bugs and request features
2. **Documentation**: Check this guide and inline help (`--help`)
3. **Debug Mode**: Use `RUST_LOG=debug` for detailed logging
4. **Test Mode**: Use `CRUCIBLE_TEST_MODE=1` for isolated testing