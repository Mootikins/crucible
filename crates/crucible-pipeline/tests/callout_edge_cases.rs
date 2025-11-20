//! Comprehensive edge case tests for callout parsing
//!
//! This test suite verifies that callout parsing handles edge cases properly
//! across both Pulldown and markdown-it parsers.

use anyhow::Result;
use crucible_pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use std::sync::Arc;
use std::pin::Pin;
use crucible_core::processing::{ChangeDetectionStore, FileState};
use crucible_core::ChangeDetectionResult;
use crucible_core::EnrichedNoteStore;
use crucible_core::enrichment::{EnrichmentService, EnrichedNote};
use crucible_core::parser::PulldownParser;
use crucible_core::parser::MarkdownParser;
use crucible_merkle::{MerkleStore, HybridMerkleTree};
use async_trait::async_trait;
use futures::Future;

// Mock implementations (copied from parser_selection_tests.rs)
#[derive(Clone)]
struct MockChangeDetectionStore;

#[async_trait]
impl ChangeDetectionStore for MockChangeDetectionStore {
    async fn get_file_state(&self, _path: &Path) -> ChangeDetectionResult<Option<FileState>> {
        Ok(None)
    }

    async fn store_file_state(&self, _path: &Path, _file_state: FileState) -> ChangeDetectionResult<()> {
        Ok(())
    }

    async fn delete_file_state(&self, _path: &Path) -> ChangeDetectionResult<()> {
        Ok(())
    }

    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>> {
        Ok(vec![])
    }
}

#[derive(Clone)]
struct MockMerkleStore;

#[async_trait]
impl MerkleStore for MockMerkleStore {
    async fn store(&self, _id: &str, _tree: &crucible_merkle::HybridMerkleTree) -> Result<(), crucible_merkle::StorageError> {
        Ok(())
    }

    async fn retrieve(&self, _id: &str) -> Result<crucible_merkle::HybridMerkleTree, crucible_merkle::StorageError> {
        Err(crucible_merkle::StorageError::NotFound("test".to_string()))
    }

    async fn delete(&self, _id: &str) -> Result<(), crucible_merkle::StorageError> {
        Ok(())
    }

    async fn get_metadata(&self, _id: &str) -> Result<Option<crucible_merkle::TreeMetadata>, crucible_merkle::StorageError> {
        Ok(None)
    }

    async fn update_incremental(&self, _id: &str, _tree: &crucible_merkle::HybridMerkleTree, _changed_sections: &[usize]) -> Result<(), crucible_merkle::StorageError> {
        Ok(())
    }

    async fn list_trees(&self) -> Result<Vec<crucible_merkle::TreeMetadata>, crucible_merkle::StorageError> {
        Ok(vec![])
    }
}

#[derive(Clone)]
struct MockEnrichmentService;

#[async_trait]
impl EnrichmentService for MockEnrichmentService {
    fn min_words_for_embedding(&self) -> usize {
        10
    }

    fn max_batch_size(&self) -> usize {
        100
    }

    fn has_embedding_provider(&self) -> bool {
        false
    }

    async fn enrich(&'life0 self, _note: crucible_core::parser::ParsedNote, _stop_words: Vec<String>) -> Result<EnrichedNote> {
        // Return a basic enriched note for testing
        Ok(EnrichedNote {
            id: "".to_string(),
            title: "Test".to_string(),
            content: "".to_string(),
            file_path: PathBuf::from("test.md"),
            embedding: None,
            metadata: std::collections::HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }

    async fn enrich_with_tree(&'life0 self, _note: crucible_core::parser::ParsedNote, _tree: HybridMerkleTree, _stop_words: Vec<String>) -> Result<EnrichedNote> {
        self.enrich(_note, _stop_words).await
    }

    async fn infer_relations(&'life0 self, _note: &'life0 EnrichedNote, _similarity_threshold: f64) -> Result<Vec<crucible_core::enrichment::InferredRelation>> {
        Ok(vec![])
    }
}

#[derive(Clone)]
struct MockEnrichedNoteStore {
    notes: Arc<std::sync::Mutex<Vec<EnrichedNote>>>,
}

impl MockEnrichedNoteStore {
    fn new() -> Self {
        Self {
            notes: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn get_notes(&self) -> Vec<EnrichedNote> {
        self.notes.lock().unwrap().clone()
    }
}

#[async_trait]
impl EnrichedNoteStore for MockEnrichedNoteStore {
    async fn store_enriched(&self, enriched: &EnrichedNote, _relative_path: &str) -> Result<()> {
        self.notes.lock().unwrap().push(enriched.clone());
        Ok(())
    }
}

fn create_test_file(content: &str) -> Result<(TempDir, PathBuf)> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test_note.md");
    std::fs::write(&file_path, content)?;
    Ok((temp_dir, file_path))
}

fn create_pipeline_with_parser(backend: ParserBackend) -> NotePipeline {
    let change_detector = Arc::new(MockChangeDetectionStore);
    let merkle_store = Arc::new(MockMerkleStore);
    let enrichment_service = Arc::new(MockEnrichmentService);
    let storage = Arc::new(MockEnrichedNoteStore::new());

    let config = NotePipelineConfig {
        parser: backend,
        skip_enrichment: true,
        force_reprocess: false,
    };

    NotePipeline::with_config(
        change_detector,
        merkle_store,
        enrichment_service,
        storage,
        config,
    )
}

fn extract_callouts(content: &str, backend: ParserBackend) -> Result<Vec<crucible_core::parser::Callout>> {
    let pipeline = create_pipeline_with_parser(backend);
    let (_temp_dir, file_path) = create_test_file(content)?;

    // Use direct parser access to get the callouts
    match backend {
        ParserBackend::Pulldown => {
            let parser = PulldownParser::new();
            let parsed = parser.parse_content(content, &file_path)?;
            Ok(parsed.callouts)
        }
        #[cfg(feature = "markdown-it-parser")]
        ParserBackend::MarkdownIt => {
            let parser = MarkdownParser::new();
            let parsed = parser.parse_content(content, &file_path)?;
            Ok(parsed.callouts)
        }
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_nested_callouts() {
    let content = r#"> [!note] Outer callout
> This is the outer content
> > [!warning] Nested callout
> > This should be a separate callout, not nested
>
> More outer content"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    // Should extract 2 separate callouts (not nested)
    assert_eq!(callouts.len(), 2, "Should extract 2 separate callouts");
    assert_eq!(callouts[0].callout_type, "note");
    assert_eq!(callouts[1].callout_type, "warning");
}

#[tokio::test]
async fn test_special_characters_in_titles() {
    let content = r#"> [!note] Title with émojis 🎉 and spëcial chars!
> Content here

> [!warning] Title with "quotes" & <brackets> & [square]
> More content"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 2);
    assert_eq!(callouts[0].title.as_ref().unwrap(), "Title with émojis 🎉 and spëcial chars!");
    assert_eq!(callouts[1].title.as_ref().unwrap(), "Title with \"quotes\" & <brackets> & [square]");
}

#[tokio::test]
async fn test_complex_content_in_callouts() {
    let content = r#"> [!note] Callout with complex content
>
> ## Subheading inside
>
> - List item 1
> - List item 2
>   - Nested item
>
> ```
> code block inside
> ```
>
> [Inline link](http://example.com)
>
> Table:
> | Col1 | Col2 |
> |------|------|
> | A    | B    |"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 1);
    assert!(callouts[0].content.contains("## Subheading inside"));
    assert!(callouts[0].content.contains("List item 1"));
    assert!(callouts[0].content.contains("code block inside"));
    assert!(callouts[0].content.contains("[Inline link]"));
    assert!(callouts[0].content.contains("| Col1 | Col2 |"));
}

#[tokio::test]
async fn test_malformed_callout_syntax() {
    let content = r#"> [!note] Proper callout
> This is valid

> [! without closing bracket
> This should be ignored

> [!missing-exclamation]
> Also invalid

> [note] Missing exclamation
> Invalid syntax

> [] Empty bracket

> [!] Exclamation but no type
> Also invalid"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    // Should only extract the valid callout
    assert_eq!(callouts.len(), 1, "Should only extract valid callout");
    assert_eq!(callouts[0].callout_type, "note");
}

#[tokio::test]
async fn test_unicode_and_international_content() {
    let content = r#"> [!note] Título en español
> Contenido con caracteres especiales: ñáéíóú

> [!tip] 中文标题
> 这是中文内容，包含各种字符

> [!warning] عربي
> محتوى باللغة العربية

> [!info] Русский
> Содержимое на русском языке"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 4);
    assert!(callouts[0].title.as_ref().unwrap().contains("español"));
    assert!(callouts[1].title.as_ref().unwrap().contains("中文"));
    assert!(callouts[2].title.as_ref().unwrap().contains("عربي"));
    assert!(callouts[3].title.as_ref().unwrap().contains("Русский"));
}

#[tokio::test]
async fn test_callouts_at_document_boundaries() {
    let content = r#"> [!note] First callout
> At the very beginning

> [!tip] Last callout
> At the very end"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 2);
    assert_eq!(callouts[0].callout_type, "note");
    assert_eq!(callouts[1].callout_type, "tip");
}

#[tokio::test]
async fn test_callouts_with_no_content() {
    let content = r#"> [!note] Empty callout
>
> [!warning]

> [!tip] Just one line"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 3);
    assert_eq!(callouts[0].content.trim(), "");
    assert_eq!(callouts[1].content.trim(), "");
    assert_eq!(callouts[2].content, "Just one line");
}

#[tokio::test]
async fn test_mixed_callout_types() {
    let content = r#"> [!note] Standard note
> This is a standard callout

> [!custom-type] Custom callout
> This uses a non-standard type

> [!danger] Another standard
> Standard danger type

> [!my-special-callout] Another custom
> Custom with hyphens"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 4);
    assert_eq!(callouts[0].callout_type, "note");
    assert_eq!(callouts[1].callout_type, "custom-type");
    assert_eq!(callouts[2].callout_type, "danger");
    assert_eq!(callouts[3].callout_type, "my-special-callout");
}

#[tokio::test]
async fn test_very_long_callout_content() {
    let long_content = "A".repeat(10000); // 10KB of content
    let content = format!(
        r#"> [!note] Very long callout
> {}
> End of long content"#,
        long_content
    );

    let callouts = extract_callouts(&content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 1);
    assert!(callouts[0].content.len() > 10000);
}

#[tokio::test]
async fn test_callouts_with_empty_lines() {
    let content = r#"> [!note] Callout with empty lines
> First line
>
>
> Second line
>
> Third line"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 1);
    assert!(callouts[0].content.contains("First line"));
    assert!(callouts[0].content.contains("Second line"));
    assert!(callouts[0].content.contains("Third line"));
}

#[tokio::test]
async fn test_callouts_with_markup_in_content() {
    let content = r#"> [!info] Markup test
>
> **Bold text** and *italic text*
>
> `inline code` and ```code block```
>
> [link text](http://example.com)
>
> > Nested blockquote"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    assert_eq!(callouts.len(), 1);
    assert!(callouts[0].content.contains("**Bold text**"));
    assert!(callouts[0].content.contains("`inline code`"));
    assert!(callouts[0].content.contains("[link text]"));
    assert!(callouts[0].content.contains("> Nested blockquote"));
}

#[tokio::test]
async fn test_consecutive_callouts_without_separation() {
    let content = r#"> [!note] First callout
> First content
> [!warning] Second callout
> Second content
> [!tip] Third callout
> Third content"#;

    let callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();

    // This should parse as 3 separate callouts, not one
    assert_eq!(callouts.len(), 3, "Should parse 3 separate callouts");
    assert_eq!(callouts[0].callout_type, "note");
    assert_eq!(callouts[1].callout_type, "warning");
    assert_eq!(callouts[2].callout_type, "tip");
}

#[tokio::test]
async fn test_performance_with_many_callouts() {
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!(r#"> [!note] Callout {}
> This is callout number {} with some content

"#, i, i));
    }

    let start = std::time::Instant::now();
    let callouts = extract_callouts(&content, ParserBackend::Pulldown).unwrap();
    let duration = start.elapsed();

    assert_eq!(callouts.len(), 100, "Should extract all 100 callouts");
    assert!(duration.as_millis() < 1000, "Should complete within 1 second, took {:?}", duration);
}

// ============================================================================
// Parser Comparison Tests
// ============================================================================

#[cfg(feature = "markdown-it-parser")]
#[tokio::test]
async fn test_parser_parity_basic_callouts() {
    let content = r#"> [!note] Simple note
> Content here

> [!warning] Warning with title
> Warning content"#;

    let pulldown_callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();
    let mdit_callouts = extract_callouts(content, ParserBackend::MarkdownIt).unwrap();

    assert_eq!(pulldown_callouts.len(), mdit_callouts.len(), "Both parsers should extract same number of callouts");

    for (pd, mdit) in pulldown_callouts.iter().zip(mdit_callouts.iter()) {
        assert_eq!(pd.callout_type, mdit.callout_type, "Callout types should match");
        assert_eq!(pd.title, mdit.title, "Callout titles should match");
    }
}

#[cfg(feature = "markdown-it-parser")]
#[tokio::test]
async fn test_parser_parity_complex_content() {
    let content = r#"> [!note] Complex callout
> With **bold** and *italic* text
>
> - Lists
> - And other content
>
> ```rust
> let x = 42;
> ```"#;

    let pulldown_callouts = extract_callouts(content, ParserBackend::Pulldown).unwrap();
    let mdit_callouts = extract_callouts(content, ParserBackend::MarkdownIt).unwrap();

    assert_eq!(pulldown_callouts.len(), mdit_callouts.len());

    if let (Some(pd), Some(mdit)) = (pulldown_callouts.first(), mdit_callouts.first()) {
        assert_eq!(pd.callout_type, mdit.callout_type);
        // Content might differ in formatting but should be semantically equivalent
        assert!(pd.content.contains("bold"));
        assert!(mdit.content.contains("bold"));
    }
}