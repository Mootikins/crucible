//! Tests for [`super`]: the note search tools.
//!
//! A sibling file rather than an inline `mod tests`, for the file-size gate —
//! same shape as `agent_factory/tests.rs`.

use super::*;
use std::fs;
use tempfile::TempDir;

use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

fn create_search_tools(kiln_path: String) -> SearchTools {
    let knowledge_repo = Arc::new(MockKnowledgeRepository);
    let embedding_provider = Arc::new(MockEmbeddingProvider);
    SearchTools::new(kiln_path, knowledge_repo, embedding_provider)
}

// ===== semantic_search disclosure tests =====

/// A repository that answers every query with one hit, so the tool result
/// can be inspected. The hit is deliberately built with `kiln: None` —
/// `search_across_kilns` overwrites it from the source, which is the only
/// thing allowed to decide what a hit is attributed to.
struct OneHitRepository;

#[async_trait::async_trait]
impl KnowledgeRepository for OneHitRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }
    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }
    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
        _limit: usize,
    ) -> crucible_core::Result<Vec<crucible_core::SearchResult>> {
        Ok(vec![crucible_core::SearchResult {
            document_id: crucible_core::DocumentId("notes/Rust.md".to_string()),
            score: 0.9,
            highlights: None,
            snippet: Some("body".to_string()),
            kiln: None,
        }])
    }
}

/// The model asked for a query and gets notes back. It must not also get
/// the directory each note lives in — one absolute path per hit, straight
/// into the tool-result message.
#[tokio::test]
async fn semantic_search_never_returns_a_kiln_directory() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    let tools = SearchTools::new(
        kiln_path.clone(),
        Arc::new(OneHitRepository),
        Arc::new(MockEmbeddingProvider),
    );

    let result = tools
        .semantic_search(Parameters(SemanticSearchParams {
            query: "rust".to_string(),
            limit: 10,
        }))
        .await
        .expect("semantic_search succeeds");

    let parsed = parse_tool_json(&result);
    let rendered = serde_json::to_string(&parsed).unwrap();
    assert!(
        !rendered.contains(&kiln_path),
        "the kiln directory must not reach the model: {rendered}"
    );
    assert!(
        !rendered.contains("kiln_path"),
        "the kiln_path key is gone, not renamed: {rendered}"
    );
    // And nothing invented in its place: this source has no registry name,
    // so the key is absent rather than empty.
    let hit = &parsed["results"][0];
    assert_eq!(hit["id"], "notes/Rust.md");
    assert!(
        hit.get("kiln").is_none(),
        "an unregistered kiln yields no key at all: {hit}"
    );
}

/// With a registered source the name — and only the name — comes back.
#[tokio::test]
async fn semantic_search_returns_the_registry_name_when_there_is_one() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_path_buf();

    let tools = SearchTools::new(
        kiln_path.to_string_lossy().to_string(),
        Arc::new(MockKnowledgeRepository),
        Arc::new(MockEmbeddingProvider),
    )
    .with_search_sources(vec![KilnSearchSource {
        kiln_path: kiln_path.clone(),
        kiln_name: Some(crate::test_support::kiln_name("work-notes")),
        knowledge_repo: Arc::new(OneHitRepository),
    }]);

    let result = tools
        .semantic_search(Parameters(SemanticSearchParams {
            query: "rust".to_string(),
            limit: 10,
        }))
        .await
        .expect("semantic_search succeeds");

    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["results"][0]["kiln"], "work-notes");
    let rendered = serde_json::to_string(&parsed).unwrap();
    assert!(
        !rendered.contains(&kiln_path.to_string_lossy().to_string()),
        "the name replaces the directory, it does not accompany it: {rendered}"
    );
}

// ===== grep_notes tests =====

#[tokio::test]
async fn test_grep_notes_basic() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create test file
    fs::write(
        temp_dir.path().join("test.md"),
        "# Test Note\n\nThis contains TODO items.\n\nAnother line.",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: "TODO".to_string(),
        folder: None,
        regex: false,
        case_insensitive: true,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await;
    assert!(result.is_ok());

    let call_result = result.unwrap();
    let parsed = parse_tool_json(&call_result);
    assert_eq!(parsed["query"], "TODO");
    assert_eq!(parsed["count"], 1);

    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["line_content"]
        .as_str()
        .unwrap()
        .contains("TODO"));
}

#[tokio::test]
async fn test_grep_notes_case_sensitive() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("test.md"),
        "TODO in uppercase\ntodo in lowercase",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    // Case insensitive - should find both
    let params = Parameters(GrepNotesParams {
        query: "todo".to_string(),
        folder: None,
        regex: false,
        case_insensitive: true,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 2);

    // Case sensitive - should find only one
    let params = Parameters(GrepNotesParams {
        query: "todo".to_string(),
        folder: None,
        regex: false,
        case_insensitive: false,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1);
}

#[tokio::test]
async fn test_grep_notes_with_folder() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create subfolder with file
    fs::create_dir(temp_dir.path().join("subfolder")).unwrap();
    fs::write(
        temp_dir.path().join("subfolder/test.md"),
        "Match in subfolder",
    )
    .unwrap();

    fs::write(temp_dir.path().join("root.md"), "Match in root").unwrap();

    let search_tools = create_search_tools(kiln_path);

    // Search only in subfolder
    let params = Parameters(GrepNotesParams {
        query: "Match".to_string(),
        folder: Some("subfolder".to_string()),
        regex: false,
        case_insensitive: true,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1);
    let matches = parsed["matches"].as_array().unwrap();
    assert!(matches[0]["path"].as_str().unwrap().contains("subfolder"));
}

/// Naming a folder that cannot exist used to be an oracle for the kiln's
/// location: the refusal interpolated the *resolved* path, so one call with a
/// junk folder handed the model the absolute kiln root it was never given.
///
/// Asserted on the refusal's text rather than on the absence of a panic, and
/// on the kiln directory specifically rather than on a prefix — a message that
/// merely stopped saying "Search path" would still leak.
#[tokio::test]
async fn a_missing_folder_is_refused_without_naming_the_kiln_directory() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let search_tools = create_search_tools(kiln_path.clone());

    let params = Parameters(GrepNotesParams {
        query: "anything".to_string(),
        folder: Some("no-such-folder".to_string()),
        regex: false,
        case_insensitive: true,
        limit: 10,
    });

    let message = search_tools
        .grep_notes(params)
        .await
        .expect_err("a folder that does not exist must be refused")
        .message
        .to_string();

    assert!(
        !message.contains(&kiln_path),
        "the refusal named the kiln directory: {message}"
    );
    assert!(
        message.contains("no-such-folder"),
        "the refusal should name what the caller named: {message}"
    );
}

#[tokio::test]
async fn test_grep_notes_limit() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create file with multiple matches
    fs::write(
        temp_dir.path().join("test.md"),
        "match\nmatch\nmatch\nmatch\nmatch",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: "match".to_string(),
        folder: None,
        regex: false,
        case_insensitive: true,
        limit: 3,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 3); // Limited to 3
    assert_eq!(parsed["truncated"], true);
}

// ===== regex-mode tests =====

#[tokio::test]
async fn regex_mode_matches_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("test.md"),
        "foo123bar\nfoobar\nno match\n",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: r"foo[0-9]+bar".to_string(),
        folder: None,
        regex: true,
        case_insensitive: false,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1, "regex should match only foo123bar");
    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches[0]["line_content"], "foo123bar");
}

#[tokio::test]
async fn literal_mode_does_not_interpret_metachars() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // "foo.bar" as a regex would match "fooXbar"; as a literal it must not.
    fs::write(temp_dir.path().join("test.md"), "fooXbar\nfoo.bar\n").unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: "foo.bar".to_string(),
        folder: None,
        regex: false,
        case_insensitive: false,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1, "literal dot must not match fooXbar");
    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches[0]["line_content"], "foo.bar");
}

#[tokio::test]
async fn invalid_regex_returns_syntax_error() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    fs::write(temp_dir.path().join("test.md"), "content\n").unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: "foo(".to_string(),
        folder: None,
        regex: true,
        case_insensitive: false,
        limit: 10,
    });

    let err = search_tools
        .grep_notes(params)
        .await
        .expect_err("unbalanced paren should be rejected");
    assert!(
        err.message.contains("Invalid regex"),
        "error should identify the regex as invalid: {}",
        err.message
    );
    assert!(
        err.message.contains("unclosed group") || err.message.contains("error"),
        "error should name the syntax problem: {}",
        err.message
    );
}

#[tokio::test]
async fn grep_notes_reports_match_offsets() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(temp_dir.path().join("test.md"), "a needle here\n").unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(GrepNotesParams {
        query: "needle".to_string(),
        folder: None,
        regex: false,
        case_insensitive: false,
        limit: 10,
    });

    let result = search_tools.grep_notes(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    let matches = parsed["matches"].as_array().unwrap();
    assert_eq!(matches[0]["match_start"], 2);
    assert_eq!(matches[0]["match_end"], 8);
}

#[tokio::test]
async fn regex_mode_respects_case_sensitivity() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("test.md"),
        "TODO5 upper\ntodo7 lower\n",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path.clone());

    // Insensitive: both lines match the pattern.
    let params = Parameters(GrepNotesParams {
        query: r"todo[0-9]".to_string(),
        folder: None,
        regex: true,
        case_insensitive: true,
        limit: 10,
    });
    let parsed = parse_tool_json(&search_tools.grep_notes(params).await.unwrap());
    assert_eq!(parsed["count"], 2);

    // Sensitive: only the lowercase line matches.
    let params = Parameters(GrepNotesParams {
        query: r"todo[0-9]".to_string(),
        folder: None,
        regex: true,
        case_insensitive: false,
        limit: 10,
    });
    let parsed = parse_tool_json(&search_tools.grep_notes(params).await.unwrap());
    assert_eq!(parsed["count"], 1);
}

// ===== property_search tests =====

#[tokio::test]
async fn test_property_search_single_property() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    // Create note with frontmatter
    fs::write(
        temp_dir.path().join("draft.md"),
        "---\nstatus: draft\n---\n\nContent",
    )
    .unwrap();

    fs::write(
        temp_dir.path().join("published.md"),
        "---\nstatus: published\n---\n\nContent",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(PropertySearchParams {
        properties: serde_json::json!({"status": "draft"}),
        limit: 10,
    });

    let result = search_tools.property_search(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1);
    let matches = parsed["matches"].as_array().unwrap();
    assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
}

#[tokio::test]
async fn test_property_search_multiple_properties_and() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("match.md"),
        "---\nstatus: draft\npriority: high\n---\n\nContent",
    )
    .unwrap();

    fs::write(
        temp_dir.path().join("nomatch.md"),
        "---\nstatus: draft\npriority: low\n---\n\nContent",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(PropertySearchParams {
        properties: serde_json::json!({"status": "draft", "priority": "high"}),
        limit: 10,
    });

    let result = search_tools.property_search(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 1);
}

#[tokio::test]
async fn test_property_search_tags_or_logic() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("urgent.md"),
        "---\ntags: [urgent, work]\n---\n\nContent",
    )
    .unwrap();

    fs::write(
        temp_dir.path().join("important.md"),
        "---\ntags: [important, personal]\n---\n\nContent",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    // Search for notes with either "urgent" OR "important" tags
    let params = Parameters(PropertySearchParams {
        properties: serde_json::json!({"tags": ["urgent", "important"]}),
        limit: 10,
    });

    let result = search_tools.property_search(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 2); // Both should match
}

#[tokio::test]
async fn test_property_search_no_frontmatter() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();

    fs::write(
        temp_dir.path().join("no-fm.md"),
        "Just content, no frontmatter",
    )
    .unwrap();

    let search_tools = create_search_tools(kiln_path);

    let params = Parameters(PropertySearchParams {
        properties: serde_json::json!({"status": "draft"}),
        limit: 10,
    });

    let result = search_tools.property_search(params).await.unwrap();
    let parsed = parse_tool_json(&result);
    assert_eq!(parsed["count"], 0); // No matches
}

// ===== Helper function tests =====

#[test]
fn test_parse_yaml_frontmatter() {
    let content = "---\ntitle: Test\ntags: [one, two]\n---\n\nContent here";
    let fm = parse_yaml_frontmatter(content).unwrap();

    assert_eq!(fm["title"], "Test");
    assert_eq!(fm["tags"].as_array().unwrap().len(), 2);
}

#[test]
fn test_parse_yaml_frontmatter_none() {
    let content = "No frontmatter here";
    let fm = parse_yaml_frontmatter(content);

    assert!(fm.is_none());
}

// ===== Security Tests for Path Traversal =====

#[tokio::test]
async fn test_grep_notes_folder_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let search_tools = create_search_tools(kiln_path);

    let result = search_tools
        .grep_notes(Parameters(GrepNotesParams {
            query: "test".to_string(),
            folder: Some("../../../etc".to_string()),
            regex: false,
            case_insensitive: true,
            limit: 10,
        }))
        .await;

    assert!(
        result.is_err(),
        "Should reject path traversal in folder parameter"
    );
    if let Err(e) = result {
        assert!(
            e.message.contains("Path traversal"),
            "Error should mention path traversal"
        );
    }
}

#[tokio::test]
async fn test_grep_notes_absolute_folder() {
    let temp_dir = TempDir::new().unwrap();
    let kiln_path = temp_dir.path().to_string_lossy().to_string();
    let search_tools = create_search_tools(kiln_path);

    let result = search_tools
        .grep_notes(Parameters(GrepNotesParams {
            query: "test".to_string(),
            folder: Some("/etc".to_string()),
            regex: false,
            case_insensitive: true,
            limit: 10,
        }))
        .await;

    assert!(result.is_err(), "Should reject absolute path in folder");
    if let Err(e) = result {
        assert!(
            e.message.contains("Absolute paths are not allowed"),
            "Error should mention absolute paths"
        );
    }
}

/// Check a schema for common llama.cpp incompatible patterns.
///
/// Known issues:
/// - `"default": null` - caused by `#[serde(default)]` on `Option<T>` (redundant, just remove it)
/// - `"additionalProperties": true` - from `serde_json::Value` without custom schema
///
/// Use `#[schemars(schema_with = "...")]` for `serde_json::Value` fields.
/// Don't use `#[serde(default)]` on `Option<T>` - serde already treats missing Options as None.
fn check_schema_compatible(json: &str) -> Result<(), String> {
    if json.contains(r#""default":null"#) || json.contains(r#""default": null"#) {
        return Err(
            "Schema contains 'default: null' - remove #[serde(default)] from Option<T> fields"
                .into(),
        );
    }
    if json.contains(r#""additionalProperties":true"#)
        || json.contains(r#""additionalProperties": true"#)
    {
        return Err("Schema contains 'additionalProperties: true' - use #[schemars(schema_with = \"...\")] for serde_json::Value fields".into());
    }
    if (json.contains(r#""type":"object""#) || json.contains(r#""type": "object""#))
        && !json.contains(r#""properties""#)
    {
        return Err("Schema has type=object but missing properties key".into());
    }
    if json.contains(r#""$schema""#) {
        return Err("Schema contains $schema meta field".into());
    }
    if json.contains(r#""title""#) {
        return Err("Schema contains title meta field".into());
    }
    Ok(())
}

/// Validates all tool parameter schemas are compatible with llama.cpp's GBNF converter.
///
/// Common mistakes this catches:
/// - Using `#[serde(default)]` on `Option<T>` fields (generates `"default": null`)
/// - Using bare `serde_json::Value` without `#[schemars(schema_with = "...")]`
#[test]
fn test_tool_schemas_llama_cpp_compatible() {
    use crate::provider::tool_bridge::sanitize_tool_schema;
    use crate::tools::mcp_server::{
        CancelJobParams, DelegateSessionParams, GetJobResultParams, ListJobsParams,
    };
    use crate::tools::notes::{
        CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
        UpdateNoteParams,
    };

    let sanitize = |raw: schemars::Schema| -> String {
        let mut v: serde_json::Value = serde_json::to_value(&raw).unwrap();
        sanitize_tool_schema(&mut v);
        serde_json::to_string(&v).unwrap()
    };

    let schemas: &[(&str, String)] = &[
        (
            "GrepNotesParams",
            sanitize(schemars::schema_for!(GrepNotesParams)),
        ),
        (
            "SemanticSearchParams",
            sanitize(schemars::schema_for!(SemanticSearchParams)),
        ),
        (
            "PropertySearchParams",
            sanitize(schemars::schema_for!(PropertySearchParams)),
        ),
        (
            "CreateNoteParams",
            sanitize(schemars::schema_for!(CreateNoteParams)),
        ),
        (
            "ReadNoteParams",
            sanitize(schemars::schema_for!(ReadNoteParams)),
        ),
        (
            "ReadMetadataParams",
            sanitize(schemars::schema_for!(ReadMetadataParams)),
        ),
        (
            "UpdateNoteParams",
            sanitize(schemars::schema_for!(UpdateNoteParams)),
        ),
        (
            "DeleteNoteParams",
            sanitize(schemars::schema_for!(DeleteNoteParams)),
        ),
        (
            "ListNotesParams",
            sanitize(schemars::schema_for!(ListNotesParams)),
        ),
        (
            "ListJobsParams",
            sanitize(schemars::schema_for!(ListJobsParams)),
        ),
        (
            "GetJobResultParams",
            sanitize(schemars::schema_for!(GetJobResultParams)),
        ),
        (
            "CancelJobParams",
            sanitize(schemars::schema_for!(CancelJobParams)),
        ),
        (
            "DelegateSessionParams",
            sanitize(schemars::schema_for!(DelegateSessionParams)),
        ),
    ];

    let mut errors = Vec::new();
    for (name, json) in schemas {
        if let Err(e) = check_schema_compatible(json) {
            errors.push(format!("{name}: {e}"));
        }
    }

    assert!(
        errors.is_empty(),
        "Schema compatibility issues:\n{}",
        errors.join("\n")
    );
}

#[test]
fn test_check_schema_catches_missing_properties() {
    // Raw schema with type=object but no properties (like ListJobsParams before sanitization)
    let raw = r#"{"type":"object"}"#;
    assert!(
        check_schema_compatible(raw).is_err(),
        "Should reject schema with type=object but missing properties"
    );

    // Schema with $schema field
    let with_schema_field = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
    assert!(
        check_schema_compatible(with_schema_field).is_err(),
        "Should reject schema with $schema field"
    );

    // Schema with title field
    let with_title = r#"{"title":"Foo","type":"object","properties":{}}"#;
    assert!(
        check_schema_compatible(with_title).is_err(),
        "Should reject schema with title field"
    );

    // Valid schema should pass
    let valid = r#"{"type":"object","properties":{}}"#;
    assert!(
        check_schema_compatible(valid).is_ok(),
        "Should accept valid schema"
    );
}

// ===== NoteStore Integration Tests =====
// These tests verify the property_search NoteStore code path works correctly

#[cfg(test)]
mod note_store_tests {
    use super::super::*;
    use async_trait::async_trait;
    use crucible_core::events::{InternalSessionEvent, NoteChangeType, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, StorageResult};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use tempfile::TempDir;

    use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};

    /// Mock NoteStore for testing the NoteStore integration path
    struct MockNoteStore {
        notes: Mutex<HashMap<String, NoteRecord>>,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self {
                notes: Mutex::new(HashMap::new()),
            }
        }

        fn add_note(&self, record: NoteRecord) {
            let mut notes = self.notes.lock().unwrap();
            notes.insert(record.path.clone(), record);
        }
    }

    fn note(
        path: impl Into<String>,
        title: impl Into<String>,
        tags: Vec<String>,
        properties: HashMap<String, serde_json::Value>,
    ) -> NoteRecord {
        NoteRecord::new(path, BlockHash::zero())
            .with_title(title)
            .with_tags(tags)
            .with_properties(properties)
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
            self.add_note(note.clone());
            let existed = self.notes.lock().unwrap().contains_key(&note.path);
            let event = if existed {
                SessionEvent::internal(InternalSessionEvent::NoteModified {
                    path: note.path.into(),
                    change_type: NoteChangeType::Content,
                })
            } else {
                SessionEvent::internal(InternalSessionEvent::NoteCreated {
                    path: note.path.into(),
                    title: Some(note.title),
                })
            };
            Ok(vec![event])
        }

        async fn get(
            &self,
            path: &str,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.get(path).cloned())
        }

        async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
            let mut notes = self.notes.lock().unwrap();
            let existed = notes.remove(path).is_some();
            Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: path.into(),
                existed,
            }))
        }

        async fn list(
            &self,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Vec<NoteRecord>> {
            let notes = self.notes.lock().unwrap();
            Ok(notes.values().cloned().collect())
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
            _authority: &crucible_core::storage::Scope,
        ) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<crucible_core::storage::note_store::SearchResult>> {
            Ok(vec![])
        }
    }

    fn create_search_tools_with_store(
        kiln_path: String,
        note_store: Arc<dyn NoteStore>,
    ) -> SearchTools {
        let knowledge_repo = Arc::new(MockKnowledgeRepository);
        let embedding_provider = Arc::new(MockEmbeddingProvider);
        SearchTools::with_note_store(kiln_path, knowledge_repo, embedding_provider, note_store)
    }

    #[tokio::test]
    async fn test_property_search_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with notes having properties
        let mock_store = Arc::new(MockNoteStore::new());

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), serde_json::json!("draft"));

        mock_store.add_note(note(
            "draft-note.md".to_string(),
            "Draft Note".to_string(),
            vec!["work".to_string()],
            props1,
        ));

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("published"));

        mock_store.add_note(note(
            "published-note.md".to_string(),
            "Published Note".to_string(),
            vec!["blog".to_string()],
            props2,
        ));

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for draft notes
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok(), "property_search should succeed");

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);

        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);

        // Verify source is from index
        assert_eq!(matches[0]["source"], "index");
        assert!(matches[0]["path"].as_str().unwrap().contains("draft"));
    }

    #[tokio::test]
    async fn test_property_search_by_tags_uses_note_store() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create a mock NoteStore with tagged notes
        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(note(
            "rust-note.md".to_string(),
            "Rust Note".to_string(),
            vec!["rust".to_string(), "programming".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "python-note.md".to_string(),
            "Python Note".to_string(),
            vec!["python".to_string(), "programming".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "cooking-note.md".to_string(),
            "Cooking Note".to_string(),
            vec!["cooking".to_string(), "recipes".to_string()],
            HashMap::new(),
        ));

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for notes with "rust" or "python" tags (OR logic)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"tags": ["rust", "python"]}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok(), "property_search should succeed");

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);

        // Should match both rust and python notes (OR logic)
        assert_eq!(parsed["count"], 2);
    }

    #[tokio::test]
    async fn test_property_search_single_tag() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        mock_store.add_note(note(
            "tagged.md".to_string(),
            "Tagged Note".to_string(),
            vec!["important".to_string()],
            HashMap::new(),
        ));

        mock_store.add_note(note(
            "untagged.md".to_string(),
            "Untagged Note".to_string(),
            vec![],
            HashMap::new(),
        ));

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for notes with "important" tag (single string)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"tags": "important"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn test_property_search_multiple_properties_and_logic() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), serde_json::json!("draft"));
        props1.insert("priority".to_string(), serde_json::json!("high"));

        mock_store.add_note(note(
            "high-priority-draft.md".to_string(),
            "High Priority Draft".to_string(),
            vec![],
            props1,
        ));

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), serde_json::json!("draft"));
        props2.insert("priority".to_string(), serde_json::json!("low"));

        mock_store.add_note(note(
            "low-priority-draft.md".to_string(),
            "Low Priority Draft".to_string(),
            vec![],
            props2,
        ));

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search for draft AND high priority (AND logic between properties)
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft", "priority": "high"}),
                limit: 10,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);

        // Should only match the high priority draft
        assert_eq!(parsed["count"], 1);
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches[0]["path"]
            .as_str()
            .unwrap()
            .contains("high-priority"));
    }

    #[tokio::test]
    async fn test_property_search_respects_limit() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let mock_store = Arc::new(MockNoteStore::new());

        // Add multiple notes with same status
        for i in 0..5 {
            let mut props = HashMap::new();
            props.insert("status".to_string(), serde_json::json!("draft"));

            mock_store.add_note(note(
                format!("note{i}.md"),
                format!("Note {i}"),
                vec![],
                props,
            ));
        }

        let search_tools = create_search_tools_with_store(kiln_path, mock_store);

        // Search with limit of 3
        let result = search_tools
            .property_search(Parameters(PropertySearchParams {
                properties: serde_json::json!({"status": "draft"}),
                limit: 3,
            }))
            .await;

        assert!(result.is_ok());

        let call_result = result.unwrap();
        let parsed = parse_tool_json(&call_result);

        // Should respect the limit
        assert_eq!(parsed["count"], 3);
    }
}
