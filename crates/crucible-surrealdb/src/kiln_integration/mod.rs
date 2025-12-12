//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements the bridge between ParsedNote structures and the database schema.
//! Includes comprehensive vector embedding support for semantic search and processing.

mod document_storage;
mod embeddings;
mod relations;
mod repository;
mod semantic_search;
mod types;
mod utils;

// Re-export for crate-internal use
pub(crate) use relations::parse_entity_record_id;

// Re-export for tests only
#[cfg(test)]
pub(crate) use relations::{
    fetch_embed_relation_pairs, find_entity_id_by_title, query_embedding_sources_for_entity,
};
#[cfg(test)]
pub(crate) use utils::record_body;

// Public API re-exports
pub use document_storage::{retrieve_parsed_document, store_parsed_document};

pub use embeddings::{
    clear_all_embeddings, clear_all_embeddings_and_recreate_index, clear_document_embeddings,
    delete_document_chunks, ensure_embedding_index, ensure_embedding_index_from_existing,
    get_all_document_embeddings, get_database_stats, get_document_chunk_hashes,
    get_document_embeddings, get_embedding_by_content_hash, get_embedding_index_metadata,
    store_document_embedding, store_embedding, store_embedding_with_chunk_id,
    store_embeddings_batch,
};

pub use relations::{
    create_embed_relationships, create_wikilink_edges, get_documents_by_tag, get_embed_metadata,
    get_embed_relations, get_embed_with_metadata, get_embedded_documents,
    get_embedded_documents_by_type, get_embedding_documents, get_linked_documents,
    get_wikilink_relations, get_wikilinked_documents,
};

pub use semantic_search::{semantic_search, semantic_search_with_reranking};

pub use types::{
    CachedEmbedding, EmbedMetadata, EmbedRelation, EmbeddingData, EmbeddingIndexMetadata,
    LinkRelation,
};

pub use utils::{generate_document_id, normalize_document_id};

// Schema initialization
use crate::eav_graph::apply_eav_graph_schema;
use crate::SurrealClient;
use anyhow::Result;
use tracing::info;

/// Initialize the kiln schema in the database
pub async fn initialize_kiln_schema(client: &SurrealClient) -> Result<()> {
    apply_eav_graph_schema(client).await?;
    info!("Kiln schema initialized using EAV+Graph definitions");

    // Note: MTREE index creation is deferred to first embedding storage or explicit call
    // to avoid blocking startup on large databases. The index will be created lazily
    // when store_embeddings_batch is called, or you can call ensure_embedding_index_from_existing()
    // explicitly if you want to pre-create the index.

    Ok(())
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;
    use crucible_core::parser::Wikilink;
    use crucible_core::types::{ParsedNote, Tag};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_store_embedding_with_graph_relations() {
        use crate::types::SurrealDbConfig;
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        let _ = initialize_kiln_schema(&client).await;

        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/Projects/test_file.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "Test content for embedding".to_string();
        doc.content_hash = "test_hash_123".to_string();

        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        println!("Stored note with ID: {}", note_id);

        let vector: Vec<f32> = (0..768).map(|i| i as f32 / 768.0).collect();

        let chunk_id = store_embedding(
            &client,
            &note_id,
            vector.clone(),
            "test-model",
            1000,
            0,
            None,
            None,
        )
        .await
        .unwrap();

        println!("Stored embedding with ID: {}", chunk_id);

        assert!(chunk_id.starts_with("embeddings:"));
        assert!(chunk_id.contains("_chunk_0"));

        let traversal_sql = format!(
            "SELECT out FROM entities:⟨{}⟩->has_embedding",
            note_id.strip_prefix("entities:").unwrap_or(&note_id)
        );
        println!("Executing traversal query: {}", traversal_sql);

        let result = client.query(&traversal_sql, &[]).await.unwrap();

        println!("Graph traversal returned {} records", result.records.len());

        assert_eq!(
            result.records.len(),
            1,
            "Should retrieve one embedding via graph traversal"
        );

        let embedding_record = &result.records[0];
        assert!(
            embedding_record.data.contains_key("out"),
            "Should have 'out' field with embedding ID"
        );

        println!("Graph relations test passed!");
    }

    #[tokio::test]
    async fn test_multiple_chunks_with_graph_relations() {
        use crate::types::SurrealDbConfig;
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        let _ = initialize_kiln_schema(&client).await;

        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/Projects/large_file.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "Large test content".to_string();
        doc.content_hash = "test_hash_456".to_string();

        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        let vector: Vec<f32> = (0..768).map(|i| i as f32 / 768.0).collect();

        for chunk_pos in 0..3 {
            let chunk_id = store_embedding(
                &client,
                &note_id,
                vector.clone(),
                "test-model",
                1000,
                chunk_pos,
                None,
                None,
            )
            .await
            .unwrap();

            println!("Stored chunk {}: {}", chunk_pos, chunk_id);
        }

        let traversal_sql = format!(
            "SELECT out FROM entities:⟨{}⟩->has_embedding",
            note_id.strip_prefix("entities:").unwrap_or(&note_id)
        );
        let result = client.query(&traversal_sql, &[]).await.unwrap();

        println!(
            "Retrieved {} embeddings via graph traversal",
            result.records.len()
        );

        assert_eq!(
            result.records.len(),
            3,
            "Should retrieve all three embedding chunks"
        );

        println!("Multiple chunks graph relations test passed!");
    }

    #[tokio::test]
    async fn tag_associations_create_hierarchy() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();

        let kiln_root = PathBuf::from("/vault");
        let mut doc = ParsedNote::new(kiln_root.join("projects/sample.md"));
        doc.content_hash = "tag-hash-1".into();
        doc.tags.push(Tag::new("project/crucible", 0));
        doc.tags.push(Tag::new("design/ui", 0));

        let _doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        let tags = client.query("SELECT * FROM tags", &[]).await.unwrap();
        assert_eq!(tags.records.len(), 4);

        let entity_tags = client
            .query("SELECT * FROM entity_tags", &[])
            .await
            .unwrap();
        assert_eq!(entity_tags.records.len(), 2);

        let docs = get_documents_by_tag(&client, "project/crucible")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test]
    async fn wikilink_edges_create_relations_and_placeholders() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let kiln_root = PathBuf::from("/vault");

        let mut doc = ParsedNote::new(kiln_root.join("projects/source.md"));
        doc.content_hash = "wikihash".into();
        doc.content.plain_text = "Scenario with wikilinks".into();
        doc.wikilinks.push(Wikilink::new("TargetNote", 5));
        doc.wikilinks.push(Wikilink::new("../Shared/OtherDoc", 15));

        let mut target_doc = ParsedNote::new(kiln_root.join("projects/TargetNote.md"));
        target_doc.content_hash = "targethash".into();
        target_doc.content.plain_text = "Target note".into();
        store_parsed_document(&client, &target_doc, &kiln_root)
            .await
            .unwrap();

        let doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        create_wikilink_edges(&client, &doc_id, &doc, &kiln_root)
            .await
            .unwrap();

        let relations = client
            .query(
                "SELECT relation_type, out, in FROM relations ORDER BY relation_type",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(relations.records.len(), 4);

        let targets: Vec<String> = relations
            .records
            .iter()
            .filter_map(|record| record.data.get("out").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        assert!(targets.iter().any(|t| t.contains("projects/TargetNote.md")));
        assert!(targets.iter().any(|t| t.contains("Shared/OtherDoc.md")));

        let relation_types: Vec<String> = relations
            .records
            .iter()
            .filter_map(|record| record.data.get("relation_type").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        assert!(relation_types.iter().any(|t| t == "wikilink"));
        assert!(relation_types.iter().any(|t| t == "wikilink_backlink"));

        let linked = get_linked_documents(&client, &doc_id).await.unwrap();
        assert_eq!(linked.len(), 2);

        let relation_list = get_wikilink_relations(&client, &doc_id).await.unwrap();
        assert_eq!(relation_list.len(), 2);

        let placeholder = client
            .query(
                "SELECT data FROM type::thing('entities', 'note:Shared/OtherDoc.md')",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(placeholder.records.len(), 1);
    }

    #[tokio::test]
    async fn embed_relationships_create_relations_and_backlinks() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let kiln_root = PathBuf::from("/vault");

        let mut doc = ParsedNote::new(kiln_root.join("media/source.md"));
        doc.content_hash = "embedhash".into();
        doc.content.plain_text = "Doc with embeds".into();
        doc.wikilinks.push(Wikilink::embed("Assets/Diagram", 3));

        let mut target_doc = ParsedNote::new(kiln_root.join("media/Assets/Diagram.md"));
        target_doc.content_hash = "diagramhash".into();
        target_doc.content.plain_text = "Diagram content".into();
        store_parsed_document(&client, &target_doc, &kiln_root)
            .await
            .unwrap();

        let doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        create_embed_relationships(&client, &doc_id, &doc, &kiln_root)
            .await
            .unwrap();

        let relations = client
            .query("SELECT relation_type, out, in FROM relations", &[])
            .await
            .unwrap();
        assert_eq!(relations.records.len(), 2);
        let mut has_forward = false;
        let mut has_backlink = false;
        for record in &relations.records {
            match record.data.get("relation_type").and_then(|v| v.as_str()) {
                Some("embed") => {
                    has_forward = true;
                    assert!(record
                        .data
                        .get("out")
                        .and_then(utils::record_ref_to_string)
                        .map(|s| s.contains("Assets/Diagram.md"))
                        .unwrap_or(false));
                }
                Some("embed_backlink") => {
                    has_backlink = true;
                    assert!(record
                        .data
                        .get("out")
                        .and_then(utils::record_ref_to_string)
                        .map(|s| s.contains("media/source.md"))
                        .unwrap_or(false));
                }
                _ => {}
            }
        }
        assert!(has_forward);
        assert!(has_backlink);
        let embed_target_ids: Vec<String> = relations
            .records
            .iter()
            .filter(|record| {
                record.data.get("relation_type").and_then(|v| v.as_str()) == Some("embed")
            })
            .filter_map(|record| record.data.get("out").and_then(utils::record_ref_to_string))
            .collect();
        assert!(embed_target_ids
            .iter()
            .any(|id| id.contains("Assets/Diagram.md")));

        let embed_relations = get_embed_relations(&client, &doc_id).await.unwrap();
        assert_eq!(embed_relations.len(), 1);

        let entity = find_entity_id_by_title(&client, "Diagram")
            .await
            .unwrap()
            .expect("entity for Diagram should exist");
        assert!(entity.id.contains("Assets/Diagram"));

        let embed_pairs = fetch_embed_relation_pairs(&client).await.unwrap();
        assert_eq!(embed_pairs.len(), 1);
        assert!(
            embed_pairs[0].1.contains("Assets/Diagram.md"),
            "pair target {}",
            embed_pairs[0].1
        );
        assert_eq!(
            record_body(&embed_pairs[0].1),
            entity.id,
            "normalized target {} expected {}",
            record_body(&embed_pairs[0].1),
            entity.id
        );

        let backlink_sources = query_embedding_sources_for_entity(&client, &entity)
            .await
            .unwrap();
        assert_eq!(backlink_sources.len(), 1);

        let filtered_docs =
            get_embedded_documents_by_type(&client, &doc_id, &embed_relations[0].embed_type)
                .await
                .unwrap();
        assert_eq!(filtered_docs.len(), 1);

        let embedded_docs = get_embedded_documents(&client, &doc_id).await.unwrap();
        assert_eq!(embedded_docs.len(), 1);

        let metadata = get_embed_metadata(&client, &doc_id).await.unwrap();
        assert_eq!(metadata.len(), 1);

        let embedding_docs = get_embedding_documents(&client, "Diagram").await.unwrap();
        assert_eq!(embedding_docs.len(), 1);
    }

    #[tokio::test]
    async fn test_semantic_search_with_knn() {
        use crate::types::SurrealDbConfig;
        use std::sync::Arc;

        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        initialize_kiln_schema(&client).await.unwrap();

        ensure_embedding_index(&client, 768).await.unwrap();

        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/notes/test_note.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "This is a test note about Rust programming".to_string();
        doc.content_hash = "semantic_test_hash".to_string();

        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        let vector_768: Vec<f32> = (0..768).map(|i| (i as f32 / 768.0).sin()).collect();
        store_embedding(
            &client,
            &note_id,
            vector_768.clone(),
            "nomic-embed-text",
            768,
            0,
            None,
            Some("This is a test note about Rust programming"),
        )
        .await
        .unwrap();

        let mock_provider =
            Arc::new(crucible_llm::embeddings::mock::MockEmbeddingProvider::with_dimensions(768));

        let results = semantic_search(&client, "Rust programming", 5, mock_provider)
            .await
            .unwrap();

        assert!(!results.is_empty(), "Should find at least one result");

        let found = results
            .iter()
            .any(|(id, _)| id.contains("test_note.md") || id.contains("note:"));
        assert!(found, "Should find our test note in results: {:?}", results);

        println!("Semantic search with KNN test passed!");
    }

    #[tokio::test]
    async fn test_dynamic_index_recreation() {
        use crate::types::SurrealDbConfig;
        use std::sync::Arc;

        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        initialize_kiln_schema(&client).await.unwrap();

        ensure_embedding_index(&client, 384).await.unwrap();

        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/notes/test_384.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "384 dimension note".to_string();
        doc.content_hash = "hash_384".to_string();

        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        let vector_384: Vec<f32> = (0..384).map(|i| (i as f32 / 384.0).sin()).collect();
        store_embedding(
            &client,
            &note_id,
            vector_384.clone(),
            "test-model-384",
            384,
            0,
            None,
            Some("384 dimension note"),
        )
        .await
        .unwrap();

        let mock_provider_384 =
            Arc::new(crucible_llm::embeddings::mock::MockEmbeddingProvider::with_dimensions(384));
        let results = semantic_search(&client, "dimension test", 5, mock_provider_384)
            .await
            .expect("Should find results with 384-dim index");
        assert!(!results.is_empty(), "Should find 384-dim note");
        println!("Search with 384-dim index works");

        clear_all_embeddings_and_recreate_index(&client, 768)
            .await
            .unwrap();

        let doc_path_768 = PathBuf::from("/test/kiln/notes/test_768.md");
        let mut doc_768 = ParsedNote::new(doc_path_768.clone());
        doc_768.content.plain_text = "768 dimension note".to_string();
        doc_768.content_hash = "hash_768".to_string();

        let note_id_768 = store_parsed_document(&client, &doc_768, &kiln_root)
            .await
            .unwrap();

        let vector_768: Vec<f32> = (0..768).map(|i| (i as f32 / 768.0).sin()).collect();
        store_embedding(
            &client,
            &note_id_768,
            vector_768.clone(),
            "test-model-768",
            768,
            0,
            None,
            Some("768 dimension note"),
        )
        .await
        .unwrap();

        let mock_provider_768 =
            Arc::new(crucible_llm::embeddings::mock::MockEmbeddingProvider::with_dimensions(768));
        let results = semantic_search(&client, "dimension test", 5, mock_provider_768)
            .await
            .expect("Should find results with 768-dim index");

        assert!(
            !results.is_empty(),
            "Should find 768-dim note after index recreation"
        );

        let found = results
            .iter()
            .any(|(id, _)| id.contains("test_768.md") || id.contains("note:"));
        assert!(found, "Should find the 768-dim test note: {:?}", results);

        println!("Dynamic index recreation test passed!");
    }
}
