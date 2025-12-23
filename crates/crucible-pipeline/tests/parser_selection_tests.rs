//! Tests for parser selection and feature parity between parsers
//!
//! This test suite verifies:
//! 1. Parser backend selection via configuration
//! 2. Feature parity between CrucibleParser (default) and MarkdownItParser
//! 3. Edge cases and error handling for both parsers

use anyhow::Result;
use async_trait::async_trait;
use crucible_core::enrichment::{EnrichedNote, EnrichmentService};
use crucible_core::processing::{ChangeDetectionResult, ChangeDetectionStore, FileState};
use crucible_core::test_support::mocks::MockEnrichmentService;
use crucible_core::EnrichedNoteStore;
use crucible_merkle::{HybridMerkleTree, MerkleStore, StorageError, TreeMetadata};
use crucible_pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod common;
use common::create_test_file;

// ============================================================================
// Mock Implementations (copied from pipeline_integration_tests.rs)
// ============================================================================

#[derive(Clone)]
struct MockChangeDetectionStore {
    state: Arc<Mutex<HashMap<String, FileState>>>,
}

impl MockChangeDetectionStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl ChangeDetectionStore for MockChangeDetectionStore {
    async fn get_file_state(&self, path: &Path) -> ChangeDetectionResult<Option<FileState>> {
        let state = self.state.lock().unwrap();
        Ok(state.get(&path.to_string_lossy().to_string()).cloned())
    }

    async fn store_file_state(
        &self,
        path: &Path,
        file_state: FileState,
    ) -> ChangeDetectionResult<()> {
        let mut state = self.state.lock().unwrap();
        state.insert(path.to_string_lossy().to_string(), file_state);
        Ok(())
    }

    async fn delete_file_state(&self, path: &Path) -> ChangeDetectionResult<()> {
        let mut state = self.state.lock().unwrap();
        state.remove(&path.to_string_lossy().to_string());
        Ok(())
    }

    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>> {
        let state = self.state.lock().unwrap();
        Ok(state.keys().map(PathBuf::from).collect())
    }
}

#[derive(Clone)]
struct MockMerkleStore {
    state: Arc<Mutex<HashMap<String, HybridMerkleTree>>>,
}

impl MockMerkleStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl MerkleStore for MockMerkleStore {
    async fn store(&self, id: &str, tree: &HybridMerkleTree) -> Result<(), StorageError> {
        let mut state = self.state.lock().unwrap();
        state.insert(id.to_string(), tree.clone());
        Ok(())
    }

    async fn retrieve(&self, id: &str) -> Result<HybridMerkleTree, StorageError> {
        let state = self.state.lock().unwrap();
        state
            .get(id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut state = self.state.lock().unwrap();
        state.remove(id);
        Ok(())
    }

    async fn get_metadata(&self, _id: &str) -> Result<Option<TreeMetadata>, StorageError> {
        Ok(None)
    }

    async fn update_incremental(
        &self,
        id: &str,
        tree: &HybridMerkleTree,
        _changed_sections: &[usize],
    ) -> Result<(), StorageError> {
        self.store(id, tree).await
    }

    async fn list_trees(&self) -> Result<Vec<TreeMetadata>, StorageError> {
        Ok(vec![])
    }
}

#[derive(Clone)]
struct MockEnrichedNoteStore {
    state: Arc<Mutex<Vec<EnrichedNote>>>,
}

#[allow(dead_code)]
impl MockEnrichedNoteStore {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_stored_notes(&self) -> Vec<EnrichedNote> {
        self.state.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.state.lock().unwrap().clear();
    }
}

#[async_trait]
impl EnrichedNoteStore for MockEnrichedNoteStore {
    async fn store_enriched(&self, enriched: &EnrichedNote, _relative_path: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        state.push(enriched.clone());
        Ok(())
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

fn create_pipeline_with_parser(
    backend: ParserBackend,
) -> (NotePipeline, Arc<MockEnrichedNoteStore>) {
    let change_detector = Arc::new(MockChangeDetectionStore::new());
    let merkle_store = Arc::new(MockMerkleStore::new());
    let enrichment_service = Arc::new(MockEnrichmentService::new());
    let storage = Arc::new(MockEnrichedNoteStore::new());

    let config = NotePipelineConfig {
        parser: backend,
        skip_enrichment: false,
        force_reprocess: false,
    };

    let pipeline = NotePipeline::with_config(
        change_detector as Arc<dyn ChangeDetectionStore>,
        merkle_store as Arc<dyn MerkleStore>,
        enrichment_service as Arc<dyn EnrichmentService>,
        storage.clone() as Arc<dyn EnrichedNoteStore>,
        config,
    );

    (pipeline, storage)
}

// ============================================================================
// Parser Selection Tests
// ============================================================================

#[tokio::test]
async fn test_default_parser_is_default() {
    let config = NotePipelineConfig::default();
    assert_eq!(
        config.parser,
        ParserBackend::Default,
        "Default parser should be Default (CrucibleParser)"
    );
}

#[tokio::test]
async fn test_default_parser_processes_successfully() {
    let (pipeline, storage) = create_pipeline_with_parser(ParserBackend::Default);

    let content = r#"# Test Note

This is a test with [[wikilink]] and #tag.

> [!note] A callout
> With content

Inline math: $x^2 + y^2 = z^2$
"#;

    let (_temp_dir, file_path) = create_test_file(content).unwrap();
    let result = pipeline.process(&file_path).await;

    assert!(result.is_ok(), "Default parser should process successfully");

    let notes = storage.get_stored_notes();
    assert_eq!(notes.len(), 1, "Should have stored one note");
}

#[cfg(feature = "markdown-it-parser")]
#[tokio::test]
async fn test_markdown_it_parser_processes_successfully() {
    let (pipeline, storage) = create_pipeline_with_parser(ParserBackend::MarkdownIt);

    let content = r#"# Test Note

This is a test with [[wikilink]] and #tag.

> [!note] A callout
> With content

Inline math: $x^2 + y^2 = z^2$
"#;

    let (_temp_dir, file_path) = create_test_file(content).unwrap();
    let result = pipeline.process(&file_path).await;

    assert!(
        result.is_ok(),
        "Markdown-it parser should process successfully"
    );

    let notes = storage.get_stored_notes();
    assert_eq!(notes.len(), 1, "Should have stored one note");
}

// ============================================================================
// Feature Parity Tests
// ============================================================================

#[tokio::test]
async fn test_parity_wikilinks_extraction() {
    let test_content = r#"# Test

Link to [[Page 1]] and [[Page 2|Alias]].
Embed: ![[Image]].
Block ref: [[Note#^block]].
Heading ref: [[Note#Heading]].
"#;

    // Test with Default parser
    let (pipeline_pulldown, storage_pulldown) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp1, path1) = create_test_file(test_content).unwrap();
    pipeline_pulldown.process(&path1).await.unwrap();
    let notes_pulldown = storage_pulldown.get_stored_notes();

    // Extract wikilinks from Default parser result
    let wikilinks_pulldown = &notes_pulldown[0].parsed.content.wikilinks;

    // Verify Default parser extracted wikilinks correctly
    assert_eq!(wikilinks_pulldown.len(), 5, "Should extract 5 wikilinks");
    assert_eq!(wikilinks_pulldown[0].target, "Page 1");
    assert_eq!(wikilinks_pulldown[1].target, "Page 2");
    assert_eq!(wikilinks_pulldown[1].alias, Some("Alias".to_string()));
    assert!(wikilinks_pulldown[2].is_embed, "Third link should be embed");

    #[cfg(feature = "markdown-it-parser")]
    {
        // Test with Markdown-it parser
        let (pipeline_mdit, storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        pipeline_mdit.process(&path2).await.unwrap();
        let notes_mdit = storage_mdit.get_stored_notes();

        let wikilinks_mdit = &notes_mdit[0].parsed.content.wikilinks;

        // Compare results
        assert_eq!(
            wikilinks_pulldown.len(),
            wikilinks_mdit.len(),
            "Both parsers should extract same number of wikilinks"
        );

        for (i, (pulldown, mdit)) in wikilinks_pulldown
            .iter()
            .zip(wikilinks_mdit.iter())
            .enumerate()
        {
            assert_eq!(
                pulldown.target, mdit.target,
                "Wikilink {} target mismatch",
                i
            );
            assert_eq!(pulldown.alias, mdit.alias, "Wikilink {} alias mismatch", i);
            assert_eq!(
                pulldown.is_embed, mdit.is_embed,
                "Wikilink {} embed flag mismatch",
                i
            );
        }
    }
}

#[tokio::test]
async fn test_parity_tags_extraction() {
    let test_content = r#"# Test

Tags: #project #important #status/active

Nested tag: #work/review/urgent
"#;

    // Test with Default parser
    let (pipeline_pulldown, storage_pulldown) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp1, path1) = create_test_file(test_content).unwrap();
    pipeline_pulldown.process(&path1).await.unwrap();
    let notes_pulldown = storage_pulldown.get_stored_notes();

    let tags_pulldown = &notes_pulldown[0].parsed.content.tags;

    // Verify Default parser extracted tags correctly
    assert_eq!(tags_pulldown.len(), 4, "Should extract 4 tags");
    assert_eq!(tags_pulldown[0].name, "project");
    assert_eq!(tags_pulldown[1].name, "important");
    assert_eq!(tags_pulldown[2].name, "status/active");
    assert_eq!(tags_pulldown[3].name, "work/review/urgent");

    #[cfg(feature = "markdown-it-parser")]
    {
        // Test with Markdown-it parser
        let (pipeline_mdit, storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        pipeline_mdit.process(&path2).await.unwrap();
        let notes_mdit = storage_mdit.get_stored_notes();

        let tags_mdit = &notes_mdit[0].parsed.content.tags;

        // Compare results
        assert_eq!(
            tags_pulldown.len(),
            tags_mdit.len(),
            "Both parsers should extract same number of tags"
        );

        for (i, (pulldown, mdit)) in tags_pulldown.iter().zip(tags_mdit.iter()).enumerate() {
            assert_eq!(pulldown.name, mdit.name, "Tag {} name mismatch", i);
        }
    }
}

#[tokio::test]
async fn test_parity_callouts_extraction() {
    let test_content = r#"# Test

> [!note]
> Simple note callout
> With multiple lines

Text between callouts.

> [!warning] Important Warning
> This is a warning with title

Another text section.

> [!tip]
> Tip without title
"#;

    // Test with Default parser
    let (pipeline_pulldown, storage_pulldown) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp1, path1) = create_test_file(test_content).unwrap();
    pipeline_pulldown.process(&path1).await.unwrap();
    let notes_pulldown = storage_pulldown.get_stored_notes();

    let callouts_pulldown = &notes_pulldown[0].parsed.content.callouts;

    // Verify Default parser extracted callouts correctly
    assert_eq!(callouts_pulldown.len(), 3, "Should extract 3 callouts");
    assert_eq!(callouts_pulldown[0].callout_type, "note".into());
    assert_eq!(callouts_pulldown[0].title, None);
    assert_eq!(callouts_pulldown[1].callout_type, "warning".into());
    assert_eq!(
        callouts_pulldown[1].title,
        Some("Important Warning".to_string())
    );
    assert_eq!(callouts_pulldown[2].callout_type, "tip".into());

    #[cfg(feature = "markdown-it-parser")]
    {
        // Test with Markdown-it parser
        let (pipeline_mdit, storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        pipeline_mdit.process(&path2).await.unwrap();
        let notes_mdit = storage_mdit.get_stored_notes();

        let callouts_mdit = &notes_mdit[0].parsed.content.callouts;

        // Compare results
        assert_eq!(
            callouts_pulldown.len(),
            callouts_mdit.len(),
            "Both parsers should extract same number of callouts"
        );

        for (i, (pulldown, mdit)) in callouts_pulldown
            .iter()
            .zip(callouts_mdit.iter())
            .enumerate()
        {
            assert_eq!(
                pulldown.callout_type, mdit.callout_type,
                "Callout {} type mismatch",
                i
            );
            assert_eq!(pulldown.title, mdit.title, "Callout {} title mismatch", i);
        }
    }
}

#[tokio::test]
async fn test_parity_latex_extraction() {
    let test_content = r#"# Test

Inline math: $x^2 + y^2 = z^2$

Block math:
$$
\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

More inline: $\alpha + \beta = \gamma$
"#;

    // Test with Default parser
    let (pipeline_pulldown, storage_pulldown) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp1, path1) = create_test_file(test_content).unwrap();
    pipeline_pulldown.process(&path1).await.unwrap();
    let notes_pulldown = storage_pulldown.get_stored_notes();

    let latex_pulldown = &notes_pulldown[0].parsed.content.latex_expressions;

    // Debug output
    eprintln!("=== EXTRACTED LATEX EXPRESSIONS ===");
    for (i, expr) in latex_pulldown.iter().enumerate() {
        eprintln!(
            "[{}] is_block={}, offset={}, length={}, content={:?}",
            i, expr.is_block, expr.offset, expr.length, expr.expression
        );
    }
    eprintln!("===================================");

    // Verify Default parser extracted LaTeX correctly
    assert_eq!(
        latex_pulldown.len(),
        3,
        "Should extract 3 LaTeX expressions"
    );
    assert!(!latex_pulldown[0].is_block, "First should be inline");
    assert!(latex_pulldown[1].is_block, "Second should be block");
    assert!(!latex_pulldown[2].is_block, "Third should be inline");

    #[cfg(feature = "markdown-it-parser")]
    {
        // Test with Markdown-it parser
        let (pipeline_mdit, storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        pipeline_mdit.process(&path2).await.unwrap();
        let notes_mdit = storage_mdit.get_stored_notes();

        let latex_mdit = &notes_mdit[0].parsed.content.latex_expressions;

        // Compare results
        assert_eq!(
            latex_pulldown.len(),
            latex_mdit.len(),
            "Both parsers should extract same number of LaTeX expressions"
        );

        for (i, (pulldown, mdit)) in latex_pulldown.iter().zip(latex_mdit.iter()).enumerate() {
            assert_eq!(
                pulldown.is_block, mdit.is_block,
                "LaTeX {} block flag mismatch",
                i
            );
        }
    }
}

#[tokio::test]
async fn test_parity_complex_document() {
    let test_content = r#"---
title: Complex Test Document
tags: [meta, test]
---

# Main Title

This document tests all syntax types together.

## Wikilinks Section

Link to [[Document A]] and [[Document B|Alias B]].
Embed: ![[Image.png]].

## Tags

Project tags: #project #important
Status: #status/active #status/review

## Callouts

> [!note] Important Note
> This is a note with multiple lines
> And more content here

> [!warning]
> Simple warning

## Math

Inline: $E = mc^2$

Block:
$$
\nabla \times \mathbf{E} = -\frac{\partial \mathbf{B}}{\partial t}
$$

## Code

```rust
fn main() {
    // This should NOT be parsed for syntax
    // No [[wikilinks]] or #tags here
    println!("Hello");
}
```

Regular text with [[actual link]] and #actual-tag.
"#;

    // Test with Default parser
    let (pipeline_pulldown, storage_pulldown) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp1, path1) = create_test_file(test_content).unwrap();
    pipeline_pulldown.process(&path1).await.unwrap();
    let notes_pulldown = storage_pulldown.get_stored_notes();

    let content_pulldown = &notes_pulldown[0].parsed.content;

    // Verify all syntax types extracted
    assert!(
        content_pulldown.wikilinks.len() >= 3,
        "Should extract wikilinks"
    );
    assert!(content_pulldown.tags.len() >= 4, "Should extract tags");
    assert!(
        content_pulldown.callouts.len() >= 2,
        "Should extract callouts"
    );
    assert!(
        content_pulldown.latex_expressions.len() >= 2,
        "Should extract LaTeX"
    );

    #[cfg(feature = "markdown-it-parser")]
    {
        // Test with Markdown-it parser
        let (pipeline_mdit, storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        pipeline_mdit.process(&path2).await.unwrap();
        let notes_mdit = storage_mdit.get_stored_notes();

        let content_mdit = &notes_mdit[0].parsed.content;

        // Both should extract similar amounts (exact match depends on implementation details)
        assert!(
            content_pulldown.wikilinks.len() == content_mdit.wikilinks.len(),
            "Wikilink count should match: pulldown={}, mdit={}",
            content_pulldown.wikilinks.len(),
            content_mdit.wikilinks.len()
        );
        assert!(
            content_pulldown.tags.len() == content_mdit.tags.len(),
            "Tag count should match: pulldown={}, mdit={}",
            content_pulldown.tags.len(),
            content_mdit.tags.len()
        );
        assert!(
            content_pulldown.callouts.len() == content_mdit.callouts.len(),
            "Callout count should match: pulldown={}, mdit={}",
            content_pulldown.callouts.len(),
            content_mdit.callouts.len()
        );
        assert!(
            content_pulldown.latex_expressions.len() == content_mdit.latex_expressions.len(),
            "LaTeX count should match: pulldown={}, mdit={}",
            content_pulldown.latex_expressions.len(),
            content_mdit.latex_expressions.len()
        );
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_wikilinks_not_in_code_blocks() {
    let test_content = r#"# Test

Normal [[wikilink]] here.

```
Code block with [[fake link]]
```

`Inline code with [[another fake]]`

Another normal [[real link]].
"#;

    let (pipeline, storage) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp, path) = create_test_file(test_content).unwrap();
    pipeline.process(&path).await.unwrap();
    let notes = storage.get_stored_notes();

    let wikilinks = &notes[0].parsed.content.wikilinks;

    // Should only extract wikilinks outside code blocks
    assert_eq!(
        wikilinks.len(),
        2,
        "Should only extract 2 wikilinks (not from code blocks)"
    );
    assert_eq!(wikilinks[0].target, "wikilink");
    assert_eq!(wikilinks[1].target, "real link");
}

#[tokio::test]
async fn test_malformed_syntax_handling() {
    let test_content = r#"# Test

Malformed wikilink: [[incomplete
Malformed tag: #
Empty wikilink: [[]]
Malformed callout: > [!
Malformed math: $incomplete
"#;

    // Both parsers should handle malformed syntax gracefully
    let (pipeline, _storage) = create_pipeline_with_parser(ParserBackend::Default);
    let (_temp, path) = create_test_file(test_content).unwrap();
    let result = pipeline.process(&path).await;

    assert!(
        result.is_ok(),
        "Parser should handle malformed syntax gracefully"
    );

    #[cfg(feature = "markdown-it-parser")]
    {
        let (pipeline_mdit, _storage_mdit) = create_pipeline_with_parser(ParserBackend::MarkdownIt);
        let (_temp2, path2) = create_test_file(test_content).unwrap();
        let result_mdit = pipeline_mdit.process(&path2).await;

        assert!(
            result_mdit.is_ok(),
            "Markdown-it should handle malformed syntax gracefully"
        );
    }
}
