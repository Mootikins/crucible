## MODIFIED Requirements

### Requirement: Blocking File Processing Integration
The system SHALL integrate file processing directly into CLI command execution workflow with queued database operations to eliminate lock contention.

#### Scenario: Automatic File Processing on CLI Startup
- **WHEN** CLI command that requires database queries is executed
- **THEN** system automatically processes pending file changes using queued database transactions
- **AND** waits for processing completion before command execution
- **AND** ensures database state reflects all file changes through single-threaded database access

#### Scenario: Integration with Existing EventDrivenEmbeddingProcessor
- **WHEN** file processing is triggered by CLI startup
- **THEN** system uses existing EventDrivenEmbeddingProcessor infrastructure with transaction queuing
- **AND** enqueues database operations instead of direct synchronous calls
- **AND** maintains consistency with real-time file watching through centralized database coordination

### Requirement: Startup File Processing Workflow
The system SHALL provide a complete file processing workflow during CLI initialization with transaction queuing for database operations.

#### Scenario: File Change Detection and Processing
- **WHEN** CLI starts up
- **THEN** system scans for file changes since last processing
- **AND** enqueues database transactions for all detected changes
- **AND** processes files through the embedding pipeline with CPU/I/O separation

#### Scenario: Database Consistency After Processing
- **WHEN** file processing completes and all transactions are processed
- **THEN** database state reflects all processed changes through single-threaded database access
- **AND** CLI commands operate on up-to-date data
- **AND** batch-aware consistency guarantees are maintained through transaction coordination

## ADDED Requirements

### Requirement: Transaction-Based File Processing
The system SHALL process files using a transaction-queuing architecture that separates CPU-bound parsing from I/O-bound database operations.

#### Scenario: Parallel Parsing with Queued Database Operations
- **WHEN** processing multiple files concurrently
- **THEN** system parses files in parallel across multiple threads
- **AND** enqueues database transactions instead of making direct database calls
- **AND** dedicates a single thread for all database operations to prevent lock contention

#### Scenario: Backpressure Management for Database Transactions
- **WHEN** transaction queue approaches capacity limits
- **THEN** system applies backpressure to file processing threads
- **AND** prevents queue overflow through bounded capacity management
- **AND** maintains system stability under high processing loads

#### Scenario: Transaction Result Handling
- **WHEN** database transactions complete or fail
- **THEN** system propagates results to appropriate processing threads
- **AND** provides error details for failed transactions
- **AND** maintains processing state and progress tracking

### Requirement: Error Handling and Recovery for Queued Transactions
The system SHALL provide comprehensive error handling for database transactions in the queuing architecture.

#### Scenario: Transaction Retry Mechanism
- **WHEN** database transactions fail due to transient errors
- **THEN** system automatically retries failed transactions with exponential backoff
- **AND** limits retry attempts to prevent infinite loops
- **AND** moves persistently failing transactions to dead-letter queue

#### Scenario: Graceful Degradation with Queue Issues
- **WHEN** transaction queue experiences systemic failures
- **THEN** system provides graceful degradation of processing capabilities
- **AND** maintains CLI functionality with appropriate warnings
- **AND** provides clear guidance for issue resolution

### Requirement: Performance Monitoring and Optimization
The system SHALL provide monitoring and optimization capabilities for the transaction-based processing architecture.

#### Scenario: Queue Performance Monitoring
- **WHEN** monitoring system performance during file processing
- **THEN** system provides metrics on queue depth, processing rates, and transaction latency
- **AND** detects when queue becomes a processing bottleneck
- **AND** provides alerts for performance degradation

#### Scenario: Transaction Batching Optimization
- **WHEN** multiple related transactions are queued for the same file
- **THEN** system automatically batches related operations for better database efficiency
- **AND** maintains transaction atomicity for file-level operations
- **AND** optimizes database round trips through intelligent grouping