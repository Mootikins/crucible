# Tool System Architecture - Technical Details

**Companion to**: `TOOL_REFACTORING_PLAN.md`
**Date**: 2025-11-20

## Table of Contents

1. [Note Resolution System](#note-resolution-system)
2. [Permission System](#permission-system)
3. [Storage Abstraction](#storage-abstraction)
4. [Error Handling](#error-handling)
5. [Tool Parameter Design](#tool-parameter-design)

---

## Note Resolution System

### Problem Statement

**Current**: Tools accept `path: String` parameters (e.g., "folder/note.md")
**Required**: Tools accept `name: String` parameters (e.g., "My Note" or "[[My Note]]")

The resolver abstracts away storage details and allows agents to think in terms of note names, not filesystem paths.

### Interface Design

```rust
/// Resolves note references (names/wikilinks) to storage locations
#[async_trait]
pub trait NoteResolver: Send + Sync {
    /// Resolve a note reference to its storage representation
    ///
    /// # Arguments
    /// * `reference` - Note name, wikilink, or path-qualified name
    ///
    /// # Returns
    /// * `Ok(ResolvedNote)` - Single unambiguous match found
    /// * `Err(ResolverError::NotFound)` - No matches found
    /// * `Err(ResolverError::Ambiguous)` - Multiple matches found
    async fn resolve(&self, reference: &str) -> Result<ResolvedNote, ResolverError>;

    /// Find all notes matching a reference pattern
    ///
    /// Useful for disambiguation when resolve() returns Ambiguous
    async fn find_matches(&self, reference: &str) -> Result<Vec<NoteCandidate>, ResolverError>;

    /// Check if a note name is available (for create operations)
    async fn is_available(&self, name: &str, folder: Option<&str>) -> Result<bool, ResolverError>;
}

/// Successfully resolved note reference
#[derive(Debug, Clone)]
pub struct ResolvedNote {
    /// Canonical note name (without extension or path)
    pub name: String,

    /// Storage-specific identifier (opaque to tools)
    pub storage_id: StorageId,

    /// Basic metadata for display
    pub metadata: BasicMetadata,
}

/// Candidate note when multiple matches exist
#[derive(Debug, Clone)]
pub struct NoteCandidate {
    pub name: String,
    pub path_context: String,  // e.g., "projects/work/"
    pub storage_id: StorageId,
    pub last_modified: SystemTime,
}

/// Storage-agnostic identifier
#[derive(Debug, Clone)]
pub enum StorageId {
    FilePath(PathBuf),           // For file-based storage
    DatabaseId(String),          // For SurrealDB storage
    Url(String),                 // For remote storage
}

#[derive(Debug, Clone)]
pub struct BasicMetadata {
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub size_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("Note not found: {0}")]
    NotFound(String),

    #[error("Ambiguous reference '{0}': {1} matches found")]
    Ambiguous(String, usize),

    #[error("Invalid reference format: {0}")]
    InvalidFormat(String),

    #[error("Storage error: {0}")]
    Storage(String),
}
```

### Reference Format Parsing

```rust
/// Parse different note reference formats
pub struct ReferenceParser;

impl ReferenceParser {
    /// Parse a reference string into structured format
    pub fn parse(reference: &str) -> ParsedReference {
        // Strip wikilink brackets if present: [[Note]] -> Note
        let cleaned = reference
            .trim()
            .strip_prefix("[[")
            .and_then(|s| s.strip_suffix("]]"))
            .unwrap_or(reference);

        // Check for section links: Note#Section
        let (base, section) = cleaned
            .split_once('#')
            .map(|(b, s)| (b, Some(s.to_string())))
            .unwrap_or((cleaned, None));

        // Check for aliases: Note|Alias
        let (name, alias) = base
            .split_once('|')
            .map(|(n, a)| (n, Some(a.to_string())))
            .unwrap_or((base, None));

        // Check for path context: folder/Note
        let (folder, note_name) = name
            .rsplit_once('/')
            .map(|(f, n)| (Some(f.to_string()), n))
            .unwrap_or((None, name));

        ParsedReference {
            note_name: note_name.to_string(),
            folder_context: folder,
            section,
            alias,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedReference {
    pub note_name: String,
    pub folder_context: Option<String>,
    pub section: Option<String>,
    pub alias: Option<String>,
}
```

### File-Based Implementation

```rust
/// File-based implementation of NoteResolver
pub struct FileNoteResolver {
    kiln_root: PathBuf,
    /// Cache of name -> paths for performance
    name_cache: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
}

impl FileNoteResolver {
    pub fn new(kiln_root: PathBuf) -> Self {
        Self {
            kiln_root,
            name_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Build cache by scanning filesystem
    pub async fn refresh_cache(&self) -> Result<(), ResolverError> {
        let mut cache = self.name_cache.write().await;
        cache.clear();

        // Walk filesystem and build name -> paths mapping
        for entry in WalkDir::new(&self.kiln_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "md") {
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();
                    cache.entry(name).or_default().push(path.to_path_buf());
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl NoteResolver for FileNoteResolver {
    async fn resolve(&self, reference: &str) -> Result<ResolvedNote, ResolverError> {
        let parsed = ReferenceParser::parse(reference);

        // Try to get cached matches
        let cache = self.name_cache.read().await;
        let matches = cache
            .get(&parsed.note_name)
            .ok_or_else(|| ResolverError::NotFound(parsed.note_name.clone()))?;

        // Filter by folder context if provided
        let filtered: Vec<_> = if let Some(folder) = parsed.folder_context {
            matches
                .iter()
                .filter(|path| {
                    path.parent()
                        .and_then(|p| p.strip_prefix(&self.kiln_root).ok())
                        .map(|p| p.to_string_lossy().starts_with(&folder))
                        .unwrap_or(false)
                })
                .collect()
        } else {
            matches.iter().collect()
        };

        match filtered.len() {
            0 => Err(ResolverError::NotFound(parsed.note_name)),
            1 => {
                let path = filtered[0];
                let metadata = tokio::fs::metadata(path).await
                    .map_err(|e| ResolverError::Storage(e.to_string()))?;

                Ok(ResolvedNote {
                    name: parsed.note_name,
                    storage_id: StorageId::FilePath(path.clone()),
                    metadata: BasicMetadata {
                        created: metadata.created().ok(),
                        modified: metadata.modified().ok(),
                        size_bytes: metadata.len(),
                    },
                })
            }
            n => Err(ResolverError::Ambiguous(parsed.note_name, n)),
        }
    }

    async fn find_matches(&self, reference: &str) -> Result<Vec<NoteCandidate>, ResolverError> {
        let parsed = ReferenceParser::parse(reference);
        let cache = self.name_cache.read().await;

        let matches = cache
            .get(&parsed.note_name)
            .ok_or_else(|| ResolverError::NotFound(parsed.note_name.clone()))?;

        let mut candidates = Vec::new();
        for path in matches {
            if let Ok(metadata) = tokio::fs::metadata(path).await {
                let relative = path.strip_prefix(&self.kiln_root)
                    .unwrap_or(path);

                candidates.push(NoteCandidate {
                    name: parsed.note_name.clone(),
                    path_context: relative.parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    storage_id: StorageId::FilePath(path.clone()),
                    last_modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }

        Ok(candidates)
    }

    async fn is_available(&self, name: &str, folder: Option<&str>) -> Result<bool, ResolverError> {
        let cache = self.name_cache.read().await;

        if let Some(matches) = cache.get(name) {
            if let Some(folder) = folder {
                // Check if name is available in specific folder
                let exists_in_folder = matches.iter().any(|path| {
                    path.parent()
                        .and_then(|p| p.strip_prefix(&self.kiln_root).ok())
                        .map(|p| p.to_string_lossy().starts_with(folder))
                        .unwrap_or(false)
                });
                Ok(!exists_in_folder)
            } else {
                // Name exists somewhere
                Ok(false)
            }
        } else {
            // Name doesn't exist anywhere
            Ok(true)
        }
    }
}
```

---

## Permission System

### Problem Statement

**Current**: All operations execute immediately without user approval
**Required**: Write operations require user approval, with session-based auto-approve

### Interface Design

```rust
/// Manages permissions and user approval for tool operations
#[async_trait]
pub trait PermissionManager: Send + Sync {
    /// Check if an operation requires user approval
    fn requires_approval(&self, operation: &ToolOperation) -> bool;

    /// Request user approval for an operation
    ///
    /// Returns true if approved, false if denied
    async fn request_approval(&self, operation: &ToolOperation) -> Result<bool>;

    /// Check if operation is auto-approved for current session
    fn is_auto_approved(&self, operation: &ToolOperation) -> bool;

    /// Enable auto-approve for operation type in current session
    fn set_auto_approve(&mut self, operation_type: OperationType, enabled: bool);
}

/// Describes a tool operation for permission checking
#[derive(Debug, Clone)]
pub struct ToolOperation {
    pub tool_name: String,
    pub operation_type: OperationType,
    pub scope: PermissionScope,
    pub description: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Read,
    Create,
    Update,
    Delete,
    Administrative,
}

#[derive(Debug, Clone)]
pub enum PermissionScope {
    /// Reading within current working directory
    ReadInScope(PathBuf),

    /// Reading outside current working directory
    ReadOutOfScope(PathBuf),

    /// Creating new content
    WriteCreate {
        target: PathBuf,
        size_bytes: u64,
    },

    /// Modifying existing content
    WriteUpdate {
        target: PathBuf,
        size_bytes: u64,
    },

    /// Deleting content
    WriteDelete {
        target: PathBuf,
        has_backlinks: bool,
    },

    /// Administrative operations
    Admin {
        operation: String,
        affects_count: usize,
    },
}
```

### Default Implementation

```rust
/// Default permission manager with configurable rules
pub struct DefaultPermissionManager {
    /// Current working directory (kiln root)
    working_directory: PathBuf,

    /// Session-level auto-approve settings
    auto_approve_settings: HashMap<OperationType, bool>,

    /// Approval function (CLI provides this)
    approval_fn: Arc<dyn Fn(&ToolOperation) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>> + Send + Sync>,

    /// Audit log
    audit_log: Arc<Mutex<Vec<AuditEntry>>>,
}

impl DefaultPermissionManager {
    pub fn new(
        working_directory: PathBuf,
        approval_fn: impl Fn(&ToolOperation) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            working_directory,
            auto_approve_settings: HashMap::new(),
            approval_fn: Arc::new(approval_fn),
            audit_log: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl PermissionManager for DefaultPermissionManager {
    fn requires_approval(&self, operation: &ToolOperation) -> bool {
        match &operation.scope {
            // Read within scope: no approval needed
            PermissionScope::ReadInScope(_) => false,

            // All other operations need approval (unless auto-approved)
            _ => !self.is_auto_approved(operation),
        }
    }

    async fn request_approval(&self, operation: &ToolOperation) -> Result<bool> {
        // Check auto-approve first
        if self.is_auto_approved(operation) {
            self.log_operation(operation, true, "auto-approved").await;
            return Ok(true);
        }

        // Check if approval is needed
        if !self.requires_approval(operation) {
            self.log_operation(operation, true, "no-approval-needed").await;
            return Ok(true);
        }

        // Request approval from user
        let approved = (self.approval_fn)(operation).await?;

        self.log_operation(
            operation,
            approved,
            if approved { "user-approved" } else { "user-denied" },
        ).await;

        Ok(approved)
    }

    fn is_auto_approved(&self, operation: &ToolOperation) -> bool {
        self.auto_approve_settings
            .get(&operation.operation_type)
            .copied()
            .unwrap_or(false)
    }

    fn set_auto_approve(&mut self, operation_type: OperationType, enabled: bool) {
        self.auto_approve_settings.insert(operation_type, enabled);
    }
}

impl DefaultPermissionManager {
    async fn log_operation(&self, operation: &ToolOperation, approved: bool, reason: &str) {
        let entry = AuditEntry {
            timestamp: SystemTime::now(),
            tool_name: operation.tool_name.clone(),
            operation_type: operation.operation_type,
            approved,
            reason: reason.to_string(),
            details: operation.details.clone(),
        };

        self.audit_log.lock().await.push(entry);
    }

    pub async fn get_audit_log(&self) -> Vec<AuditEntry> {
        self.audit_log.lock().await.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: SystemTime,
    pub tool_name: String,
    pub operation_type: OperationType,
    pub approved: bool,
    pub reason: String,
    pub details: HashMap<String, String>,
}
```

### CLI Integration

```rust
/// CLI provides this function to DefaultPermissionManager
async fn cli_approval_function(operation: &ToolOperation) -> Result<bool> {
    println!("\n🔒 Permission Required");
    println!("Tool: {}", operation.tool_name);
    println!("Operation: {:?}", operation.operation_type);
    println!("Description: {}", operation.description);

    match &operation.scope {
        PermissionScope::WriteCreate { target, size_bytes } => {
            println!("Action: Create new file");
            println!("Location: {}", target.display());
            println!("Size: {} bytes", size_bytes);
        }
        PermissionScope::WriteUpdate { target, size_bytes } => {
            println!("Action: Update existing file");
            println!("Location: {}", target.display());
            println!("New size: {} bytes", size_bytes);
        }
        PermissionScope::WriteDelete { target, has_backlinks } => {
            println!("Action: Delete file");
            println!("Location: {}", target.display());
            if *has_backlinks {
                println!("⚠️  Warning: This note has incoming links!");
            }
        }
        _ => {}
    }

    println!("\nOptions:");
    println!("  [y] Approve");
    println!("  [n] Deny");
    println!("  [a] Approve and auto-approve all {} operations", operation.operation_type);

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    match input.trim().to_lowercase().as_str() {
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        "a" | "auto" => {
            // Note: This would need to communicate back to set auto-approve
            // In practice, we'd use a channel or callback
            Ok(true)
        }
        _ => Ok(false),
    }
}
```

---

## Storage Abstraction

### Updated KnowledgeRepository Trait

The existing `KnowledgeRepository` trait needs new methods for note-centric operations:

```rust
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    // === Existing methods ===
    async fn get_note_by_name(&self, name: &str) -> Result<Option<ParsedNote>>;
    async fn list_notes(&self, path: Option<&str>) -> Result<Vec<NoteMetadata>>;
    async fn search_vectors(&self, vector: Vec<f32>) -> Result<Vec<SearchResult>>;

    // === New methods for tool system ===

    /// Create a new note at the resolved location
    async fn create_note(&self, location: &ResolvedNote, content: &str) -> Result<()>;

    /// Update an existing note
    async fn update_note(&self, location: &ResolvedNote, content: &str) -> Result<()>;

    /// Delete a note
    async fn delete_note(&self, location: &ResolvedNote) -> Result<()>;

    /// Get note metadata (tags, links, properties)
    async fn get_metadata(&self, location: &ResolvedNote) -> Result<NoteMetadataFull>;

    /// Find notes related by links or semantics
    async fn find_related(&self, location: &ResolvedNote, limit: usize) -> Result<Vec<RelatedNote>>;

    /// Search notes by tags
    async fn search_by_tags(&self, tags: &[String]) -> Result<Vec<SearchResult>>;

    /// Search notes by metadata properties
    async fn search_by_properties(&self, properties: &serde_json::Value) -> Result<Vec<SearchResult>>;

    /// Get all tags in the kiln
    async fn list_all_tags(&self) -> Result<Vec<TagInfo>>;

    /// Add a tag to a note
    async fn add_tag(&self, location: &ResolvedNote, tag: &str) -> Result<()>;

    /// Remove a tag from a note
    async fn remove_tag(&self, location: &ResolvedNote, tag: &str) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct NoteMetadataFull {
    pub name: String,
    pub tags: Vec<String>,
    pub properties: HashMap<String, serde_json::Value>,
    pub backlinks: Vec<String>,
    pub forwardlinks: Vec<String>,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct RelatedNote {
    pub name: String,
    pub relationship: RelationshipType,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub enum RelationshipType {
    BacklinkFrom,
    ForwardlinkTo,
    SemanticallyRelated,
    SharedTag(String),
}

#[derive(Debug, Clone)]
pub struct TagInfo {
    pub tag: String,
    pub count: usize,
    pub parent: Option<String>,
}
```

---

## Error Handling

### Unified Error Type for Tools

```rust
/// Errors that can occur during tool execution
#[derive(Debug, thiserror::Error)]
pub enum ToolExecutionError {
    #[error("Note resolution failed: {0}")]
    Resolution(#[from] ResolverError),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Storage operation failed: {0}")]
    Storage(#[from] crucible_core::Error),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Operation timeout after {0}ms")]
    Timeout(u64),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convert ToolExecutionError to rmcp::ErrorData
impl From<ToolExecutionError> for rmcp::ErrorData {
    fn from(err: ToolExecutionError) -> Self {
        match err {
            ToolExecutionError::Resolution(e) => {
                match e {
                    ResolverError::NotFound(name) => {
                        rmcp::ErrorData::invalid_params(
                            format!("Note not found: {}", name),
                            Some(serde_json::json!({
                                "error_type": "not_found",
                                "note_name": name
                            }))
                        )
                    }
                    ResolverError::Ambiguous(name, count) => {
                        rmcp::ErrorData::invalid_params(
                            format!("Ambiguous note reference '{}': {} matches", name, count),
                            Some(serde_json::json!({
                                "error_type": "ambiguous",
                                "note_name": name,
                                "match_count": count
                            }))
                        )
                    }
                    _ => rmcp::ErrorData::internal_error(e.to_string(), None),
                }
            }
            ToolExecutionError::PermissionDenied(msg) => {
                rmcp::ErrorData::invalid_params(
                    format!("Permission denied: {}", msg),
                    Some(serde_json::json!({"error_type": "permission_denied"}))
                )
            }
            ToolExecutionError::InvalidParameters(msg) => {
                rmcp::ErrorData::invalid_params(msg, None)
            }
            _ => rmcp::ErrorData::internal_error(err.to_string(), None),
        }
    }
}
```

---

## Tool Parameter Design

### Updated Parameter Structs

```rust
// ===== NoteTools Parameters =====

/// Create a new note (kiln-agnostic)
#[derive(Deserialize, JsonSchema)]
struct CreateNoteParams {
    /// Note name (without .md extension)
    /// Examples: "My Note", "Project Planning"
    #[schemars(description = "Note name (without extension)")]
    name: String,

    /// Note content in Markdown
    #[schemars(description = "Note content in Markdown format")]
    content: String,

    /// Optional folder context for disambiguation
    /// Examples: "projects/work", "archive"
    #[schemars(description = "Optional folder path for note location")]
    folder: Option<String>,
}

/// Read a note (kiln-agnostic)
#[derive(Deserialize, JsonSchema)]
struct ReadNoteParams {
    /// Note name or wikilink
    /// Examples: "My Note", "[[My Note]]", "projects/My Note"
    #[schemars(description = "Note name, wikilink, or path-qualified name")]
    name: String,
}

/// Update an existing note (kiln-agnostic)
#[derive(Deserialize, JsonSchema)]
struct UpdateNoteParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink to update")]
    name: String,

    /// New content
    #[schemars(description = "New content in Markdown format")]
    content: String,
}

/// Delete a note (kiln-agnostic)
#[derive(Deserialize, JsonSchema)]
struct DeleteNoteParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink to delete")]
    name: String,
}

// ===== SearchTools Parameters =====

/// Get note metadata
#[derive(Deserialize, JsonSchema)]
struct GetMetadataParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink")]
    name: String,
}

/// Find related notes
#[derive(Deserialize, JsonSchema)]
struct FindRelatedParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink")]
    name: String,

    /// Maximum number of results
    #[serde(default = "default_top_k")]
    #[schemars(description = "Maximum number of related notes to return")]
    limit: u32,
}

// ===== KilnTools Parameters =====

/// Add tag to note
#[derive(Deserialize, JsonSchema)]
struct AddTagParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink")]
    name: String,

    /// Tag to add (with or without # prefix)
    #[schemars(description = "Tag to add (e.g., 'important' or '#important')")]
    tag: String,
}

/// Remove tag from note
#[derive(Deserialize, JsonSchema)]
struct RemoveTagParams {
    /// Note name or wikilink
    #[schemars(description = "Note name or wikilink")]
    name: String,

    /// Tag to remove
    #[schemars(description = "Tag to remove")]
    tag: String,
}
```

---

## Summary

This architecture provides:

1. **Storage Agnosticism**: `NoteResolver` abstracts storage details
2. **Permission Control**: `PermissionManager` enforces approval requirements
3. **Extensibility**: Traits allow new backends and policies
4. **SOLID Compliance**: Dependencies inverted, responsibilities separated
5. **User Safety**: All writes require approval, with audit logging

The implementation can proceed incrementally:
1. Implement `NoteResolver` trait and file-based implementation
2. Implement `PermissionManager` trait and default implementation
3. Refactor `NoteTools` to use both
4. Extend `KnowledgeRepository` with new methods
5. Implement remaining tools

Each phase can be tested independently before integration.
