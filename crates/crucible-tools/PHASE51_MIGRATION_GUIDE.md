# Phase 5.1 Migration Guide

## Overview

Phase 5.1 migration enables the transition from existing Rune tools in `crucible-tools` to the new ScriptEngine service in `crucible-services`. This migration provides:

- **Seamless transition**: Existing Rune tools continue to work while gaining new capabilities
- **Enhanced security**: Tools benefit from ScriptEngine's security policies and sandboxing
- **Better performance**: Improved caching, resource management, and execution patterns
- **Service integration**: Full integration with the event system and monitoring
- **Backward compatibility**: Original RuneService API remains functional

## Architecture

### Migration Components

```
┌─────────────────────┐    ┌──────────────────────┐    ┌─────────────────────┐
│   Existing Rune     │    │   Migration Bridge   │    │  ScriptEngine       │
│   Tools             │───▶│   (ToolMigration     │───▶│  Service            │
│   (crucible-tools)  │    │   Bridge)            │    │  (crucible-services)│
└─────────────────────┘    └──────────────────────┘    └─────────────────────┘
        │                           │                           │
        ▼                           ▼                           ▼
┌─────────────────────┐    ┌──────────────────────┐    ┌─────────────────────┐
│   Rune Service      │    │   Migration Manager  │    │  Event System       │
│   (backward compat) │    │   (Phase51Manager)   │    │  & Monitoring       │
└─────────────────────┘    └──────────────────────┘    └─────────────────────┘
```

### Key Components

1. **ToolMigrationBridge**: Adapts existing Rune tools to work with ScriptEngine
2. **Phase51MigrationManager**: Orchestrates the complete migration process
3. **MigrationConfig**: Configuration for migration behavior
4. **MigrationValidation**: Validates migration integrity and functionality

## Quick Start

### Basic Migration

```rust
use crucible_tools::{Phase51MigrationManager, MigrationManagerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create migration manager with default configuration
    let config = MigrationManagerConfig::default();
    let mut manager = Phase51MigrationManager::new(config).await?;

    // Execute migration
    let report = manager.execute_migration().await?;

    println!("Migration completed!");
    println!("Tools migrated: {}", report.state.successfully_migrated);
    println!("Duration: {:?}", report.duration);

    Ok(())
}
```

### Custom Configuration

```rust
use crucible_tools::{
    Phase51MigrationManager, MigrationManagerConfig, MigrationMode,
    ValidationMode, SecurityLevel
};
use std::path::PathBuf;

let config = MigrationManagerConfig {
    mode: MigrationMode::Incremental,
    migration_directories: vec![
        PathBuf::from("./tools"),
        PathBuf::from("./rune_scripts"),
    ],
    security_level: SecurityLevel::Safe,
    validation_mode: ValidationMode::Comprehensive,
    enable_parallel_migration: false,
    max_concurrent_migrations: 3,
    rollback_on_failure: true,
    ..Default::default()
};

let mut manager = Phase51MigrationManager::new(config).await?;
let report = manager.execute_migration().await?;
```

## Migration Modes

### Dry Run Mode
- **Purpose**: Preview what would be migrated without making changes
- **Use case**: Planning and verification
- **Behavior**: Discovers tools and reports migration plan

```rust
let config = MigrationManagerConfig {
    mode: MigrationMode::DryRun,
    ..Default::default()
};
```

### Incremental Mode
- **Purpose**: Migrate tools one by one with validation
- **Use case**: Safe, controlled migration with error handling
- **Behavior**: Migrates each tool individually with validation after each

```rust
let config = MigrationManagerConfig {
    mode: MigrationMode::Incremental,
    validation_mode: ValidationMode::Basic,
    rollback_on_failure: true,
    ..Default::default()
};
```

### Full Mode
- **Purpose**: Migrate all tools at once
- **Use case**: Fast migration when confidence is high
- **Behavior**: Uses ScriptEngine's auto-migration capability

```rust
let config = MigrationManagerConfig {
    mode: MigrationMode::Full,
    validation_mode: ValidationMode::Comprehensive,
    ..Default::default()
};
```

### Manual Mode
- **Purpose**: Control migration of individual tools
- **Use case**: Selective migration or testing
- **Behavior**: Requires explicit calls to migrate each tool

```rust
let config = MigrationManagerConfig {
    mode: MigrationMode::Manual,
    ..Default::default()
};

let mut manager = Phase51MigrationManager::new(config).await?;

// Migrate specific tools
let echo_tool = manager.migrate_specific_tool("echo_tool").await?;
let calc_tool = manager.migrate_specific_tool("calculator").await?;
```

## Validation Modes

### Skip Validation
- **Purpose**: Fastest migration
- **Use case**: When tools are known to be compatible
- **Risk**: Migrated tools may not work correctly

### Basic Validation
- **Purpose**: Essential validation only
- **Use case**: Balance between speed and reliability
- **Checks**: Tool existence, basic execution test

### Comprehensive Validation
- **Purpose**: Thorough validation
- **Use case**: Production migrations
- **Checks**: Tool integrity, execution testing, security validation

## Security Levels

### Safe Mode (Default)
- **Memory limit**: 50MB
- **Execution timeout**: 10 seconds
- **File access**: Disabled
- **Network access**: Disabled
- **System calls**: Disabled
- **Allowed modules**: `crucible::basic` only

### Development Mode
- **Memory limit**: Unlimited
- **Execution timeout**: None
- **File access**: Full access
- **Network access**: Full access
- **System calls**: Full access
- **Allowed modules**: All modules

### Production Mode
- **Memory limit**: 100MB
- **Execution timeout**: 30 seconds
- **File access**: Disabled
- **Network access**: HTTP only
- **System calls**: Disabled
- **Allowed modules**: `crucible::basic`, `crucible::http`, `crucible::json`

## Error Handling

### Migration Error Types

```rust
pub enum MigrationErrorType {
    DiscoveryFailed,     // Tool discovery failed
    CompilationFailed,   // Tool compilation failed
    RegistrationFailed,  // Tool registration failed
    ValidationFailed,    // Tool validation failed
    ConfigurationError,  // Configuration error
    ServiceError,        // ScriptEngine service error
    Unknown,            // Unknown error
}
```

### Error Recovery Strategies

1. **Automatic Rollback**: Enable `rollback_on_failure` to automatically revert failed migrations
2. **Manual Recovery**: Use `rollback_tool_migration()` for manual control
3. **Retry Logic**: Implement custom retry logic for transient failures
4. **Partial Migration**: Continue with successful tools despite some failures

## Performance Considerations

### Memory Management
- Each tool gets its own VM instance for isolation
- Compiled scripts are cached to improve performance
- Resource limits prevent memory exhaustion

### Execution Performance
- Script caching reduces compilation overhead
- Parallel migration can speed up the process
- Security policies add minimal overhead

### Optimization Tips

```rust
let config = MigrationManagerConfig {
    // Enable parallel migration for faster processing
    enable_parallel_migration: true,
    max_concurrent_migrations: std::thread::available_parallelism()?.get(),

    // Use appropriate cache size
    // Too small: frequent recompilation
    // Too large: memory usage
    // Recommended: 500-1000 for most workloads
    ..Default::default()
};
```

## Monitoring and Observability

### Migration Statistics

```rust
let stats = manager.get_migration_statistics().await;
println!("Total migrated: {}", stats.total_migrated);
println!("Active tools: {}", stats.active_tools);
println!("Inactive tools: {}", stats.inactive_tools);
```

### Migration Validation

```rust
let validation = bridge.validate_migration().await?;
println!("Migration valid: {}", validation.valid);
println!("Issues: {}", validation.issues.len());
println!("Warnings: {}", validation.warnings.len());
```

### Export Reports

```rust
let report_json = manager.export_migration_report(&report).await?;
std::fs::write("migration_report.json", report_json)?;
```

## Best Practices

### 1. Planning
- Start with dry run mode to understand the scope
- Review tool dependencies and requirements
- Plan migration in phases if possible

### 2. Configuration
- Use appropriate security levels for your environment
- Enable validation for production migrations
- Set reasonable resource limits

### 3. Testing
- Test migration in development environment first
- Validate migrated tools work as expected
- Monitor performance after migration

### 4. Rollback Planning
- Have rollback procedures ready
- Document original tool configurations
- Test rollback process

### 5. Monitoring
- Monitor migration progress and errors
- Track performance metrics after migration
- Set up alerts for migration issues

## Troubleshooting

### Common Issues

#### Migration Fails with "Tool not found"
- **Cause**: Tool directory doesn't exist or contains no valid tools
- **Solution**: Verify migration directories contain `.rn` or `.rune` files

#### Compilation Errors
- **Cause**: Tool syntax errors or missing dependencies
- **Solution**: Review tool source code and error messages
- **Tip**: Use Development mode temporarily for debugging

#### Validation Failures
- **Cause**: Security policy violations or execution failures
- **Solution**: Adjust security level or fix tool issues
- **Tip**: Check validation warnings for guidance

#### Performance Issues
- **Cause**: Too many concurrent migrations or insufficient resources
- **Solution**: Reduce `max_concurrent_migrations` or increase resource limits

### Debug Information

Enable debug logging for detailed troubleshooting:

```rust
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

### Health Checks

Monitor ScriptEngine service health:

```rust
let health = manager.bridge()?.service_health().await?;
match health.status {
    ServiceStatus::Healthy => println!("Service healthy"),
    ServiceStatus::Degraded => println!("Service degraded: {:?}", health.details),
    ServiceStatus::Unhealthy => println!("Service unhealthy"),
}
```

## Migration Checklist

### Pre-Migration
- [ ] Identify all Rune tools to be migrated
- [ ] Review tool dependencies and requirements
- [ ] Choose appropriate migration mode and configuration
- [ ] Set up monitoring and logging
- [ ] Create backup of original tools

### During Migration
- [ ] Run initial dry run to verify scope
- [ ] Monitor migration progress
- [ ] Check for compilation and validation errors
- [ ] Review migration statistics
- [ ] Export migration report

### Post-Migration
- [ ] Validate migrated tools work correctly
- [ ] Test performance and resource usage
- [ ] Monitor error rates and warnings
- [ ] Update documentation and procedures
- [ ] Archive original tools (if keeping backup)

### Rollback Planning
- [ ] Document rollback procedures
- [ ] Test rollback process
- [ ] Have rollback triggers and criteria
- [ ] Plan communication for rollback scenarios

## Advanced Usage

### Custom Migration Logic

```rust
// Implement custom tool filtering
let mut manager = Phase51MigrationManager::new(config).await?;

// Migrate only specific tools based on criteria
let tools_to_migrate = discover_tools().await?
    .into_iter()
    .filter(|tool| should_migrate(tool))
    .collect::<Vec<_>>();

for tool in tools_to_migrate {
    match manager.migrate_specific_tool(&tool.name).await {
        Ok(_) => println!("Migrated: {}", tool.name),
        Err(e) => eprintln!("Failed: {} - {}", tool.name, e),
    }
}
```

### Integration with CI/CD

```rust
// Example CI integration
async fn ci_migration_pipeline() -> Result<()> {
    // 1. Dry run to validate migration scope
    let dry_config = MigrationManagerConfig {
        mode: MigrationMode::DryRun,
        ..Default::default()
    };

    let mut dry_manager = Phase51MigrationManager::new(dry_config).await?;
    let dry_report = dry_manager.execute_migration().await?;

    // 2. Fail CI if no tools found
    assert!(dry_report.state.total_discovered > 0, "No tools found to migrate");

    // 3. Perform actual migration
    let config = MigrationManagerConfig {
        mode: MigrationMode::Full,
        validation_mode: ValidationMode::Comprehensive,
        ..Default::default()
    };

    let mut manager = Phase51MigrationManager::new(config).await?;
    let report = manager.execute_migration().await?;

    // 4. Validate migration success
    assert!(report.state.successfully_migrated > 0, "No tools migrated successfully");
    assert_eq!(report.state.failed_migrations, 0, "Some tools failed to migrate");

    // 5. Export report for artifacts
    let report_json = manager.export_migration_report(&report).await?;
    std::fs::write("migration_report.json", report_json)?;

    Ok(())
}
```

## API Reference

### MigrationManagerConfig

```rust
pub struct MigrationManagerConfig {
    pub mode: MigrationMode,
    pub security_level: SecurityLevel,
    pub migration_directories: Vec<PathBuf>,
    pub preserve_original_service: bool,
    pub enable_parallel_migration: bool,
    pub max_concurrent_migrations: usize,
    pub validation_mode: ValidationMode,
    pub rollback_on_failure: bool,
}
```

### MigrationReport

```rust
pub struct MigrationReport {
    pub migration_id: String,
    pub config: MigrationManagerConfig,
    pub stats: MigrationStats,
    pub state: MigrationState,
    pub migrated_tools: Vec<String>,
    pub failed_tools: Vec<MigrationError>,
    pub validation: Option<MigrationValidation>,
    pub duration: Option<Duration>,
    pub timestamp: DateTime<Utc>,
}
```

### ToolMigrationBridge

```rust
impl ToolMigrationBridge {
    pub async fn new(
        rune_config: RuneServiceConfig,
        migration_config: MigrationConfig,
    ) -> Result<Self>;

    pub async fn execute_migrated_tool(
        &self,
        tool_name: &str,
        parameters: Value,
        execution_context: Option<ToolExecutionContext>,
    ) -> Result<ToolExecutionResult>;

    pub async fn list_migrated_tools(&self) -> Result<Vec<MigratedTool>>;
    pub async fn remove_migrated_tool(&self, tool_name: &str) -> Result<bool>;
    pub async fn validate_migration(&self) -> Result<MigrationValidation>;
}
```

## Support and Contributing

### Getting Help
- Check the [troubleshooting guide](#troubleshooting) first
- Review test files for example usage
- Check the Rust documentation for detailed API information

### Contributing
- Follow Rust coding standards
- Add comprehensive tests for new features
- Update documentation for API changes
- Ensure all migrations are backward compatible

## License

This migration system is part of the Crucible project and follows the same licensing terms.