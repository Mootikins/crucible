use crucible_core::parser::types::{DocumentContent, LatexExpression, ParsedDocument};
use crucible_surrealdb::eav_graph::{apply_eav_graph_schema, DocumentIngestor, EAVGraphStore};
use crucible_surrealdb::SurrealClient;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread")]
async fn test_latex_blocks_stored() {
    // Manually build a test document with LaTeX to test the ingestor
    let mut doc = ParsedDocument::default();
    doc.path = PathBuf::from("math.md");
    doc.content_hash = "test123".into();
    doc.content = DocumentContent::default();
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
    let ingestor = DocumentIngestor::new(&store);

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
