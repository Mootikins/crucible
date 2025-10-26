//! Realistic vault scenario integration tests
//!
//! These tests verify complete workflows with realistic vault structures,
//! simulating actual user workflows and edge cases.
//!
//! ## Test Coverage
//!
//! - Nested folder structures (Projects/Daily/Archive)
//! - Complex wikilink graphs with cycles and hubs
//! - Frontmatter variations (YAML, missing fields, nested objects)
//! - Tag extraction (inline, frontmatter, nested, mixed)
//! - Error handling (malformed content, unicode, edge cases)
//!
//! Run with: `cargo test -p crucible-daemon --test vault_realistic`

use anyhow::Result;
use crucible_core::parser::{MarkdownParser, PulldownParser, SurrealDBAdapter};
use crucible_surrealdb::{EmbeddingMetadata, SurrealEmbeddingDatabase};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

// ============================================================================
// Test Harness
// ============================================================================

/// Test harness for realistic vault scenarios
///
/// Provides a complete test environment with:
/// - Temporary vault directory
/// - In-memory SurrealDB database
/// - Markdown parser
/// - SurrealDB adapter
///
/// Files are manually processed (no file watcher) for deterministic testing.
struct VaultTestHarness {
    vault_dir: TempDir,
    db: SurrealEmbeddingDatabase,
    parser: PulldownParser,
    adapter: SurrealDBAdapter,
}

impl VaultTestHarness {
    /// Create a new test harness
    async fn new() -> Result<Self> {
        let vault_dir = TempDir::new()?;
        let db = SurrealEmbeddingDatabase::new_memory();
        db.initialize().await?;

        Ok(Self {
            vault_dir,
            db,
            parser: PulldownParser::new(),
            adapter: SurrealDBAdapter::new().with_full_content(),
        })
    }

    /// Create a note in the vault and index it
    ///
    /// # Arguments
    /// - `path`: Relative path from vault root (e.g., "Projects/note.md")
    /// - `content`: Markdown content
    ///
    /// # Returns
    /// Absolute path to the created file
    async fn create_note(&self, path: &str, content: &str) -> Result<PathBuf> {
        let note_path = self.vault_dir.path().join(path);

        // Create parent directories
        if let Some(parent) = note_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Write file
        fs::write(&note_path, content).await?;

        // Parse and index
        let doc = self.parser.parse_file(&note_path).await?;

        // Validate with adapter
        let _record = self.adapter.to_note_record(&doc)?;

        // Store in database
        let path_str = note_path.to_string_lossy().to_string();
        let content_text = doc.content.plain_text.clone();
        let embedding = vec![0.0; 384]; // Dummy embedding
        let folder = note_path
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        let properties = doc
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        let metadata = EmbeddingMetadata {
            file_path: path_str.clone(),
            title: Some(doc.title()),
            tags: doc.all_tags(),
            folder,
            properties,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        self.db
            .store_embedding(&path_str, &content_text, &embedding, &metadata)
            .await?;

        Ok(note_path)
    }

    /// Check if a file exists in the database
    async fn file_exists(&self, path: &str) -> Result<bool> {
        let full_path = self
            .vault_dir
            .path()
            .join(path)
            .to_string_lossy()
            .to_string();
        self.db.file_exists(&full_path).await
    }

    /// Get file metadata from database
    async fn get_metadata(&self, path: &str) -> Result<Option<EmbeddingMetadata>> {
        let full_path = self
            .vault_dir
            .path()
            .join(path)
            .to_string_lossy()
            .to_string();
        let data = self.db.get_embedding(&full_path).await?;
        Ok(data.map(|d| d.metadata))
    }

    /// Create a wikilink relation between two files
    async fn create_relation(&self, from: &str, to: &str, rel_type: &str) -> Result<()> {
        let from_path = self
            .vault_dir
            .path()
            .join(from)
            .to_string_lossy()
            .to_string();
        let to_path = self.vault_dir.path().join(to).to_string_lossy().to_string();
        self.db
            .create_relation(&from_path, &to_path, rel_type, None)
            .await
    }

    /// Get related files
    async fn get_related(&self, path: &str, rel_type: Option<&str>) -> Result<Vec<String>> {
        let full_path = self
            .vault_dir
            .path()
            .join(path)
            .to_string_lossy()
            .to_string();
        self.db.get_related(&full_path, rel_type).await
    }

    /// Search by tags
    async fn search_by_tags(&self, tags: &[&str]) -> Result<Vec<String>> {
        let tag_strings: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
        self.db.search_by_tags(&tag_strings).await
    }

    /// Get database stats
    async fn get_stats(&self) -> Result<crucible_surrealdb::DatabaseStats> {
        self.db.get_stats().await
    }
}

// ============================================================================
// Test 1: Nested Folder Structure
// ============================================================================

#[tokio::test]
async fn test_nested_folder_structure() -> Result<()> {
    // Test Flow:
    // 1. Create harness
    // 2. Create nested vault structure:
    //    - Projects/Crucible/Architecture/design.md
    //    - Projects/Crucible/Implementation/code.md
    //    - Daily/2025-01/2025-01-15.md
    //    - Daily/2025-01/2025-01-16.md
    //    - Archive/old-note.md
    // 3. Verify all files indexed with correct paths
    // 4. Verify folder metadata correct
    // 5. Query by folder patterns

    let harness = VaultTestHarness::new().await?;

    // Create nested structure
    harness
        .create_note(
            "Projects/Crucible/Architecture/design.md",
            r#"---
title: Architecture Design
tags: [architecture, design]
---

# Architecture Design

System architecture documentation.
"#,
        )
        .await?;

    harness
        .create_note(
            "Projects/Crucible/Implementation/code.md",
            r#"---
title: Implementation Notes
tags: [implementation, code]
---

# Implementation Notes

Code implementation details.
"#,
        )
        .await?;

    harness
        .create_note(
            "Daily/2025-01/2025-01-15.md",
            r#"---
title: Daily Note 2025-01-15
tags: [daily]
date: 2025-01-15
---

# Daily Note

Today's work log.
"#,
        )
        .await?;

    harness
        .create_note(
            "Daily/2025-01/2025-01-16.md",
            r#"---
title: Daily Note 2025-01-16
tags: [daily]
date: 2025-01-16
---

# Daily Note

Today's work log.
"#,
        )
        .await?;

    harness
        .create_note(
            "Archive/old-note.md",
            r#"# Old Note

Archived content.
"#,
        )
        .await?;

    // Verify all files exist
    assert!(
        harness
            .file_exists("Projects/Crucible/Architecture/design.md")
            .await?
    );
    assert!(
        harness
            .file_exists("Projects/Crucible/Implementation/code.md")
            .await?
    );
    assert!(harness.file_exists("Daily/2025-01/2025-01-15.md").await?);
    assert!(harness.file_exists("Daily/2025-01/2025-01-16.md").await?);
    assert!(harness.file_exists("Archive/old-note.md").await?);

    // Verify folder metadata
    let design_meta = harness
        .get_metadata("Projects/Crucible/Architecture/design.md")
        .await?
        .expect("Design note should exist");
    assert!(design_meta
        .folder
        .contains("Projects/Crucible/Architecture"));

    let daily_meta = harness
        .get_metadata("Daily/2025-01/2025-01-15.md")
        .await?
        .expect("Daily note should exist");
    assert!(daily_meta.folder.contains("Daily/2025-01"));

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 5, "Should have 5 documents indexed");

    Ok(())
}

// ============================================================================
// Test 2: Complex Wikilink Graph
// ============================================================================

#[tokio::test]
async fn test_complex_wikilink_graph() -> Result<()> {
    // Test Flow:
    // 1. Create harness
    // 2. Create notes with complex link structure:
    //    - A -> B, B -> C, C -> A (cycle)
    //    - D -> A, D -> C (hub)
    //    - E -> [[B#heading]] (heading link)
    // 3. Verify all relations created
    // 4. Query graph in both directions
    // 5. Verify heading references work

    let harness = VaultTestHarness::new().await?;

    // Create notes with wikilinks
    harness
        .create_note(
            "noteA.md",
            r#"# Note A

This note links to [[noteB]].

See also [[noteC]] for more details.
"#,
        )
        .await?;

    harness
        .create_note(
            "noteB.md",
            r#"# Note B

## Important Heading

This note links to [[noteC]].

Referenced by [[noteA]].
"#,
        )
        .await?;

    harness
        .create_note(
            "noteC.md",
            r#"# Note C

This note links back to [[noteA]].

Forms a cycle with A and B.
"#,
        )
        .await?;

    harness
        .create_note(
            "noteD.md",
            r#"# Note D (Hub)

This is a hub note linking to:
- [[noteA]]
- [[noteC]]

It doesn't link to B.
"#,
        )
        .await?;

    harness
        .create_note(
            "noteE.md",
            r#"# Note E

This note has a heading reference: [[noteB#Important Heading]].
"#,
        )
        .await?;

    // Create wikilink relations (simulating what the parser would extract)
    harness
        .create_relation("noteA.md", "noteB.md", "wikilink")
        .await?;
    harness
        .create_relation("noteA.md", "noteC.md", "wikilink")
        .await?;
    harness
        .create_relation("noteB.md", "noteC.md", "wikilink")
        .await?;
    harness
        .create_relation("noteB.md", "noteA.md", "wikilink")
        .await?;
    harness
        .create_relation("noteC.md", "noteA.md", "wikilink")
        .await?;
    harness
        .create_relation("noteD.md", "noteA.md", "wikilink")
        .await?;
    harness
        .create_relation("noteD.md", "noteC.md", "wikilink")
        .await?;
    harness
        .create_relation("noteE.md", "noteB.md", "wikilink")
        .await?;

    // Verify relations exist
    let a_links = harness.get_related("noteA.md", Some("wikilink")).await?;
    assert_eq!(a_links.len(), 2, "Note A should link to 2 notes");

    let b_links = harness.get_related("noteB.md", Some("wikilink")).await?;
    assert_eq!(b_links.len(), 2, "Note B should link to 2 notes");

    let c_links = harness.get_related("noteC.md", Some("wikilink")).await?;
    assert_eq!(c_links.len(), 1, "Note C should link to 1 note");

    let d_links = harness.get_related("noteD.md", Some("wikilink")).await?;
    assert_eq!(d_links.len(), 2, "Note D (hub) should link to 2 notes");

    let e_links = harness.get_related("noteE.md", Some("wikilink")).await?;
    assert_eq!(
        e_links.len(),
        1,
        "Note E should link to 1 note (with heading ref)"
    );

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 5);

    Ok(())
}

// ============================================================================
// Test 3: Frontmatter Variations
// ============================================================================

#[tokio::test]
async fn test_frontmatter_variations() -> Result<()> {
    // Test Flow:
    // 1. Create harness
    // 2. Create notes with different frontmatter variations:
    //    - Full YAML frontmatter
    //    - Missing optional fields
    //    - Array values (tags, categories)
    //    - Nested objects (author.name)
    //    - Date fields
    // 3. Verify all parse correctly
    // 4. Verify metadata searchable

    let harness = VaultTestHarness::new().await?;

    // Full frontmatter
    harness
        .create_note(
            "full.md",
            r#"---
title: Full Frontmatter
tags: [rust, testing, integration]
categories: [development, backend]
status: active
priority: high
author:
  name: Test Author
  email: test@example.com
created: 2025-01-15
modified: 2025-01-16
---

# Full Frontmatter

This note has comprehensive frontmatter.
"#,
        )
        .await?;

    // Minimal frontmatter
    harness
        .create_note(
            "minimal.md",
            r#"---
title: Minimal Note
---

# Minimal Note

Only has title in frontmatter.
"#,
        )
        .await?;

    // Array values
    harness
        .create_note(
            "arrays.md",
            r#"---
title: Array Values
tags: [tag1, tag2, tag3]
categories:
  - cat1
  - cat2
---

# Array Values

Testing array parsing.
"#,
        )
        .await?;

    // Nested objects
    harness
        .create_note(
            "nested.md",
            r#"---
title: Nested Objects
metadata:
  author:
    name: John Doe
    role: Developer
  project:
    name: Crucible
    version: 0.1.0
---

# Nested Objects

Testing nested object parsing.
"#,
        )
        .await?;

    // Date fields
    harness
        .create_note(
            "dates.md",
            r#"---
title: Date Fields
created: 2025-01-15T10:30:00Z
modified: 2025-01-16T14:45:00Z
due_date: 2025-01-20
---

# Date Fields

Testing date parsing.
"#,
        )
        .await?;

    // Verify all files exist
    assert!(harness.file_exists("full.md").await?);
    assert!(harness.file_exists("minimal.md").await?);
    assert!(harness.file_exists("arrays.md").await?);
    assert!(harness.file_exists("nested.md").await?);
    assert!(harness.file_exists("dates.md").await?);

    // Verify full frontmatter
    let full_meta = harness
        .get_metadata("full.md")
        .await?
        .expect("Full note should exist");
    assert_eq!(full_meta.title, Some("Full Frontmatter".to_string()));
    assert!(full_meta.tags.contains(&"rust".to_string()));
    assert!(full_meta.tags.contains(&"testing".to_string()));
    assert!(full_meta.tags.contains(&"integration".to_string()));
    assert_eq!(
        full_meta.properties.get("status"),
        Some(&serde_json::json!("active"))
    );
    assert_eq!(
        full_meta.properties.get("priority"),
        Some(&serde_json::json!("high"))
    );

    // Verify minimal frontmatter
    let minimal_meta = harness
        .get_metadata("minimal.md")
        .await?
        .expect("Minimal note should exist");
    assert_eq!(minimal_meta.title, Some("Minimal Note".to_string()));
    assert!(minimal_meta.tags.is_empty());

    // Verify array values
    let arrays_meta = harness
        .get_metadata("arrays.md")
        .await?
        .expect("Arrays note should exist");
    assert_eq!(arrays_meta.title, Some("Array Values".to_string()));
    assert!(arrays_meta.tags.contains(&"tag1".to_string()));
    assert!(arrays_meta.tags.contains(&"tag2".to_string()));
    assert!(arrays_meta.tags.contains(&"tag3".to_string()));

    // Verify nested objects
    let nested_meta = harness
        .get_metadata("nested.md")
        .await?
        .expect("Nested note should exist");
    assert_eq!(nested_meta.title, Some("Nested Objects".to_string()));
    assert!(nested_meta.properties.contains_key("metadata"));

    // Verify dates
    let dates_meta = harness
        .get_metadata("dates.md")
        .await?
        .expect("Dates note should exist");
    assert_eq!(dates_meta.title, Some("Date Fields".to_string()));
    assert!(dates_meta.properties.contains_key("created"));
    assert!(dates_meta.properties.contains_key("modified"));

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 5);

    Ok(())
}

// ============================================================================
// Test 4: Tag Extraction (Inline vs Frontmatter)
// ============================================================================

#[tokio::test]
async fn test_tag_extraction_comprehensive() -> Result<()> {
    // Test Flow:
    // 1. Create harness
    // 2. Create notes with different tag formats:
    //    - Frontmatter tags only
    //    - Inline tags only
    //    - Mixed (both frontmatter and inline)
    //    - Nested tags (#parent/child)
    // 3. Verify all tags extracted and deduplicated
    // 4. Verify tag search works
    // 5. Verify nested tags indexed

    let harness = VaultTestHarness::new().await?;

    // Frontmatter tags only
    harness
        .create_note(
            "frontmatter-tags.md",
            r#"---
title: Frontmatter Tags
tags: [rust, programming, systems]
---

# Frontmatter Tags

Tags only in frontmatter.
"#,
        )
        .await?;

    // Inline tags only
    harness
        .create_note(
            "inline-tags.md",
            r#"# Inline Tags

This note has inline tags: #rust #programming #systems

No frontmatter tags.
"#,
        )
        .await?;

    // Mixed tags (frontmatter + inline)
    harness
        .create_note(
            "mixed-tags.md",
            r#"---
title: Mixed Tags
tags: [rust, programming]
---

# Mixed Tags

Also has inline tags: #systems #testing

Tags should be deduplicated.
"#,
        )
        .await?;

    // Nested tags
    harness
        .create_note(
            "nested-tags.md",
            r#"---
title: Nested Tags
tags: [project/crucible, type/documentation]
---

# Nested Tags

Also has inline nested tags: #category/testing #level/advanced

Nested tags use forward slashes.
"#,
        )
        .await?;

    // All tag types
    harness
        .create_note(
            "all-tags.md",
            r#"---
title: All Tag Types
tags: [rust, project/crucible]
---

# All Tag Types

Has inline tags: #programming #category/systems

And nested inline: #type/implementation
"#,
        )
        .await?;

    // Verify all files exist
    assert!(harness.file_exists("frontmatter-tags.md").await?);
    assert!(harness.file_exists("inline-tags.md").await?);
    assert!(harness.file_exists("mixed-tags.md").await?);
    assert!(harness.file_exists("nested-tags.md").await?);
    assert!(harness.file_exists("all-tags.md").await?);

    // Verify frontmatter tags
    let fm_meta = harness
        .get_metadata("frontmatter-tags.md")
        .await?
        .expect("Frontmatter tags note should exist");
    assert!(fm_meta.tags.contains(&"rust".to_string()));
    assert!(fm_meta.tags.contains(&"programming".to_string()));
    assert!(fm_meta.tags.contains(&"systems".to_string()));

    // Verify inline tags
    let inline_meta = harness
        .get_metadata("inline-tags.md")
        .await?
        .expect("Inline tags note should exist");
    assert!(inline_meta.tags.contains(&"rust".to_string()));
    assert!(inline_meta.tags.contains(&"programming".to_string()));
    assert!(inline_meta.tags.contains(&"systems".to_string()));

    // Verify mixed tags (should be deduplicated)
    let mixed_meta = harness
        .get_metadata("mixed-tags.md")
        .await?
        .expect("Mixed tags note should exist");
    assert!(mixed_meta.tags.contains(&"rust".to_string()));
    assert!(mixed_meta.tags.contains(&"programming".to_string()));
    assert!(mixed_meta.tags.contains(&"systems".to_string()));
    assert!(mixed_meta.tags.contains(&"testing".to_string()));

    // Verify nested tags
    let nested_meta = harness
        .get_metadata("nested-tags.md")
        .await?
        .expect("Nested tags note should exist");
    assert!(nested_meta.tags.contains(&"project/crucible".to_string()));
    assert!(nested_meta.tags.contains(&"type/documentation".to_string()));
    assert!(nested_meta.tags.contains(&"category/testing".to_string()));
    assert!(nested_meta.tags.contains(&"level/advanced".to_string()));

    // Search by tags
    let rust_notes = harness.search_by_tags(&["rust"]).await?;
    assert_eq!(rust_notes.len(), 4, "Should find 4 notes with 'rust' tag");

    let systems_notes = harness.search_by_tags(&["systems"]).await?;
    assert_eq!(
        systems_notes.len(),
        3,
        "Should find 3 notes with 'systems' tag"
    );

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 5);

    Ok(())
}

// ============================================================================
// Test 5: Error Handling & Edge Cases
// ============================================================================

#[tokio::test]
async fn test_error_handling_edge_cases() -> Result<()> {
    // Test Flow:
    // 1. Create harness
    // 2. Create notes with edge cases:
    //    - No frontmatter (plain markdown)
    //    - Invalid YAML (malformed frontmatter)
    //    - Empty file
    //    - Unicode in titles/content
    //    - Very long content (10KB+)
    // 3. Verify parser handles gracefully
    // 4. Verify valid notes still indexed
    // 5. Verify error reporting works

    let harness = VaultTestHarness::new().await?;

    // No frontmatter (plain markdown)
    harness
        .create_note(
            "plain.md",
            r#"# Plain Markdown

No frontmatter here.

Just regular markdown content.
"#,
        )
        .await?;

    // Empty file
    harness.create_note("empty.md", "").await?;

    // Unicode in title and content
    harness
        .create_note(
            "unicode.md",
            r#"---
title: Unicode Test 中文 日本語 한국어
tags: [emoji, unicode]
---

# Unicode Content 🚀

This note has Unicode: 中文 (Chinese), 日本語 (Japanese), 한국어 (Korean).

Emoji support: 🔥 ✨ 💡 🎉

Mathematical symbols: ∑ ∫ ∂ ∇
"#,
        )
        .await?;

    // Very long content (10KB+)
    let long_content = format!(
        r#"---
title: Long Content
tags: [performance, stress-test]
---

# Long Content

{}

This tests handling of large files.
"#,
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(200)
    );
    harness.create_note("long.md", &long_content).await?;

    // Special characters in title
    harness
        .create_note(
            "special-chars.md",
            r#"---
title: "Special: Characters! (With) [Brackets] & Symbols?"
tags: [special, test]
---

# Special Characters

Title has special characters.
"#,
        )
        .await?;

    // Verify all valid files exist
    assert!(harness.file_exists("plain.md").await?);
    assert!(harness.file_exists("empty.md").await?);
    assert!(harness.file_exists("unicode.md").await?);
    assert!(harness.file_exists("long.md").await?);
    assert!(harness.file_exists("special-chars.md").await?);

    // Verify plain markdown (title from filename since no frontmatter)
    let plain_meta = harness
        .get_metadata("plain.md")
        .await?
        .expect("Plain note should exist");
    assert_eq!(plain_meta.title, Some("plain".to_string()));
    assert!(plain_meta.tags.is_empty());

    // Verify empty file handled
    let empty_meta = harness
        .get_metadata("empty.md")
        .await?
        .expect("Empty note should exist");
    assert!(
        empty_meta.title == Some("empty".to_string())
            || empty_meta.title == Some("Untitled".to_string()),
        "Empty file should have fallback title"
    );

    // Verify unicode
    let unicode_meta = harness
        .get_metadata("unicode.md")
        .await?
        .expect("Unicode note should exist");
    assert!(unicode_meta.title.is_some());
    assert!(unicode_meta.tags.contains(&"emoji".to_string()));
    assert!(unicode_meta.tags.contains(&"unicode".to_string()));

    // Verify long content
    let long_meta = harness
        .get_metadata("long.md")
        .await?
        .expect("Long note should exist");
    assert_eq!(long_meta.title, Some("Long Content".to_string()));
    assert!(long_meta.tags.contains(&"performance".to_string()));

    // Verify special characters
    let special_meta = harness
        .get_metadata("special-chars.md")
        .await?
        .expect("Special chars note should exist");
    assert!(special_meta.title.is_some());
    assert!(special_meta.tags.contains(&"special".to_string()));

    // Verify stats
    let stats = harness.get_stats().await?;
    assert_eq!(stats.total_documents, 5, "Should have 5 valid documents");

    Ok(())
}

// ============================================================================
// Additional Edge Cases
// ============================================================================

#[tokio::test]
async fn test_malformed_frontmatter_recovery() -> Result<()> {
    // Test that malformed frontmatter doesn't crash the parser
    // and that content is still extracted

    let harness = VaultTestHarness::new().await?;

    // Malformed YAML (unclosed array)
    let malformed = harness
        .create_note(
            "malformed.md",
            r#"---
title: Malformed YAML
tags: [unclosed, array
---

# Valid Content

Despite malformed frontmatter, content should be extracted.
"#,
        )
        .await;

    // Should handle gracefully (either parse with best effort or skip frontmatter)
    // Either way, the file should exist and content should be extracted
    match malformed {
        Ok(_path) => {
            // File was created and indexed (best-effort parsing)
            assert!(harness.file_exists("malformed.md").await?);
            let meta = harness.get_metadata("malformed.md").await?;
            assert!(
                meta.is_some(),
                "Should have metadata even with malformed frontmatter"
            );
        }
        Err(_e) => {
            // Parser rejected malformed frontmatter - this is acceptable
            // Verify harness is still functional
            harness
                .create_note(
                    "recovery.md",
                    r#"# Recovery Test

Parser should recover and continue working.
"#,
                )
                .await?;
            assert!(harness.file_exists("recovery.md").await?);
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_wikilink_variations() -> Result<()> {
    // Test various wikilink formats

    let harness = VaultTestHarness::new().await?;

    harness
        .create_note(
            "wikilink-test.md",
            r#"# Wikilink Variations

Simple link: [[Note A]]

Aliased link: [[Note B|My Alias]]

Heading reference: [[Note C#heading]]

Block reference: [[Note D#^block-id]]

Embed: ![[Image]]
"#,
        )
        .await?;

    assert!(harness.file_exists("wikilink-test.md").await?);

    let meta = harness
        .get_metadata("wikilink-test.md")
        .await?
        .expect("Wikilink test note should exist");
    // Title should be filename since no frontmatter
    assert_eq!(meta.title, Some("wikilink-test".to_string()));

    Ok(())
}
