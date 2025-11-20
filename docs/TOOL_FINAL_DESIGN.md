# Crucible Tool System - Final Design

**Date**: 2025-11-20
**Informed by**: MCP best practices, Claude Code patterns, existing codebase architecture

## Design Principles

1. **Domain-aware operations** - Not generic CRUD, but note-specific actions
2. **Paths as primary interface** - Full relative paths from kiln root
3. **Efficient metadata access** - Read metadata without reading full content
4. **Partial file reads** - Support line ranges for large files
5. **Permission controls** - User approval for write operations
6. **Flexible search** - Fast text/property search + powerful semantic search

## Final Tool Set (10 tools)

### NoteTools (6 tools) - Note-specific operations

#### 1. `create_note`
Create a new note with optional frontmatter.

```rust
#[derive(Deserialize, JsonSchema)]
struct CreateNoteParams {
    /// Relative path from kiln root (e.g., "projects/work/proposal.md")
    path: String,

    /// Note content in Markdown
    content: String,

    /// Optional frontmatter properties (tags, custom fields, etc.)
    /// Will be serialized as YAML frontmatter
    #[serde(default)]
    frontmatter: Option<serde_json::Value>,
}
```

**Returns**:
```json
{
  "path": "projects/work/proposal.md",
  "status": "created",
  "word_count": 245,
  "char_count": 1523
}
```

#### 2. `read_note`
Read note content with optional line range support.

```rust
#[derive(Deserialize, JsonSchema)]
struct ReadNoteParams {
    /// Relative path from kiln root
    path: String,

    /// Optional: Start line (1-indexed, inclusive)
    #[serde(default)]
    start_line: Option<usize>,

    /// Optional: End line (1-indexed, inclusive)
    #[serde(default)]
    end_line: Option<usize>,
}
```

**Returns**:
```json
{
  "path": "projects/work/proposal.md",
  "content": "# Project Proposal\n\nContent here...",
  "total_lines": 150,
  "lines_returned": 150
}
```

**Use cases**:
- Full read: `read_note(path: "note.md")`
- First 50 lines: `read_note(path: "note.md", end_line: 50)`
- Lines 100-150: `read_note(path: "note.md", start_line: 100, end_line: 150)`

#### 3. `read_metadata`
Read ONLY metadata (frontmatter + structural stats) without full content.

```rust
#[derive(Deserialize, JsonSchema)]
struct ReadMetadataParams {
    /// Relative path from kiln root
    path: String,
}
```

**Returns**:
```json
{
  "path": "projects/work/proposal.md",
  "frontmatter": {
    "tags": ["project", "important"],
    "status": "draft",
    "created": "2025-11-20"
  },
  "stats": {
    "word_count": 1245,
    "char_count": 7834,
    "heading_count": 12,
    "code_block_count": 3,
    "wikilink_count": 8
  },
  "modified": "2025-11-20T10:30:00Z"
}
```

**Why separate from `read_note`?**
- Agents can browse metadata cheaply without loading full content
- Essential for "list all notes with status=draft" workflows
- Avoids "parameter explosion" on `read_note`

#### 4. `update_note`
Update note content and/or frontmatter.

```rust
#[derive(Deserialize, JsonSchema)]
struct UpdateNoteParams {
    /// Relative path from kiln root
    path: String,

    /// New content (if updating content)
    /// If None, content unchanged
    content: Option<String>,

    /// New frontmatter properties (if updating frontmatter)
    /// If None, frontmatter unchanged
    /// If Some, completely replaces existing frontmatter
    frontmatter: Option<serde_json::Value>,
}
```

**Returns**:
```json
{
  "path": "projects/work/proposal.md",
  "status": "updated",
  "changes": {
    "content_updated": true,
    "frontmatter_updated": false
  },
  "word_count": 1350
}
```

**Use cases**:
- Update content only: `update_note(path, content: "new content")`
- Update frontmatter only: `update_note(path, frontmatter: {tags: ["new"]})`
- Update both: `update_note(path, content: "...", frontmatter: {...})`

#### 5. `delete_note`
Delete a note with safety checks.

```rust
#[derive(Deserialize, JsonSchema)]
struct DeleteNoteParams {
    /// Relative path from kiln root
    path: String,
}
```

**Returns**:
```json
{
  "path": "projects/work/old-proposal.md",
  "status": "deleted",
  "had_backlinks": true,
  "backlink_count": 3
}
```

**Permission behavior**: Warns user if note has incoming wikilinks.

#### 6. `list_notes`
List notes in a directory with optional metadata.

```rust
#[derive(Deserialize, JsonSchema)]
struct ListNotesParams {
    /// Optional folder path (relative to kiln root)
    /// If None, lists entire kiln
    folder: Option<String>,

    /// Include frontmatter in results
    #[serde(default)]
    include_frontmatter: bool,

    /// Recursive listing
    #[serde(default = "default_true")]
    recursive: bool,
}
```

**Returns**:
```json
{
  "folder": "projects/work",
  "notes": [
    {
      "path": "projects/work/proposal.md",
      "name": "proposal.md",
      "word_count": 1245,
      "modified": "2025-11-20T10:30:00Z",
      "frontmatter": {
        "tags": ["project"],
        "status": "draft"
      }
    }
  ],
  "count": 1
}
```

---

### SearchTools (3 tools) - Fast and semantic search

#### 7. `text_search`
Fast full-text search using ripgrep.

```rust
#[derive(Deserialize, JsonSchema)]
struct TextSearchParams {
    /// Search query (supports regex if pattern starts with 'regex:')
    query: String,

    /// Optional: Limit search to folder
    folder: Option<String>,

    /// Case insensitive search
    #[serde(default = "default_true")]
    case_insensitive: bool,

    /// Maximum results to return
    #[serde(default = "default_top_k")]
    limit: usize,
}
```

**Returns**:
```json
{
  "query": "TODO",
  "matches": [
    {
      "path": "projects/work/tasks.md",
      "line_number": 42,
      "line_content": "- TODO: Finish implementation",
      "context_before": ["## Open Items", ""],
      "context_after": ["- TODO: Write tests"]
    }
  ],
  "count": 15,
  "truncated": false
}
```

**Use cases**:
- Find TODOs: `text_search("TODO")`
- Find regex: `text_search("regex:deadline:\\s*\\d{4}")`
- In folder: `text_search("important", folder: "projects")`

#### 8. `property_search`
Search by frontmatter properties (includes tag search).

```rust
#[derive(Deserialize, JsonSchema)]
struct PropertySearchParams {
    /// Properties to match (all must match - AND logic)
    /// Use array values for OR logic: {"tags": ["urgent", "important"]}
    properties: serde_json::Value,

    /// Maximum results to return
    #[serde(default = "default_top_k")]
    limit: usize,
}
```

**Returns**:
```json
{
  "properties": {"status": "draft", "tags": ["project"]},
  "matches": [
    {
      "path": "projects/work/proposal.md",
      "frontmatter": {
        "status": "draft",
        "tags": ["project", "important"],
        "created": "2025-11-20"
      },
      "word_count": 1245
    }
  ],
  "count": 3
}
```

**Use cases**:
- Find drafts: `property_search({status: "draft"})`
- Find by tag: `property_search({tags: ["urgent"]})`
- Multiple tags (OR): `property_search({tags: ["urgent", "important"]})`
- Complex: `property_search({status: "draft", tags: ["project"]})`

**Why combined tag/property search?**
- Tags ARE properties (in frontmatter)
- Avoids duplication
- More flexible (can search any property combination)

#### 9. `semantic_search`
Semantic similarity search using embeddings.

```rust
#[derive(Deserialize, JsonSchema)]
struct SemanticSearchParams {
    /// Natural language query
    query: String,

    /// Optional: Filter by properties before searching
    filters: Option<serde_json::Value>,

    /// Maximum results to return
    #[serde(default = "default_top_k")]
    limit: usize,
}
```

**Returns**:
```json
{
  "query": "machine learning techniques for text classification",
  "results": [
    {
      "path": "research/ml-notes.md",
      "score": 0.89,
      "snippet": "...discusses various ML approaches including SVMs, neural networks...",
      "frontmatter": {"tags": ["machine-learning", "research"]},
      "word_count": 2341
    }
  ],
  "count": 10
}
```

**Use cases**:
- Conceptual search: `semantic_search("project planning techniques")`
- With filters: `semantic_search("rust async", filters: {tags: ["programming"]})`

---

### KilnTools (2 tools) - System operations

#### 10. `get_kiln_info`
Get kiln statistics and root information.

```rust
#[derive(Deserialize, JsonSchema)]
struct GetKilnInfoParams {
    /// Include detailed statistics
    #[serde(default)]
    detailed: bool,
}
```

**Returns**:
```json
{
  "roots": [
    {
      "uri": "file:///home/user/my-kiln",
      "name": "My Knowledge Base"
    }
  ],
  "stats": {
    "total_notes": 342,
    "total_size_bytes": 5242880,
    "total_words": 125340,
    "indexed_notes": 340,
    "pending_index": 2
  }
}
```

**Note**: Combined `get_roots` + `get_stats` into single tool for efficiency.

---

## Tool Comparison: Before → After

| Before (Plan v1) | After (Final) | Reason |
|------------------|---------------|---------|
| 17 tools | 10 tools | Removed redundancy |
| `read_note(include_metadata)` | `read_metadata()` separate | Can't read JUST metadata otherwise |
| No line range support | `read_note(start_line, end_line)` | Essential for large files |
| `tag_search()` separate | Merged into `property_search()` | Tags are properties |
| `find_related_notes()` | ❌ Removed | Wikilink resolution is internal |
| `add_tag()` / `remove_tag()` | ❌ Removed | Use `update_note(frontmatter)` |
| `get_roots()` + `get_stats()` | Combined `get_kiln_info()` | Single call more efficient |

---

## Implementation Details

### Frontmatter Handling

**Read**:
```rust
async fn read_metadata(&self, params: Parameters<ReadMetadataParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let full_path = Path::new(&self.kiln_path).join(&params.path);

    // Parse file (already does this efficiently)
    let parsed = self.parser.parse_file(&full_path).await?;

    // Extract frontmatter properties
    let frontmatter = parsed.content.frontmatter
        .as_ref()
        .map(|fm| fm.properties().clone())
        .unwrap_or_default();

    // Return metadata only
    Ok(CallToolResult::success(vec![
        rmcp::model::Content::json(serde_json::json!({
            "path": params.path,
            "frontmatter": frontmatter,
            "stats": {
                "word_count": parsed.metadata.word_count,
                "char_count": parsed.metadata.char_count,
                "heading_count": parsed.metadata.heading_count,
                "code_block_count": parsed.metadata.code_block_count,
                "wikilink_count": parsed.content.wikilinks.len(),
            },
            "modified": parsed.modified,
        }))?
    ]))
}
```

**Write**:
```rust
async fn create_note(&self, params: Parameters<CreateNoteParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let full_path = Path::new(&self.kiln_path).join(&params.path);

    // Build content with optional frontmatter
    let content = if let Some(fm) = params.frontmatter {
        let yaml = serde_yaml::to_string(&fm)?;
        format!("---\n{}\n---\n\n{}", yaml, params.content)
    } else {
        params.content
    };

    // Permission check
    let operation = ToolOperation {
        tool_name: "create_note",
        operation_type: OperationType::Create,
        scope: PermissionScope::WriteCreate {
            target: full_path.clone(),
            size_bytes: content.len() as u64,
        },
        description: format!("Create note '{}'", params.path),
    };

    if !self.permissions.request_approval(&operation).await? {
        return Err(rmcp::ErrorData::invalid_params("User denied permission", None));
    }

    // Write file
    tokio::fs::write(&full_path, &content).await?;

    // Parse to get stats for response
    let parsed = self.parser.parse_file(&full_path).await?;

    Ok(CallToolResult::success(vec![
        rmcp::model::Content::json(serde_json::json!({
            "path": params.path,
            "status": "created",
            "word_count": parsed.metadata.word_count,
            "char_count": parsed.metadata.char_count,
        }))?
    ]))
}
```

### Line Range Reading

```rust
async fn read_note(&self, params: Parameters<ReadNoteParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let full_path = Path::new(&self.kiln_path).join(&params.path);
    let content = tokio::fs::read_to_string(&full_path).await?;

    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Apply line range if specified
    let (content_slice, lines_returned) = match (params.start_line, params.end_line) {
        (Some(start), Some(end)) => {
            let start_idx = (start.saturating_sub(1)).min(total_lines);
            let end_idx = end.min(total_lines);
            let slice = lines[start_idx..end_idx].join("\n");
            (slice, end_idx - start_idx)
        }
        (None, Some(end)) => {
            let end_idx = end.min(total_lines);
            let slice = lines[..end_idx].join("\n");
            (slice, end_idx)
        }
        (Some(start), None) => {
            let start_idx = (start.saturating_sub(1)).min(total_lines);
            let slice = lines[start_idx..].join("\n");
            (slice, total_lines - start_idx)
        }
        (None, None) => {
            (content, total_lines)
        }
    };

    Ok(CallToolResult::success(vec![
        rmcp::model::Content::json(serde_json::json!({
            "path": params.path,
            "content": content_slice,
            "total_lines": total_lines,
            "lines_returned": lines_returned,
        }))?
    ]))
}
```

### Property Search Implementation

```rust
async fn property_search(&self, params: Parameters<PropertySearchParams>)
    -> Result<CallToolResult, rmcp::ErrorData>
{
    let search_props = params.properties.as_object()
        .ok_or_else(|| rmcp::ErrorData::invalid_params("properties must be object", None))?;

    let mut matches = Vec::new();

    // Walk all notes in kiln
    for entry in WalkDir::new(&self.kiln_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
    {
        // Parse note to get frontmatter
        let parsed = self.parser.parse_file(entry.path()).await?;

        let Some(frontmatter) = &parsed.content.frontmatter else {
            continue;
        };

        let props = frontmatter.properties();

        // Check if all search properties match
        let matches_all = search_props.iter().all(|(key, search_value)| {
            props.get(key).map_or(false, |prop_value| {
                // Handle array values as OR logic
                if let Some(search_array) = search_value.as_array() {
                    // Property value must match any of the search values
                    if let Some(prop_array) = prop_value.as_array() {
                        // Array intersection
                        search_array.iter().any(|sv| prop_array.contains(sv))
                    } else {
                        // Single value must match any search value
                        search_array.contains(prop_value)
                    }
                } else {
                    // Exact match
                    prop_value == search_value
                }
            })
        });

        if matches_all {
            let relative_path = entry.path()
                .strip_prefix(&self.kiln_path)
                .unwrap()
                .to_string_lossy();

            matches.push(serde_json::json!({
                "path": relative_path,
                "frontmatter": props,
                "word_count": parsed.metadata.word_count,
            }));

            if matches.len() >= params.limit {
                break;
            }
        }
    }

    Ok(CallToolResult::success(vec![
        rmcp::model::Content::json(serde_json::json!({
            "properties": params.properties,
            "matches": matches,
            "count": matches.len(),
        }))?
    ]))
}
```

---

## Permission System Integration

All write operations (`create_note`, `update_note`, `delete_note`) require user approval:

```rust
pub struct PermissionManager {
    working_directory: PathBuf,
    auto_approve_settings: HashMap<OperationType, bool>,
    approval_fn: Arc<dyn Fn(&ToolOperation) -> Pin<Box<dyn Future<Output = Result<bool>> + Send>> + Send + Sync>,
}

impl PermissionManager {
    async fn request_approval(&self, operation: &ToolOperation) -> Result<bool> {
        // Check auto-approve first
        if self.is_auto_approved(operation) {
            return Ok(true);
        }

        // Read operations within scope: auto-approve
        if matches!(operation.scope, PermissionScope::ReadInScope(_)) {
            return Ok(true);
        }

        // Call approval function (provided by CLI)
        (self.approval_fn)(operation).await
    }
}
```

---

## Migration Path

### Phase 1: Complete search tools (Week 1)
- ✅ `semantic_search` already works
- Implement `text_search` (ripgrep integration)
- Implement `property_search` (frontmatter filtering)

### Phase 2: Add `read_metadata` (Week 1)
- New tool: `read_metadata(path)` → returns frontmatter + stats only
- Refactor `read_note` to support line ranges

### Phase 3: Frontmatter support in CRUD (Week 1-2)
- Add `frontmatter` param to `create_note`
- Add `frontmatter` param to `update_note`

### Phase 4: Permission system (Week 2)
- Implement `PermissionManager`
- CLI integration for approval prompts
- Session-based auto-approve

### Phase 5: Polish (Week 2-3)
- Tests for all tools
- Documentation
- Integration with ACP

---

## Open Questions

1. **Line range efficiency**: Should we use `BufReader` and skip lines, or read full file? (Probably full file for simplicity)

2. **Property search performance**: Walk entire kiln each time, or build an index? (Walk for now, optimize later)

3. **Frontmatter format preference**: Always use YAML, or detect/preserve existing format? (Probably always YAML for simplicity)

4. **MCP resources**: Should notes also be exposed as MCP resources? (Probably yes, for `mcp://kiln/path/to/note.md` URIs)

---

## Summary

**Final tool count: 10 tools**
- NoteTools (6): create_note, read_note, read_metadata, update_note, delete_note, list_notes
- SearchTools (3): text_search, property_search, semantic_search
- KilnTools (1): get_kiln_info

**Key improvements over initial plan**:
- ✅ Separate `read_metadata()` for metadata-only reads
- ✅ Line range support in `read_note()`
- ✅ Combined tag/property search (tags are properties)
- ✅ Frontmatter integrated into CRUD operations
- ✅ Paths as primary interface (no note resolver needed)
- ✅ Cleaner, more focused tool set (10 vs 17)
