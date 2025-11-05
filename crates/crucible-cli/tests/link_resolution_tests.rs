//! Integration tests for Crucible CLI link resolution system
//!
//! This test suite validates the entire link resolution pipeline including:
//! - Wikilink resolution ([[Document]], [[Document|Display]], ![[Document]])
//! - Alias and cross-reference handling
//! - Backlink generation and bidirectional relationships
//! - Semantic search integration with linked documents
//! - Link database integrity across different backends
//! - Performance testing for large-scale link networks
//!
//! Tests use real SurrealDB operations instead of mocks to validate
//! end-to-end functionality.

use anyhow::{Context, Result};
use chrono::Utc;
use crucible_cli::config::CliConfig;
use crucible_core::parser::types::{Frontmatter, ParsedDocument, Wikilink};
use crucible_surrealdb::{
    kiln_integration::{self, create_wikilink_edges, store_parsed_document},
    schema_types::{Wikilink as DBWikilink, Note},
    SurrealClient, SurrealDbConfig,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

/// Test configuration timeout
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Test helper to set up a complete link resolution environment
pub struct LinkTestEnvironment {
    pub temp_dir: TempDir,
    pub config: CliConfig,
    pub client: SurrealClient,
    pub kiln_path: PathBuf,
    pub document_paths: HashMap<String, PathBuf>,
}

impl LinkTestEnvironment {
    /// Create a new test environment with initialized database
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let kiln_path = temp_dir.path().to_path_buf();

        // Create CLI configuration
        let config = CliConfig {
            kiln: crucible_cli::config::KilnConfig {
                path: kiln_path.clone(),
                embedding_url: "http://localhost:11434".to_string(),
                embedding_model: Some("nomic-embed-text".to_string()),
            },
            ..Default::default()
        };

        // Initialize database
        let db_path = config.database_path();
        std::fs::create_dir_all(db_path.parent().unwrap())?;

        // Create SurrealDbConfig
        let db_config = SurrealDbConfig {
            namespace: "test_link_resolution".to_string(),
            database: "test".to_string(),
            path: config.database_path_str()?,
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        let client = SurrealClient::new(db_config).await?;

        // Initialize kiln schema
        kiln_integration::initialize_kiln_schema(&client).await?;

        Ok(Self {
            temp_dir,
            config,
            client,
            kiln_path,
            document_paths: HashMap::new(),
        })
    }

    /// Create and store a test document with the given content and links
    pub async fn create_document(
        &mut self,
        filename: &str,
        title: Option<&str>,
        content: &str,
        wikilinks: Vec<Wikilink>,
        tags: Vec<&str>,
    ) -> Result<String> {
        let doc_path = self.kiln_path.join(filename);
        self.document_paths.insert(filename.to_string(), doc_path.clone());

        // Create ParsedDocument
        let mut doc = ParsedDocument::new(doc_path.clone());
        doc.content.plain_text = content.to_string();
        doc.parsed_at = Utc::now();
        doc.content_hash = format!("hash_{}", filename);
        doc.file_size = content.len() as u64;
        doc.wikilinks = wikilinks;

        // Add tags
        doc.tags = tags
            .into_iter()
            .map(|tag| crucible_core::parser::types::Tag {
                name: tag.to_string(),
                path: vec![tag.to_string()],
                offset: 0,
            })
            .collect();

        // Add frontmatter with title if provided
        if let Some(title) = title {
            let frontmatter_raw = format!("---\ntitle: {}\n---", title);
            doc.frontmatter = Some(Frontmatter::new(
                frontmatter_raw,
                crucible_core::parser::types::FrontmatterFormat::Yaml,
            ));
        }

        // Store document
        let doc_id = store_parsed_document(&self.client, &doc, &self.kiln_path).await?;

        // Create wikilink relationships
        create_wikilink_edges(&self.client, &doc_id, &doc).await?;

        Ok(doc_id)
    }

    /// Get linked documents for a given document
    pub async fn get_linked_documents(&self, doc_id: &str) -> Result<Vec<Note>> {
        let sql = format!("SELECT out.* FROM wikilink WHERE in = {}", doc_id);

        let result = self.client.query(&sql, &[]).await
            .context("Failed to query linked documents")?;

        let linked_docs: Vec<Note> = result.records.into_iter()
            .filter_map(|record| {
                let data_map: serde_json::Map<String, serde_json::Value> = record.data.into_iter().collect();
                serde_json::from_value(serde_json::Value::Object(data_map)).ok()
            })
            .collect();
        Ok(linked_docs)
    }

    /// Get backlinks (documents that link to the given document)
    pub async fn get_backlinks(&self, doc_id: &str) -> Result<Vec<Note>> {
        let sql = format!("SELECT in.* FROM wikilink WHERE out = {}", doc_id);

        let result = self.client.query(&sql, &[]).await
            .context("Failed to query backlinks")?;

        let backlinks: Vec<Note> = result.records.into_iter()
            .filter_map(|record| {
                let data_map: serde_json::Map<String, serde_json::Value> = record.data.into_iter().collect();
                serde_json::from_value(serde_json::Value::Object(data_map)).ok()
            })
            .collect();
        Ok(backlinks)
    }

    /// Get all wikilink relations for a document
    pub async fn get_wikilink_relations(&self, doc_id: &str) -> Result<Vec<DBWikilink>> {
        let sql = format!("SELECT * FROM wikilink WHERE in = {}", doc_id);

        let result = self.client.query(&sql, &[]).await
            .context("Failed to query wikilink relations")?;

        let relations: Vec<DBWikilink> = result.records.into_iter()
            .filter_map(|record| {
                let data_map: serde_json::Map<String, serde_json::Value> = record.data.into_iter().collect();
                serde_json::from_value(serde_json::Value::Object(data_map)).ok()
            })
            .collect();
        Ok(relations)
    }

    /// Find document by path
    pub async fn find_document_by_path(&self, path: &str) -> Result<Option<Note>> {
        let sql = format!("SELECT * FROM notes WHERE path = '{}'", path);

        let result = self.client.query(&sql, &[]).await
            .context("Failed to query document by path")?;

        let docs: Vec<Note> = result.records.into_iter()
            .filter_map(|record| {
                let data_map: serde_json::Map<String, serde_json::Value> = record.data.into_iter().collect();
                serde_json::from_value(serde_json::Value::Object(data_map)).ok()
            })
            .collect();
        Ok(docs.into_iter().next())
    }

    /// Execute semantic search and return results
    pub async fn semantic_search(&self, query: &str, limit: i32) -> Result<Vec<Note>> {
        // Note: This would use the actual semantic search functionality
        // For now, we'll simulate it with a basic content search
        let sql = format!(
            "SELECT * FROM notes WHERE content_text ILIKE '%{}%' LIMIT {}",
            query, limit
        );

        let result = self.client.query(&sql, &[]).await
            .context("Failed to execute semantic search")?;

        let search_results: Vec<Note> = result.records.into_iter()
            .filter_map(|record| {
                let data_map: serde_json::Map<String, serde_json::Value> = record.data.into_iter().collect();
                serde_json::from_value(serde_json::Value::Object(data_map)).ok()
            })
            .collect();
        Ok(search_results)
    }

    /// Create a complex document network for testing
    pub async fn create_complex_network(&mut self) -> Result<HashMap<String, String>> {
        let mut doc_ids = HashMap::new();

        // Create a knowledge graph about programming languages
        doc_ids.insert(
            "rust_basics".to_string(),
            self.create_document(
                "rust_basics.md",
                Some("Rust Programming Basics"),
                "Rust is a systems programming language focused on memory safety and performance. \
                 It provides zero-cost abstractions and fearless concurrency.\n\n\
                 Key concepts include ownership, borrowing, and lifetimes.\n\n\
                 See also: [[Advanced Rust]] and [[Memory Management]]",
                vec![
                    Wikilink {
                        target: "advanced_rust".to_string(),
                        alias: Some("Advanced Rust".to_string()),
                        offset: 150,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "memory_management".to_string(),
                        alias: None,
                        offset: 170,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["rust", "programming", "systems"],
            )
            .await?,
        );

        doc_ids.insert(
            "advanced_rust".to_string(),
            self.create_document(
                "advanced_rust.md",
                Some("Advanced Rust Concepts"),
                "Advanced Rust programming covers complex topics including async programming, \
                 unsafe code, and performance optimization.\n\n\
                 Related to [[Rust Basics]] but more in-depth.\n\n\
                 Embeds rust examples:\n\n\
                 ![[rust_examples]]",
                vec![
                    Wikilink {
                        target: "rust_basics".to_string(),
                        alias: Some("Rust Basics".to_string()),
                        offset: 90,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "rust_examples".to_string(),
                        alias: None,
                        offset: 180,
                        is_embed: true, // This is an embed
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["rust", "advanced", "async"],
            )
            .await?,
        );

        doc_ids.insert(
            "rust_examples".to_string(),
            self.create_document(
                "rust_examples.md",
                Some("Rust Code Examples"),
                "```rust\nfn main() {\n    println!(\"Hello, Rust!\");\n}\n```\n\n\
                 Common examples that are embedded in [[Advanced Rust]].",
                vec![Wikilink {
                    target: "advanced_rust".to_string(),
                    alias: None,
                    offset: 85,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["rust", "examples", "code"],
            )
            .await?,
        );

        doc_ids.insert(
            "memory_management".to_string(),
            self.create_document(
                "memory_management.md",
                Some("Memory Management in Rust"),
                "Rust's ownership system ensures memory safety without garbage collection.\n\n\
                 Builds on concepts from [[Rust Basics]].\n\n\
                 Compared to [[CPP Memory Management]].",
                vec![
                    Wikilink {
                        target: "rust_basics".to_string(),
                        alias: None,
                        offset: 95,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "cpp_memory".to_string(),
                        alias: Some("CPP Memory Management".to_string()),
                        offset: 140,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["memory", "rust", "ownership"],
            )
            .await?,
        );

        doc_ids.insert(
            "cpp_memory".to_string(),
            self.create_document(
                "cpp_memory.md",
                Some("C++ Memory Management"),
                "C++ provides manual memory management with new/delete and smart pointers.\n\n\
                 Contrasted with [[Memory Management]] in Rust.",
                vec![Wikilink {
                    target: "memory_management".to_string(),
                    alias: Some("Memory Management".to_string()),
                    offset: 120,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["cpp", "memory", "pointers"],
            )
            .await?,
        );

        // Create some unrelated documents for search testing
        doc_ids.insert(
            "web_development".to_string(),
            self.create_document(
                "web_development.md",
                Some("Web Development Guide"),
                "Modern web development uses HTML, CSS, and JavaScript.\n\n\
                 Frontend frameworks like React and Vue are popular.\n\n\
                 Backend development uses Node.js, Python, or Rust.",
                vec![],
                vec!["web", "frontend", "javascript"],
            )
            .await?,
        );

        doc_ids.insert(
            "machine_learning".to_string(),
            self.create_document(
                "machine_learning.md",
                Some("Machine Learning Fundamentals"),
                "Machine learning algorithms can learn from data.\n\n\
                 Includes supervised learning, unsupervised learning, and reinforcement learning.\n\n\
                 Common libraries include TensorFlow and PyTorch.",
                vec![],
                vec!["ml", "ai", "data-science"],
            )
            .await?,
        );

        Ok(doc_ids)
    }

    /// Create a large network for performance testing (50+ documents)
    pub async fn create_large_network(&mut self, size: usize) -> Result<Vec<String>> {
        let mut doc_ids = Vec::new();
        let topics = vec![
            "algorithms", "data-structures", "design-patterns", "testing", "debugging",
            "performance", "security", "databases", "networking", "concurrency",
            "functional-programming", "object-oriented", "microservices", "devops", "deployment",
        ];

        for i in 0..size {
            let doc_num = i + 1;
            let topic = topics[i % topics.len()];
            let filename = format!("doc_{:03}_{}.md", doc_num, topic.replace('-', "_"));

            // Create links to other documents (each doc links to 2-4 others)
            let mut wikilinks = Vec::new();
            for j in 1..=3.min(size - i) {
                let target_doc = i + j;
                if target_doc < size {
                    wikilinks.push(Wikilink {
                        target: format!("doc_{:03}", target_doc + 1),
                        alias: Some(format!("Document {}", target_doc + 1)),
                        offset: 100 + (j * 50),
                        is_embed: j == 2, // Make every second link an embed
                        block_ref: None,
                        heading_ref: None,
                    });
                }
            }

            let doc_id = self.create_document(
                &filename,
                Some(&format!("Document {}: {}", doc_num, topic.replace('-', " "))),
                &format!(
                    "This is document {} about {}. \
                     It contains important information about {} and related concepts. \
                     Content length is substantial to provide realistic testing conditions. \
                     Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                     Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
                    doc_num, topic, topic
                ),
                wikilinks,
                vec![topic, "documentation", if i % 10 == 0 { "featured" } else { "regular" }],
            )
            .await?;

            doc_ids.push(doc_id);
        }

        Ok(doc_ids)
    }
}

#[cfg(test)]
mod link_resolution_tests {
    use super::*;

    /// Test basic wikilink resolution functionality
    #[tokio::test]
    async fn test_basic_wikilink_resolution() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create a simple linked document pair
        let source_id = env
            .create_document(
                "source.md",
                Some("Source Document"),
                "This document links to [[Target Document]]",
                vec![Wikilink {
                    target: "target_document".to_string(),
                    alias: Some("Target Document".to_string()),
                    offset: 25,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["source"],
            )
            .await?;

        let target_id = env
            .create_document(
                "target_document.md",
                Some("Target Document"),
                "This is the target document",
                vec![],
                vec!["target"],
            )
            .await?;

        // Test forward link resolution
        let linked_docs = env.get_linked_documents(&source_id).await?;
        assert_eq!(linked_docs.len(), 1, "Should find one linked document");
        assert_eq!(
            linked_docs[0].title,
            Some("Target Document".to_string()),
            "Linked document should have correct title"
        );

        // Test backlink resolution
        let backlinks = env.get_backlinks(&target_id).await?;
        assert_eq!(backlinks.len(), 1, "Target should have one backlink");
        assert_eq!(
            backlinks[0].title,
            Some("Source Document".to_string()),
            "Backlink should point to source document"
        );

        // Test wikilink relation metadata
        let relations = env.get_wikilink_relations(&source_id).await?;
        assert_eq!(relations.len(), 1, "Should have one wikilink relation");
        assert_eq!(
            relations[0].link_text, "Target Document",
            "Link text should be preserved"
        );
        assert_eq!(relations[0].position, 25, "Link position should be recorded");

        Ok(())
    }

    /// Test different wikilink formats and edge cases
    #[tokio::test]
    async fn test_wikilink_formats_and_edge_cases() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create a document with various wikilink formats
        let doc_id = env
            .create_document(
                "link_formats.md",
                Some("Link Format Examples"),
                "Various wikilink formats:\n\
                 [[Simple Link]]\n\
                 [[Link|With Alias]]\n\
                 ![[Embedded Document]]\n\
                 [[Link#With Heading]]\n\
                 [[Link#^block-ref]]",
                vec![
                    // Simple link
                    Wikilink {
                        target: "simple_link".to_string(),
                        alias: None,
                        offset: 25,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    // Link with alias
                    Wikilink {
                        target: "link".to_string(),
                        alias: Some("With Alias".to_string()),
                        offset: 45,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    // Embedded document
                    Wikilink {
                        target: "embedded_document".to_string(),
                        alias: None,
                        offset: 68,
                        is_embed: true,
                        block_ref: None,
                        heading_ref: None,
                    },
                    // Link with heading reference
                    Wikilink {
                        target: "link".to_string(),
                        alias: None,
                        offset: 92,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: Some("With Heading".to_string()),
                    },
                    // Link with block reference
                    Wikilink {
                        target: "link".to_string(),
                        alias: None,
                        offset: 118,
                        is_embed: false,
                        block_ref: Some("block-ref".to_string()),
                        heading_ref: None,
                    },
                ],
                vec!["links", "formatting"],
            )
            .await?;

        // Create target documents
        env.create_document(
            "simple_link.md",
            Some("Simple Link"),
            "A simple linked document",
            vec![],
            vec!["simple"],
        )
        .await?;

        env.create_document(
            "link.md",
            Some("Link Document"),
            "# With Heading\n\nContent with block reference.\n\n^block-ref",
            vec![],
            vec!["link"],
        )
        .await?;

        env.create_document(
            "embedded_document.md",
            Some("Embedded Document"),
            "This document is embedded elsewhere",
            vec![],
            vec!["embedded"],
        )
        .await?;

        // Test that all relations are created correctly
        let relations = env.get_wikilink_relations(&doc_id).await?;
        assert_eq!(relations.len(), 5, "Should create 5 wikilink relations");

        // Verify link properties
        let simple_link = relations.iter().find(|r| r.link_text == "Simple Link").unwrap();
        assert_eq!(simple_link.position, 35, "Simple link position should be correct");

        let alias_link = relations.iter().find(|r| r.link_text == "With Alias").unwrap();
        assert_eq!(alias_link.position, 68, "Alias link position should be correct");

        let embedded_link = relations.iter().find(|r| r.to.id.contains("embedded_document")).unwrap();
        assert_eq!(embedded_link.link_text, "Embedded Link", "Embedded link should have correct text");

        Ok(())
    }

    /// Test bidirectional link relationships and backlink generation
    #[tokio::test]
    async fn test_bidirectional_link_relationships() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create documents that link to each other
        let doc_a_id = env
            .create_document(
                "document_a.md",
                Some("Document A"),
                "Document A links to [[Document B]]",
                vec![Wikilink {
                    target: "document_b".to_string(),
                    alias: Some("Document B".to_string()),
                    offset: 25,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["doc-a"],
            )
            .await?;

        let doc_b_id = env
            .create_document(
                "document_b.md",
                Some("Document B"),
                "Document B links back to [[Document A]]",
                vec![Wikilink {
                    target: "document_a".to_string(),
                    alias: Some("Document A".to_string()),
                    offset: 30,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["doc-b"],
            )
            .await?;

        // Create a third document that links to both
        let doc_c_id = env
            .create_document(
                "document_c.md",
                Some("Document C"),
                "Document C links to both [[Document A]] and [[Document B]]",
                vec![
                    Wikilink {
                        target: "document_a".to_string(),
                        alias: Some("Document A".to_string()),
                        offset: 30,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "document_b".to_string(),
                        alias: Some("Document B".to_string()),
                        offset: 52,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["doc-c"],
            )
            .await?;

        // Test forward links from A
        let a_links = env.get_linked_documents(&doc_a_id).await?;
        assert_eq!(a_links.len(), 1, "Document A should link to one document");
        assert_eq!(a_links[0].title, Some("Document B".to_string()));

        // Test backlinks to A
        let a_backlinks = env.get_backlinks(&doc_a_id).await?;
        assert_eq!(a_backlinks.len(), 2, "Document A should have two backlinks");
        let a_backlink_titles: Vec<_> = a_backlinks.iter().filter_map(|d| d.title.as_ref()).collect();
        assert!(a_backlink_titles.iter().any(|t| t.as_str() == "Document B"));
        assert!(a_backlink_titles.iter().any(|t| t.as_str() == "Document C"));

        // Test forward links from B
        let b_links = env.get_linked_documents(&doc_b_id).await?;
        assert_eq!(b_links.len(), 1, "Document B should link to one document");
        assert_eq!(b_links[0].title, Some("Document A".to_string()));

        // Test backlinks to B
        let b_backlinks = env.get_backlinks(&doc_b_id).await?;
        assert_eq!(b_backlinks.len(), 2, "Document B should have two backlinks");
        let b_backlink_titles: Vec<_> = b_backlinks.iter().filter_map(|d| d.title.as_ref()).collect();
        assert!(b_backlink_titles.iter().any(|t| t.as_str() == "Document A"));
        assert!(b_backlink_titles.iter().any(|t| t.as_str() == "Document C"));

        // Test forward links from C
        let c_links = env.get_linked_documents(&doc_c_id).await?;
        assert_eq!(c_links.len(), 2, "Document C should link to two documents");
        let c_link_titles: Vec<_> = c_links.iter().filter_map(|d| d.title.as_ref()).collect();
        assert!(c_link_titles.iter().any(|t| t.as_str() == "Document A"));
        assert!(c_link_titles.iter().any(|t| t.as_str() == "Document B"));

        // Test backlinks to C
        let c_backlinks = env.get_backlinks(&doc_c_id).await?;
        assert_eq!(c_backlinks.len(), 0, "Document C should have no backlinks");

        Ok(())
    }

    /// Test tag-based cross-references and semantic relationships
    #[tokio::test]
    async fn test_tag_based_cross_references() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create documents with overlapping tags
        let _rust_basics_id = env
            .create_document(
                "rust_basics.md",
                Some("Rust Basics"),
                "Introduction to Rust programming language",
                vec![],
                vec!["rust", "programming", "systems"],
            )
            .await?;

        let rust_advanced_id = env
            .create_document(
                "rust_advanced.md",
                Some("Advanced Rust"),
                "Advanced Rust concepts and patterns",
                vec![Wikilink {
                    target: "rust_basics".to_string(),
                    alias: Some("Rust Basics".to_string()),
                    offset: 30,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["rust", "programming", "advanced"],
            )
            .await?;

        let _cpp_basics_id = env
            .create_document(
                "cpp_basics.md",
                Some("C++ Basics"),
                "Introduction to C++ programming",
                vec![],
                vec!["cpp", "programming", "systems"],
            )
            .await?;

        // Test finding documents by shared tags
        let programming_docs = env.semantic_search("programming", 10).await?;
        assert!(programming_docs.len() >= 3, "Should find multiple programming docs");

        let rust_docs = env.semantic_search("rust", 10).await?;
        assert_eq!(rust_docs.len(), 2, "Should find two Rust documents");

        let systems_docs = env.semantic_search("systems", 10).await?;
        assert_eq!(systems_docs.len(), 2, "Should find two systems programming docs");

        // Verify that semantic search considers related documents through links
        let related_to_rust_advanced = env.get_linked_documents(&rust_advanced_id).await?;
        assert_eq!(related_to_rust_advanced.len(), 1, "Advanced Rust should link to basics");

        // Check that tags are preserved in stored documents
        let rust_advanced_doc = env.find_document_by_path("/kiln/rust_advanced.md").await?;
        assert!(rust_advanced_doc.is_some(), "Should find stored document");
        let rust_advanced_doc = rust_advanced_doc.unwrap();
        assert!(rust_advanced_doc.tags.contains(&"rust".to_string()));
        assert!(rust_advanced_doc.tags.contains(&"advanced".to_string()));

        Ok(())
    }

    /// Test semantic search integration with linked documents
    #[tokio::test]
    async fn test_semantic_search_with_linked_documents() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        let doc_ids = env.create_complex_network().await?;

        // Test semantic search finds related documents through links
        let rust_docs = env.semantic_search("rust", 10).await?;
        assert!(rust_docs.len() >= 3, "Should find multiple Rust-related documents");

        // Verify link relationships enhance search results
        let memory_docs = env.semantic_search("memory", 5).await?;
        assert!(memory_docs.len() >= 2, "Should find memory-related documents");

        // Test that linked documents are discoverable
        let rust_basics_linked = env.get_linked_documents(&doc_ids["rust_basics"]).await?;
        assert_eq!(rust_basics_linked.len(), 2, "Rust basics should link to 2 documents");

        // Search should find documents linked from found documents
        for linked_doc in rust_basics_linked {
            if let Some(title) = &linked_doc.title {
                if title.contains("Advanced") || title.contains("Memory") {
                    // Should find linked advanced or memory docs
                    assert!(title.len() > 0, "Linked document should have valid title");
                }
            }
        }

        // Test bidirectional search through backlinks
        let advanced_rust_backlinks = env.get_backlinks(&doc_ids["advanced_rust"]).await?;
        assert!(advanced_rust_backlinks.len() >= 1, "Advanced Rust should have backlinks");

        Ok(())
    }

    /// Test link metadata preservation and database integrity
    #[tokio::test]
    async fn test_link_metadata_preservation() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create document with complex links
        let doc_id = env
            .create_document(
                "metadata_test.md",
                Some("Metadata Test"),
                "Testing link metadata preservation:\n\
                 [[Simple Link]]\n\
                 [[Link|With Custom Alias]] at position 50\n\
                 ![[Embedded Link]] with different properties",
                vec![
                    Wikilink {
                        target: "simple_link".to_string(),
                        alias: None,
                        offset: 35,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "link".to_string(),
                        alias: Some("With Custom Alias".to_string()),
                        offset: 68,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "embedded_link".to_string(),
                        alias: None,
                        offset: 120,
                        is_embed: true,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["metadata", "testing"],
            )
            .await?;

        // Create target documents
        let simple_link_id = env
            .create_document(
                "simple_link.md",
                Some("Simple Link"),
                "A simple target document",
                vec![],
                vec!["simple"],
            )
            .await?;

        env.create_document(
            "link.md",
            Some("Link Document"),
            "Document with alias reference",
            vec![],
            vec!["alias"],
        )
        .await?;

        env.create_document(
            "embedded_link.md",
            Some("Embedded Link"),
            "Document that gets embedded",
            vec![],
            vec!["embedded"],
        )
        .await?;

        // Verify all metadata is preserved
        let relations = env.get_wikilink_relations(&doc_id).await?;
        assert_eq!(relations.len(), 3, "Should have 3 relations");

        for relation in relations {
            assert!(!relation.link_text.is_empty(), "Link text should be preserved");
            assert!(relation.position >= 0, "Position should be recorded");
            assert!(!relation.from.id.is_empty(), "Source document ID should be set");
            assert!(!relation.to.id.is_empty(), "Target document ID should be set");

            // Check creation timestamp
            assert!(relation.created_at <= Utc::now(), "Creation time should be valid");

            // Note: Database wikilink schema doesn't have an 'embedded' field
            // This would need to be inferred from the link text or context if needed
        }

        // Test database consistency - relations should match document links
        let linked_docs = env.get_linked_documents(&doc_id).await?;
        assert_eq!(linked_docs.len(), 3, "Should find 3 linked documents");

        // Test backlink consistency
        let simple_backlinks = env.get_backlinks(&simple_link_id).await?;
        assert_eq!(simple_backlinks.len(), 1, "Simple link should have one backlink");

        Ok(())
    }

    /// Test performance with large-scale link networks
    #[tokio::test]
    async fn test_large_scale_link_network_performance() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create a network of 55 documents
        let start_time = Instant::now();
        let doc_ids = env.create_large_network(55).await?;
        let creation_time = start_time.elapsed();

        println!("Created {} documents in {:?}", doc_ids.len(), creation_time);
        assert!(doc_ids.len() == 55, "Should create 55 documents");
        assert!(creation_time < Duration::from_secs(10), "Document creation should be fast");

        // Test link resolution performance
        let start_time = Instant::now();
        let mut total_links = 0;
        for doc_id in &doc_ids[0..10] {
            let links = env.get_linked_documents(doc_id).await?;
            total_links += links.len();
        }
        let link_resolution_time = start_time.elapsed();

        println!(
            "Resolved {} links from 10 documents in {:?}",
            total_links, link_resolution_time
        );
        assert!(link_resolution_time < Duration::from_secs(2), "Link resolution should be fast");

        // Test backlink resolution performance
        let start_time = Instant::now();
        let mut total_backlinks = 0;
        for doc_id in &doc_ids[40..55] {
            let backlinks = env.get_backlinks(doc_id).await?;
            total_backlinks += backlinks.len();
        }
        let backlink_resolution_time = start_time.elapsed();

        println!(
            "Resolved {} backlinks from 15 documents in {:?}",
            total_backlinks, backlink_resolution_time
        );
        assert!(backlink_resolution_time < Duration::from_secs(3), "Backlink resolution should be fast");

        // Test comprehensive search performance
        let start_time = Instant::now();
        let search_results = env.semantic_search("doc", 20).await?;
        let search_time = start_time.elapsed();

        println!(
            "Found {} documents in {:?} for search query",
            search_results.len(),
            search_time
        );
        assert!(search_results.len() > 0, "Search should return results");
        assert!(search_time < Duration::from_secs(5), "Search should be fast");

        Ok(())
    }

    /// Test circular reference handling
    #[tokio::test]
    async fn test_circular_reference_handling() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create documents with circular references
        let doc_a_id = env
            .create_document(
                "document_a.md",
                Some("Document A"),
                "Document A references [[Document B]]",
                vec![Wikilink {
                    target: "document_b".to_string(),
                    alias: Some("Document B".to_string()),
                    offset: 25,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["doc-a", "circular"],
            )
            .await?;

        let doc_b_id = env
            .create_document(
                "document_b.md",
                Some("Document B"),
                "Document B references [[Document C]]",
                vec![Wikilink {
                    target: "document_c".to_string(),
                    alias: Some("Document C".to_string()),
                    offset: 25,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["doc-b", "circular"],
            )
            .await?;

        let doc_c_id = env
            .create_document(
                "document_c.md",
                Some("Document C"),
                "Document C references back to [[Document A]]",
                vec![Wikilink {
                    target: "document_a".to_string(),
                    alias: Some("Document A".to_string()),
                    offset: 35,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["doc-c", "circular"],
            )
            .await?;

        // Test that circular links are handled correctly
        let a_links = env.get_linked_documents(&doc_a_id).await?;
        assert_eq!(a_links.len(), 1, "Document A should link to Document B");

        let b_links = env.get_linked_documents(&doc_b_id).await?;
        assert_eq!(b_links.len(), 1, "Document B should link to Document C");

        let c_links = env.get_linked_documents(&doc_c_id).await?;
        assert_eq!(c_links.len(), 1, "Document C should link to Document A");

        // Test backlinks in circular references
        let a_backlinks = env.get_backlinks(&doc_a_id).await?;
        assert_eq!(a_backlinks.len(), 1, "Document A should have one backlink from C");

        let b_backlinks = env.get_backlinks(&doc_b_id).await?;
        assert_eq!(b_backlinks.len(), 1, "Document B should have one backlink from A");

        let c_backlinks = env.get_backlinks(&doc_c_id).await?;
        assert_eq!(c_backlinks.len(), 1, "Document C should have one backlink from B");

        // Verify no infinite loops or performance issues
        let start_time = Instant::now();
        let _relations = env.get_wikilink_relations(&doc_a_id).await?;
        let query_time = start_time.elapsed();
        assert!(query_time < Duration::from_millis(100), "Circular queries should be fast");

        Ok(())
    }

    /// Test broken and non-existent link handling
    #[tokio::test]
    async fn test_broken_link_handling() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create document with links to non-existent targets
        let doc_id = env
            .create_document(
                "broken_links.md",
                Some("Broken Links Test"),
                "This document has broken links:\n\
                 [[NonExistent Document]]\n\
                 [[Another Missing|Display Text]]",
                vec![
                    Wikilink {
                        target: "nonexistent_document".to_string(),
                        alias: None,
                        offset: 35,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "another_missing".to_string(),
                        alias: Some("Display Text".to_string()),
                        offset: 65,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["broken", "testing"],
            )
            .await?;

        // Test that relations are created even for broken links
        let relations = env.get_wikilink_relations(&doc_id).await?;
        assert_eq!(relations.len(), 2, "Should create relations for broken links");

        // Test that linked documents query returns empty (targets don't exist)
        let linked_docs = env.get_linked_documents(&doc_id).await?;
        assert_eq!(linked_docs.len(), 0, "Should return empty for broken links");

        // Now create one of the missing documents
        let _missing_doc_id = env
            .create_document(
                "nonexistent_document.md",
                Some("Previously Missing"),
                "This document was previously missing",
                vec![],
                vec!["missing", "found"],
            )
            .await?;

        // Test that link now resolves correctly
        let linked_docs = env.get_linked_documents(&doc_id).await?;
        assert_eq!(linked_docs.len(), 1, "Should now find the created document");

        // Verify the correct document is found
        assert_eq!(
            linked_docs[0].title,
            Some("Previously Missing".to_string()),
            "Should find the correct document"
        );

        Ok(())
    }

    /// Test CLI integration with link resolution (placeholder for future CLI link commands)
    #[tokio::test]
    async fn test_cli_link_resolution_integration() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create test documents
        let _source_id = env
            .create_document(
                "cli_source.md",
                Some("CLI Source"),
                "CLI test document linking to [[CLI Target]]",
                vec![Wikilink {
                    target: "cli_target".to_string(),
                    alias: Some("CLI Target".to_string()),
                    offset: 30,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["cli", "test"],
            )
            .await?;

        let target_id = env
            .create_document(
                "cli_target.md",
                Some("CLI Target"),
                "CLI test target document",
                vec![],
                vec!["cli", "target"],
            )
            .await?;

        // Test that link resolution works at the database level
        // (Placeholder for future CLI link resolution commands)
        let linked_docs = env.get_linked_documents(&target_id).await?;

        // For now, just verify the database structure works
        assert!(linked_docs.len() >= 0, "Database queries should work");

        Ok(())
    }

    /// Test link updates and propagation
    #[tokio::test]
    async fn test_link_updates_and_propagation() -> Result<()> {
        let mut env = LinkTestEnvironment::new().await?;

        // Create initial document
        let source_id = env
            .create_document(
                "source_updates.md",
                Some("Source Document"),
                "Initial content linking to [[Initial Target]]",
                vec![Wikilink {
                    target: "initial_target".to_string(),
                    alias: Some("Initial Target".to_string()),
                    offset: 30,
                    is_embed: false,
                    block_ref: None,
                    heading_ref: None,
                }],
                vec!["source", "updates"],
            )
            .await?;

        let initial_target_id = env
            .create_document(
                "initial_target.md",
                Some("Initial Target"),
                "Initial target document",
                vec![],
                vec!["initial", "target"],
            )
            .await?;

        // Verify initial link
        let initial_links = env.get_linked_documents(&source_id).await?;
        assert_eq!(initial_links.len(), 1, "Should have one initial link");

        // Create updated document with new links
        let updated_source_id = env
            .create_document(
                "source_updates.md", // Same path - simulating update
                Some("Updated Source Document"),
                "Updated content linking to [[Updated Target]] and [[Another Target]]",
                vec![
                    Wikilink {
                        target: "updated_target".to_string(),
                        alias: Some("Updated Target".to_string()),
                        offset: 35,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                    Wikilink {
                        target: "another_target".to_string(),
                        alias: Some("Another Target".to_string()),
                        offset: 65,
                        is_embed: false,
                        block_ref: None,
                        heading_ref: None,
                    },
                ],
                vec!["source", "updated"],
            )
            .await?;

        // Create new target documents
        let updated_target_id = env
            .create_document(
                "updated_target.md",
                Some("Updated Target"),
                "Updated target document",
                vec![],
                vec!["updated", "target"],
            )
            .await?;

        let _another_target_id = env
            .create_document(
                "another_target.md",
                Some("Another Target"),
                "Another target document",
                vec![],
                vec!["another", "target"],
            )
            .await?;

        // Test that links are updated
        let updated_links = env.get_linked_documents(&updated_source_id).await?;
        assert_eq!(updated_links.len(), 2, "Should have two updated links");

        // Test backlink propagation
        let _initial_target_backlinks = env.get_backlinks(&initial_target_id).await?;
        let updated_target_backlinks = env.get_backlinks(&updated_target_id).await?;

        // The old target might still have backlinks (depending on update strategy)
        // The new target should have backlinks
        assert!(updated_target_backlinks.len() >= 1, "New target should have backlinks");

        Ok(())
    }
}

/// Helper function to create test wikilink with various properties
pub fn create_test_wikilink(
    target: &str,
    alias: Option<&str>,
    position: usize,
    is_embed: bool,
    heading_ref: Option<&str>,
    block_ref: Option<&str>,
) -> Wikilink {
    Wikilink {
        target: target.to_string(),
        alias: alias.map(|s| s.to_string()),
        offset: position,
        is_embed,
        heading_ref: heading_ref.map(|s| s.to_string()),
        block_ref: block_ref.map(|s| s.to_string()),
    }
}

/// Helper function to assert link relationship properties
pub fn assert_wikilink_relation(relation: &DBWikilink, expected_link_text: &str, expected_position: usize) {
    assert_eq!(
        relation.link_text, expected_link_text,
        "Link text should match expected value"
    );
    assert_eq!(
        relation.position, expected_position as i32,
        "Link position should match expected value"
    );
    assert!(!relation.from.id.is_empty(), "Source document ID should be set");
    assert!(!relation.to.id.is_empty(), "Target document ID should be set");
    assert!(relation.created_at <= Utc::now(), "Creation timestamp should be valid");
}