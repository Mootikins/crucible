use chrono::Utc;
use crucible_core::parser::{DocumentContent, Frontmatter, FrontmatterFormat, ParsedDocument};
use crucible_surrealdb::{kiln_integration, SurrealClient};
use std::path::PathBuf;

#[tokio::test]
async fn test_record_id_query_parsing() {
    // Create an in-memory client
    let client = SurrealClient::new_memory().await.unwrap();

    // Initialize the database schema (ignore error if already exists)
    let _ = kiln_integration::initialize_kiln_schema(&client).await;

    // Create a test document
    let mut doc = ParsedDocument::new(PathBuf::from("Projects/Rune_MCP/file.md"));

    // Add frontmatter
    let frontmatter = Frontmatter::new(
        r#"title: "Test Document""#.to_string(),
        FrontmatterFormat::Yaml,
    );
    doc.frontmatter = Some(frontmatter);

    // Add content
    doc.content = DocumentContent::new().with_plain_text("This is test content".to_string());
    doc.parsed_at = Utc::now();
    doc.content_hash = "test_hash".to_string();
    doc.file_size = 1024;

    // Store the document (kiln_root doesn't matter for this test)
    let kiln_root = PathBuf::from("/test/kiln");
    let record_id = kiln_integration::store_parsed_document(&client, &doc, &kiln_root)
        .await
        .unwrap();

    println!("Created record with ID: {}", record_id);

    // Test 1: Query by record ID using SELECT * FROM table:id syntax
    let result = client
        .query("SELECT * FROM notes:Projects_Rune_MCP_file_md", &[])
        .await
        .unwrap();

    assert_eq!(
        result.records.len(),
        1,
        "Should retrieve exactly one record"
    );
    assert_eq!(
        result.records[0].data.get("title").and_then(|v| v.as_str()),
        Some("Test Document"),
        "Should retrieve the correct document"
    );

    println!("✓ Record ID query parsing works correctly");

    // Test 2: Query by non-existent record ID should return empty result
    let result = client
        .query("SELECT * FROM notes:NonExistent_Document", &[])
        .await
        .unwrap();

    assert_eq!(
        result.records.len(),
        0,
        "Should return empty result for non-existent record"
    );

    println!("✓ Non-existent record ID query returns empty result");

    // Test 3: Verify case sensitivity (FROM vs from)
    let result = client
        .query("select * from notes:Projects_Rune_MCP_file_md", &[])
        .await
        .unwrap();

    assert_eq!(
        result.records.len(),
        1,
        "Should work with lowercase keywords"
    );

    println!("✓ Case-insensitive keyword parsing works");
}

#[tokio::test]
async fn test_record_id_vs_table_name() {
    let client = SurrealClient::new_memory().await.unwrap();
    let _ = kiln_integration::initialize_kiln_schema(&client).await;

    // Create test documents
    let mut doc1 = ParsedDocument::new(PathBuf::from("doc1.md"));
    doc1.frontmatter = Some(Frontmatter::new(
        r#"title: "Document 1""#.to_string(),
        FrontmatterFormat::Yaml,
    ));
    doc1.content = DocumentContent::new().with_plain_text("Content 1".to_string());
    doc1.parsed_at = Utc::now();
    doc1.content_hash = "hash1".to_string();
    doc1.file_size = 512;

    let mut doc2 = ParsedDocument::new(PathBuf::from("doc2.md"));
    doc2.frontmatter = Some(Frontmatter::new(
        r#"title: "Document 2""#.to_string(),
        FrontmatterFormat::Yaml,
    ));
    doc2.content = DocumentContent::new().with_plain_text("Content 2".to_string());
    doc2.parsed_at = Utc::now();
    doc2.content_hash = "hash2".to_string();
    doc2.file_size = 512;

    let kiln_root = PathBuf::from("/test/kiln");
    kiln_integration::store_parsed_document(&client, &doc1, &kiln_root)
        .await
        .unwrap();
    kiln_integration::store_parsed_document(&client, &doc2, &kiln_root)
        .await
        .unwrap();

    // Test: SELECT * FROM notes should return all records in the table
    let result = client.query("SELECT * FROM notes", &[]).await.unwrap();
    assert!(
        result.records.len() >= 2,
        "Should return all records when querying table name"
    );

    // Test: SELECT * FROM notes:doc1_md should return only the specific record
    let result = client
        .query("SELECT * FROM notes:doc1_md", &[])
        .await
        .unwrap();
    assert_eq!(
        result.records.len(),
        1,
        "Should return only the specific record when using record ID"
    );
    assert_eq!(
        result.records[0].data.get("title").and_then(|v| v.as_str()),
        Some("Document 1")
    );

    println!("✓ Record ID queries are distinct from table queries");
}

#[tokio::test]
async fn test_record_id_with_complex_ids() {
    let client = SurrealClient::new_memory().await.unwrap();
    let _ = kiln_integration::initialize_kiln_schema(&client).await;

    // Test with various path formats
    let test_cases = vec![
        ("simple.md", "Simple", "notes:simple_md"),
        (
            "Projects/Subfolder/File.md",
            "Nested",
            "notes:Projects_Subfolder_File_md",
        ),
        (
            "deeply/nested/path/to/file.md",
            "Deep",
            "notes:deeply_nested_path_to_file_md",
        ),
    ];

    for (path, title, expected_id) in &test_cases {
        let mut doc = ParsedDocument::new(PathBuf::from(path));
        doc.frontmatter = Some(Frontmatter::new(
            format!(r#"title: "{}""#, title),
            FrontmatterFormat::Yaml,
        ));
        doc.content = DocumentContent::new().with_plain_text("test content".to_string());
        doc.parsed_at = Utc::now();
        doc.content_hash = format!("hash_{}", title);
        doc.file_size = 512;

        let kiln_root = PathBuf::from("/test/kiln");
        let record_id = kiln_integration::store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        println!("Created document '{}' with ID: {}", title, record_id);

        // Query by the expected record ID
        let query = format!("SELECT * FROM {}", expected_id);
        let result = client.query(&query, &[]).await.unwrap();

        assert_eq!(
            result.records.len(),
            1,
            "Should retrieve record with ID: {}",
            expected_id
        );

        println!("✓ Record ID query works for: {}", expected_id);
    }
}
