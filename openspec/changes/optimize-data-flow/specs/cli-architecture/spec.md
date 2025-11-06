## MODIFIED Requirements

### Requirement: Integrated File Processing on Startup
The system SHALL process file changes efficiently using incremental change detection before executing CLI commands.

**Implementation:** `crucible-cli` orchestrates components from `crucible-watch` (file scanning/detection), `crucible-core` (hashing/diffing), and `crucible-surrealdb` (storage)

#### Scenario: Fast Startup Processing
- **WHEN** CLI command is invoked with kiln containing unchanged files
- **THEN** system completes file processing in milliseconds rather than seconds
- **AND** uses change detection to skip unchanged files entirely
- **AND** only processes blocks that have actually changed
- **AND** maintains full data consistency for CLI operations

#### Scenario: Progress Feedback for Incremental Processing
- **WHEN** file processing takes more than minimal time
- **THEN** system shows progress for changed files only
- **AND** displays count of skipped unchanged files
- **AND** provides accurate time estimates based on actual workload

#### Scenario: Merkle Tree Consistency
- **WHEN** integrated file processing completes
- **THEN** all Merkle trees in database reflect current file state
- **AND** block hashes are synchronized with document content
- **AND** embedding database is updated only for changed blocks
- **AND** system is ready for sync operations with other instances