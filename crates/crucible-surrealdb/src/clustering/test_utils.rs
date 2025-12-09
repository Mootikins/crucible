use super::{DocumentInfo};
use std::collections::HashSet;
use tempfile::TempDir;

/// Create a test directory with sample documents
pub fn create_test_kiln() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Create a simple MoC
    std::fs::write(
        path.join("Knowledge Hub.md"),
        r#"# Knowledge Management Hub

## Projects
- [[Project Alpha]]
- [[Project Beta]]

## Research
- [[AI Research]]
- [[Machine Learning Basics]]

## Meeting Notes
- [[Weekly Sync]]
- [[Project Retrospective]]
"#,
    ).unwrap();

    // Create linked documents
    std::fs::write(
        path.join("Project Alpha.md"),
        r#"---
tags: [project, active]
---

# Project Alpha

Started on 2024-01-01.

Related: [[Project Beta]], [[AI Research]]
"#,
    ).unwrap();

    std::fs::write(
        path.join("AI Research.md"),
        r#"---
tags: [research, ai]
---

# AI Research

Notes on artificial intelligence.

See also: [[Machine Learning Basics]]
"#,
    ).unwrap();

    // Create a regular note (not a MoC)
    std::fs::write(
        path.join("Daily Note 2024-01-01.md"),
        r#"---
tags: [daily]
---

# Daily Note - 2024-01-01

Worked on Project Alpha today. Made good progress on the implementation.

Also reviewed [[AI Research]] notes.
"#,
    ).unwrap();

    temp_dir
}

/// Create mock document info for testing
pub fn create_mock_documents() -> Vec<DocumentInfo> {
    vec![
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
            tags: vec!["project".to_string(), "active".to_string()],
            outbound_links: vec!["Project Beta.md".to_string(), "AI Research.md".to_string()],
            inbound_links: vec!["Knowledge Hub.md".to_string()],
            embedding: None,
            content_length: 1500,
        },
        DocumentInfo {
            file_path: "AI Research.md".to_string(),
            title: Some("AI Research".to_string()),
            tags: vec!["research".to_string(), "ai".to_string()],
            outbound_links: vec!["Machine Learning Basics.md".to_string()],
            inbound_links: vec!["Knowledge Hub.md".to_string(), "Project Alpha.md".to_string()],
            embedding: None,
            content_length: 2000,
        },
        DocumentInfo {
            file_path: "Daily Note 2024-01-01.md".to_string(),
            title: Some("Daily Note - 2024-01-01".to_string()),
            tags: vec!["daily".to_string()],
            outbound_links: vec!["Project Alpha.md".to_string(), "AI Research.md".to_string()],
            inbound_links: vec![],
            embedding: None,
            content_length: 300,
        },
    ]
}

/// Generate a set of documents with known link structure
pub fn generate_test_document_set(
    num_docs: usize,
    links_per_doc: usize,
) -> Vec<DocumentInfo> {
    let mut docs = Vec::new();
    let mut all_paths = HashSet::new();

    // Generate document paths
    for i in 0..num_docs {
        let path = format!("document_{}.md", i);
        all_paths.insert(path.clone());
    }

    let all_paths: Vec<String> = all_paths.into_iter().collect();

    // Create documents with links
    for (i, path) in all_paths.iter().enumerate() {
        let mut outbound_links = Vec::new();

        // Add some links (avoiding self-links)
        for j in 1..=links_per_doc.min(num_docs - 1) {
            let link_index = (i + j) % num_docs;
            outbound_links.push(all_paths[link_index].clone());
        }

        docs.push(DocumentInfo {
            file_path: path.clone(),
            title: Some(format!("Document {}", i)),
            tags: if i == 0 { vec!["moc".to_string()] } else { vec![] },
            outbound_links,
            inbound_links: vec![],
            embedding: None,
            content_length: 1000,
        });
    }

    // Calculate inbound links
    // Collect all links first, then update
    let mut inbound_updates: Vec<(String, String)> = Vec::new(); // (target_path, source_path)

    for i in 0..docs.len() {
        for link in &docs[i].outbound_links {
            inbound_updates.push((link.clone(), docs[i].file_path.clone()));
        }
    }

    // Now apply the updates
    for (target_path, source_path) in inbound_updates {
        if let Some(doc) = docs.iter_mut().find(|d| d.file_path == target_path) {
            doc.inbound_links.push(source_path);
        }
    }

    docs
}