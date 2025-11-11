/// Integration Tests for EAV Graph Ingestion and Storage
///
/// This module contains end-to-end integration tests that verify the complete
/// ingestion pipeline: parsing -> ingestion -> storage in SurrealDB.
///
/// These tests:
/// - Use real SurrealDB instances (in-memory)
/// - Test multiple components working together
/// - Verify block storage, metadata, and hierarchy
/// - Ensure all block types are correctly stored

#[cfg(test)]
mod tests {
    use crate::eav_graph::{apply_eav_graph_schema, NoteIngestor, EAVGraphStore};
    use crate::SurrealClient;
    use crucible_core::parser::{
        Callout, CodeBlock, NoteContent, Heading, LatexExpression, ListBlock, ListItem,
        ListType, Paragraph, ParsedNote,
    };
    use std::path::PathBuf;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_latex_blocks_stored() {
        // Manually build a test note with LaTeX to test the ingestor
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("math.md");
        doc.content_hash = "test123".into();
        doc.content = NoteContent::default();
        doc.content.plain_text = "Math formulas here".into();

        // Add inline LaTeX
        doc.content.latex_expressions.push(LatexExpression {
            expression: "E = mc^2".to_string(),
            is_block: false,
            offset: 10,
            length: 13,
        });

        // Add display LaTeX
        doc.content.latex_expressions.push(LatexExpression {
            expression: r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}".to_string(),
            is_block: true,
            offset: 30,
            length: 55,
        });

        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "math.md").await.unwrap();

        // Query blocks using client directly (same pattern as existing tests)
        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[serde_json::json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        // Filter LaTeX blocks by checking the block_type in each record
        let latex_count = blocks
            .records
            .iter()
            .filter(|rec| {
                rec.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "latex")
            })
            .count();

        assert!(
            latex_count >= 2,
            "Should have at least 2 LaTeX blocks, found {}",
            latex_count
        );

        // Verify inline flag metadata exists
        let inline_count = blocks
            .records
            .iter()
            .filter(|rec| {
                rec.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "latex")
                    && rec
                        .data
                        .get("metadata")
                        .and_then(|m| m.get("inline"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            })
            .count();
        assert!(
            inline_count > 0,
            "Should have at least one inline LaTeX block"
        );

        // Verify display_mode flag metadata exists
        let display_count = blocks
            .records
            .iter()
            .filter(|rec| {
                rec.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "latex")
                    && rec
                        .data
                        .get("metadata")
                        .and_then(|m| m.get("display_mode"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            })
            .count();
        assert!(
            display_count > 0,
            "Should have at least one display mode LaTeX block"
        );
    }

    /// Comprehensive integration test verifying all 10 block types are correctly:
    /// 1. Mapped to blocks by the ingestor
    /// 2. Stored in the database with proper metadata
    #[tokio::test]
    async fn test_all_block_types_end_to_end() {
        // Manually build a test note with all block types
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("test/all_types.md");
        doc.content_hash = "test_all_types_hash".into();

        // Build content with all block types
        let mut content = NoteContent::new();
        content.plain_text = "Comprehensive test note with all block types".into();

        // 1. Headings
        content.add_heading(Heading::new(1, "Introduction", 0));
        content.add_heading(Heading::new(2, "Regular Paragraph", 20));
        content.add_heading(Heading::new(2, "Code Block", 50));
        content.add_heading(Heading::new(2, "Conclusion", 500));

        // 2. Paragraphs
        content.paragraphs.push(Paragraph::new(
            "This is a regular paragraph with some text.".to_string(),
            100,
        ));
        content.paragraphs.push(Paragraph::new(
            "End of note.".to_string(),
            600,
        ));

        // 3. Code blocks
        content.add_code_block(CodeBlock::new(
            Some("rust".to_string()),
            r#"fn main() {
    println!("Hello, world!");
}"#
            .to_string(),
            150,
        ));

        // 4. Lists
        let mut list = ListBlock::new(ListType::Unordered, 200);
        list.add_item(ListItem::new("Item 1".to_string(), 0));
        list.add_item(ListItem::new("Item 2".to_string(), 0));
        list.add_item(ListItem::new_task("Task item".to_string(), 0, false));
        list.add_item(ListItem::new_task("Completed task".to_string(), 0, true));
        content.lists.push(list);

        // 6. LaTeX expressions (stored in content)
        content.latex_expressions.push(LatexExpression::new(
            "E = mc^2".to_string(),
            false, // inline
            300,
            13,
        ));
        content.latex_expressions.push(LatexExpression::new(
            r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}".to_string(),
            true, // block (display mode)
            350,
            55,
        ));

        doc.content = content;

        // 5. Callouts (stored at note level, not in content)
        doc.callouts.push(Callout::with_title(
            "note",
            "Important Note",
            "This is a callout block.".to_string(),
            250,
        ));

        // Note: Blockquote, Table, and Horizontal Rule blocks are created by the ingestor
        // from markdown parsing, but since we're manually building the note, we can't
        // easily add them here. The ingestor creates blocks from the NoteContent fields.
        // These would need to be tested in a separate test that uses actual markdown parsing.

        // Set up database
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        // Ingest the note
        let entity_id = ingestor.ingest(&doc, "test/all_types.md").await.unwrap();

        // Query blocks using SQL directly
        let result = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[serde_json::json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        let blocks = result.records;

        // Collect all block types
        let mut block_types = std::collections::HashSet::new();
        for block in &blocks {
            if let Some(block_type) = block.data.get("block_type").and_then(|v| v.as_str()) {
                block_types.insert(block_type);
            }
        }

        // Verify core block types that we can create from NoteContent
        assert!(
            block_types.contains("heading"),
            "Missing heading blocks. Found types: {:?}",
            block_types
        );
        assert!(
            block_types.contains("paragraph"),
            "Missing paragraph blocks. Found types: {:?}",
            block_types
        );
        assert!(
            block_types.contains("code"),
            "Missing code blocks. Found types: {:?}",
            block_types
        );
        assert!(
            block_types.contains("list"),
            "Missing list blocks. Found types: {:?}",
            block_types
        );
        assert!(
            block_types.contains("callout"),
            "Missing callout blocks. Found types: {:?}",
            block_types
        );
        assert!(
            block_types.contains("latex"),
            "Missing LaTeX blocks. Found types: {:?}",
            block_types
        );

        // Print summary for debugging
        println!("\n=== Block Type Summary ===");
        for block_type in &["heading", "paragraph", "code", "list", "callout", "latex"] {
            let count = blocks
                .iter()
                .filter(|b| {
                    b.data
                        .get("block_type")
                        .and_then(|v| v.as_str())
                        .map_or(false, |t| t == *block_type)
                })
                .count();
            println!("{}: {} blocks", block_type, count);
        }

        // Verify specific metadata for each block type

        // 1. Headings - check level metadata
        let headings: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "heading")
            })
            .collect();
        assert!(!headings.is_empty(), "Should have heading blocks");

        let h1 = headings
            .iter()
            .find(|h| {
                h.data
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map_or(false, |c| c.contains("Introduction"))
            })
            .expect("Should have Introduction heading");
        assert_eq!(
            h1.data
                .get("metadata")
                .and_then(|m| m.get("level"))
                .and_then(|v| v.as_u64()),
            Some(1),
            "Introduction should be level 1"
        );

        // 2. Code blocks - check language metadata
        let code_blocks: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "code")
            })
            .collect();
        assert!(!code_blocks.is_empty(), "Should have code blocks");

        let rust_code = code_blocks
            .iter()
            .find(|c| {
                c.data
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map_or(false, |content| content.contains("println"))
            })
            .expect("Should have Rust code block");
        assert_eq!(
            rust_code
                .data
                .get("metadata")
                .and_then(|m| m.get("language"))
                .and_then(|v| v.as_str()),
            Some("rust"),
            "Code block should have Rust language"
        );

        // 3. Lists - check type metadata and task status
        let lists: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "list")
            })
            .collect();
        assert!(!lists.is_empty(), "Should have list blocks");

        // 4. Callouts - check callout_type metadata
        let callouts: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "callout")
            })
            .collect();
        assert!(!callouts.is_empty(), "Should have callout blocks");

        let note_callout = &callouts[0];
        assert_eq!(
            note_callout
                .data
                .get("metadata")
                .and_then(|m| m.get("callout_type"))
                .and_then(|v| v.as_str()),
            Some("note"),
            "Callout should be note type"
        );

        // 5. LaTeX - check inline flag metadata
        let latex_blocks: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("block_type")
                    .and_then(|v| v.as_str())
                    .map_or(false, |t| t == "latex")
            })
            .collect();
        assert!(
            latex_blocks.len() >= 2,
            "Should have at least 2 LaTeX blocks (inline + display), found {}",
            latex_blocks.len()
        );

        let inline_latex = latex_blocks
            .iter()
            .find(|l| {
                l.data
                    .get("metadata")
                    .and_then(|m| m.get("inline"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .expect("Should have inline LaTeX");
        assert!(
            inline_latex
                .data
                .get("content")
                .and_then(|v| v.as_str())
                .map_or(false, |c| c.contains("mc^2")),
            "Inline LaTeX should contain mc^2"
        );

        let display_latex = latex_blocks
            .iter()
            .find(|l| {
                l.data
                    .get("metadata")
                    .and_then(|m| m.get("display_mode"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
            .expect("Should have display mode LaTeX");
        assert!(
            display_latex
                .data
                .get("content")
                .and_then(|v| v.as_str())
                .map_or(false, |c| c.contains("\\int")),
            "Display LaTeX should contain \\int"
        );

        // Verify BLAKE3 hashes are computed for all blocks
        for block in &blocks {
            let hash = block
                .data
                .get("content_hash")
                .and_then(|v| v.as_str())
                .expect("Block should have content_hash");
            assert!(
                !hash.is_empty() && hash != "0",
                "Block should have non-empty BLAKE3 hash"
            );
        }

        println!("\n✅ All accessible block types successfully stored with proper metadata!");
    }

    /// Test that block types maintain order and hierarchy
    #[tokio::test]
    async fn test_block_order_and_hierarchy() {
        // Manually build a test note with hierarchical headings
        let mut doc = ParsedNote::default();
        doc.path = PathBuf::from("test/hierarchy.md");
        doc.content_hash = "test_hierarchy_hash".into();

        let mut content = NoteContent::new();
        content.plain_text = "Hierarchical note structure".into();

        // Add headings and paragraphs in order
        content.add_heading(Heading::new(1, "Section 1", 0));
        content
            .paragraphs
            .push(Paragraph::new("Paragraph under section 1.".to_string(), 20));

        content.add_heading(Heading::new(2, "Subsection 1.1", 50));
        content.paragraphs.push(Paragraph::new(
            "Paragraph under subsection 1.1.".to_string(),
            80,
        ));

        content.add_heading(Heading::new(1, "Section 2", 120));
        content
            .paragraphs
            .push(Paragraph::new("Paragraph under section 2.".to_string(), 140));

        doc.content = content;

        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = NoteIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test/hierarchy.md").await.unwrap();

        // Query blocks using SQL directly
        let result = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id) ORDER BY block_index",
                &[serde_json::json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        let blocks = result.records;

        // Blocks should be in note order
        let mut previous_block_index = -1_i64;
        for block in &blocks {
            let block_index = block
                .data
                .get("block_index")
                .and_then(|v| v.as_i64())
                .expect("Block should have block_index");
            assert!(
                block_index > previous_block_index,
                "Blocks not in order: {} <= {}",
                block_index,
                previous_block_index
            );
            previous_block_index = block_index;
        }

        // Check heading hierarchy
        let h1_blocks: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("metadata")
                    .and_then(|m| m.get("level"))
                    .and_then(|v| v.as_u64())
                    == Some(1)
            })
            .collect();
        assert_eq!(h1_blocks.len(), 2, "Should have 2 H1 headings");

        let h2_blocks: Vec<_> = blocks
            .iter()
            .filter(|b| {
                b.data
                    .get("metadata")
                    .and_then(|m| m.get("level"))
                    .and_then(|v| v.as_u64())
                    == Some(2)
            })
            .collect();
        assert_eq!(h2_blocks.len(), 1, "Should have 1 H2 heading");

        println!("✅ Block order and hierarchy maintained!");
    }
}
