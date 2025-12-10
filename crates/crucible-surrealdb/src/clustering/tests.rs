use super::heuristics::detect_mocs;
use super::DocumentInfo;

#[tokio::test]
async fn test_detect_mocs_identifies_hub_document() {
    // Arrange: Create documents with a clear hub/MoC structure
    let documents = vec![
        DocumentInfo {
            file_path: "Knowledge Hub.md".to_string(),
            title: Some("Knowledge Management Hub".to_string()),
            tags: vec!["hub".to_string(), "index".to_string()],
            outbound_links: vec![
                "Project Alpha.md".to_string(),
                "Project Beta.md".to_string(),
                "AI Research.md".to_string(),
                "Machine Learning Basics.md".to_string(),
                "Weekly Sync.md".to_string(),
                "Project Retrospective.md".to_string(),
            ],
            inbound_links: vec!["Project Alpha.md".to_string(), "AI Research.md".to_string()],
            embedding: None,
            content_length: 500,
        },
        DocumentInfo {
            file_path: "Project Alpha.md".to_string(),
            title: Some("Project Alpha".to_string()),
            tags: vec!["project".to_string()],
            outbound_links: vec!["AI Research.md".to_string()],
            inbound_links: vec!["Knowledge Hub.md".to_string()],
            embedding: None,
            content_length: 2000,
        },
        DocumentInfo {
            file_path: "Daily Note.md".to_string(),
            title: Some("Daily Note - 2024-01-01".to_string()),
            tags: vec!["daily".to_string()],
            outbound_links: vec![],
            inbound_links: vec![],
            embedding: None,
            content_length: 300,
        },
    ];

    // Act: Run MoC detection
    let mocs = detect_mocs(&documents).await.unwrap();

    // Assert: Should detect the hub as a MoC
    assert_eq!(mocs.len(), 1);
    let moc = &mocs[0];
    assert_eq!(moc.file_path, "Knowledge Hub.md");
    assert!(moc.score > 0.5); // Should have high confidence
    assert!(moc.outbound_links > 5); // Should have many outbound links
    assert!(moc.reasons.iter().any(|r| r.contains("outbound links")));
    assert!(moc.reasons.iter().any(|r| r.contains("hub") || r.contains("index")));
}

#[tokio::test]
async fn test_detect_mocs_ignores_regular_documents() {
    // Arrange: Create documents without MoC characteristics
    let documents = vec![
        DocumentInfo {
            file_path: "Regular Note.md".to_string(),
            title: Some("Regular Note".to_string()),
            tags: vec!["note".to_string()],
            outbound_links: vec!["Another Note.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 1500,
        },
        DocumentInfo {
            file_path: "Another Note.md".to_string(),
            title: Some("Another Note".to_string()),
            tags: vec!["note".to_string()],
            outbound_links: vec![],
            inbound_links: vec!["Regular Note.md".to_string()],
            embedding: None,
            content_length: 800,
        },
    ];

    // Act: Run MoC detection
    let mocs = detect_mocs(&documents).await.unwrap();

    // Assert: Should not detect any MoCs
    assert_eq!(mocs.len(), 0);
}

#[tokio::test]
async fn test_detect_mocs_prioritizes_by_score() {
    // Arrange: Create multiple potential MoCs with different scores
    let documents = vec![
        DocumentInfo {
            file_path: "Best MoC.md".to_string(),
            title: Some("Map of Content: Best".to_string()),
            tags: vec!["moc".to_string(), "hub".to_string()],
            outbound_links: vec!["Doc1.md".to_string(), "Doc2.md".to_string(), "Doc3.md".to_string(), "Doc4.md".to_string(), "Doc5.md".to_string(), "Doc6.md".to_string()],
            inbound_links: vec!["Doc1.md".to_string()],
            embedding: None,
            content_length: 400,
        },
        DocumentInfo {
            file_path: "Weak MoC.md".to_string(),
            title: Some("Some Index".to_string()),
            tags: vec!["index".to_string()],
            outbound_links: vec!["Doc1.md".to_string(), "Doc2.md".to_string(), "Doc3.md".to_string(), "Doc4.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 500,
        },
    ];

    // Act: Run MoC detection
    let mocs = detect_mocs(&documents).await.unwrap();

    // Assert: Best MoC should be first (sorted by score)
    assert_eq!(mocs.len(), 2);
    assert_eq!(mocs[0].file_path, "Best MoC.md");
    assert!(mocs[0].score > mocs[1].score);
}

#[tokio::test]
async fn test_detect_mocs_multiple_reasons() {
    // Arrange: Document with multiple MoC indicators
    let documents = vec![
        DocumentInfo {
            file_path: "Table of Contents.md".to_string(),
            title: Some("Table of Contents".to_string()),
            tags: vec!["moc".to_string(), "overview".to_string()],
            outbound_links: vec!["Chapter1.md".to_string(), "Chapter2.md".to_string(), "Chapter3.md".to_string(), "Chapter4.md".to_string(), "Chapter5.md".to_string()],
            inbound_links: vec!["Chapter1.md".to_string(), "Chapter2.md".to_string(), "Chapter3.md".to_string(), "Chapter4.md".to_string()],
            embedding: None,
            content_length: 800,
        },
    ];

    // Act: Run MoC detection
    let mocs = detect_mocs(&documents).await.unwrap();

    // Assert: Should capture multiple reasons
    assert_eq!(mocs.len(), 1);
    let reasons = &mocs[0].reasons;

    // Should detect title pattern
    assert!(reasons.iter().any(|r| r.contains("Table of Contents")));

    // Should detect tags
    assert!(reasons.iter().any(|r| r.contains("moc")));

    // Should detect hub behavior
    assert!(reasons.iter().any(|r| r.contains("Acts as a link hub")));
}