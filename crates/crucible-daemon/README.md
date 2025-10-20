# Crucible Daemon

Data layer coordination daemon for the Crucible knowledge management system.

## Overview

The Crucible daemon is a **data layer coordinator** that provides filesystem watching, parsing, database synchronization, and event publishing services for the Crucible knowledge management system. It acts as the bridge between the filesystem and the core controller, ensuring data consistency and providing real-time updates.

## Features

- **Filesystem Watching**: Real-time monitoring of file changes with configurable backends
- **File Parsing**: Automatic metadata extraction and content analysis
- **Database Synchronization**: Efficient syncing of file changes to the database
- **Event Publishing**: Real-time event streams for filesystem and database changes
- **Service Integration**: Clean integration with crucible-services framework
- **Configuration Management**: Flexible configuration from files, environment, or runtime
- **Performance Optimized**: Efficient debouncing, batch operations, and resource management
- **Health Monitoring**: Built-in health checks and metrics collection

## Architecture

### Key Components

```
src/
├── main.rs              # Entry point and daemon lifecycle management
├── lib.rs               # Public library interface
├── coordinator.rs       # Main data coordination logic
├── config.rs            # Configuration management and validation
├── events.rs            # Event system and publishers
├── services.rs          # Service layer implementations
├── handlers.rs          # Event processing handlers
```

## Usage

### Starting the Daemon

```bash
# With default configuration
cargo run --bin crucible-daemon

# With custom configuration file
cargo run --bin crucible-daemon -- config/daemon.yaml
```

The daemon will start and begin monitoring configured filesystem paths, syncing changes to the database, and publishing events. Use Ctrl+C to stop the daemon gracefully.

### Using as a Library

The daemon can also be used as a library in other components:

```rust
use crucible_daemon::{DataCoordinator, DaemonConfig};

// Create configuration
let config = DaemonConfig::default();

// Create and start coordinator
let mut coordinator = DataCoordinator::new(config).await?;
coordinator.initialize().await?;
coordinator.start().await?;

// The coordinator is now running and monitoring filesystem changes
```

## Configuration

The daemon supports multiple configuration sources:

### Configuration File

```yaml
# daemon.yaml
filesystem:
  watch_paths:
    - path: "./data"
      recursive: true
      mode: "All"
  backend: "Notify"
  debounce:
    delay_ms: 100
    max_batch_size: 100

database:
  connection:
    database_type: "SurrealDB"
    connection_string: "memory"
  sync_strategies:
    - name: "auto_sync"
      source: "Filesystem"
      target: "DatabaseTable(files)"
      mode: "Incremental"

events:
  publisher: "InMemory"
  buffer:
    size: 1000
    flush_interval_ms: 1000

performance:
  workers:
    num_workers: 4
    max_queue_size: 10000
  cache:
    cache_type: "Lru"
    max_size: 10000
    ttl_seconds: 3600

health:
  checks:
    - name: "database_health"
      check_type: "Database"
      interval_seconds: 60
      timeout_seconds: 30
```

## Design Decisions

### Data Layer Focus

The daemon is specifically focused on data layer coordination rather than business logic:
- Clean separation from application logic
- Efficient filesystem and database operations
- Event-driven architecture for real-time updates
- Service-oriented design using crucible-services

### Event-Driven Architecture

The daemon uses an event-driven architecture that provides:
- Real-time notifications of data changes
- Decoupled components through event streaming
- Extensible event handling system
- Reliable event delivery with retry mechanisms

### Service Integration

Built on the crucible-services framework for:
- Standardized service interfaces
- Service discovery and routing
- Built-in health monitoring
- Load balancing and fault tolerance

## Testing

Run tests:
```bash
cargo test --package crucible-daemon
```

Run integration tests:
```bash
cargo test --package crucible-daemon --test integration_test
```

## Development

### Adding a New Event Handler

1. Implement the `EventHandler` trait in the handlers module
2. Register the handler in the coordinator initialization
3. Test with the event testing framework

### Adding a New Service

1. Implement the required service traits
2. Register the service in the service manager
3. Add health checks and monitoring

### Configuration Updates

Configuration can be updated at runtime:
```rust
let new_config = DaemonConfig::from_env()?;
coordinator.update_config(new_config).await?;
```

## Performance Optimizations

The daemon includes several performance optimizations:

- **Event Debouncing**: Reduces filesystem noise with configurable debouncing
- **Batch Operations**: Groups database operations for efficiency
- **Connection Pooling**: Reuses database connections
- **Async Processing**: Non-blocking I/O throughout
- **Resource Limits**: Configurable limits on memory and CPU usage

## Monitoring and Health

Built-in monitoring includes:
- Service health checks
- Performance metrics collection
- Event publishing for monitoring systems
- Configurable alerting and notifications

## Future Enhancements

- Enhanced file content analysis and indexing
- Advanced synchronization strategies
- Cloud storage integration
- Multi-database support
- Advanced filtering and routing rules

## Related Documentation

- [Project README](/home/moot/crucible/README.md) - Crucible project overview
- [CLI Documentation](/home/moot/crucible/crates/crucible-cli/README.md) - CLI interface

## License

See LICENSE file in repository root.
