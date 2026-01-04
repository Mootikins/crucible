//! Knowledge Repository implementation for SQLite backend
//!
//! Provides high-level operations for accessing knowledge stored in the kiln
//! via the unified NoteStore interface.

use async_trait::async_trait;
use crucible_core::parser::{Frontmatter, FrontmatterFormat, ParsedNote, Wikilink};
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_core::types::{DocumentId, SearchResult};
use crucible_core::{CrucibleError, Result as CrucibleResult};
use std::sync::Arc;

use crate::note_store::SqliteNoteStore;

/// SQLite-backed implementation of KnowledgeRepository.
///
/// Uses the SqliteNoteStore to provide knowledge access operations.
///
/// ## Example
///
/// ```ignore
/// use crucible_sqlite::{SqliteConfig, SqlitePool, create_note_store};
/// use crucible_sqlite::repository::SqliteKnowledgeRepository;
///
/// let pool = SqlitePool::new(SqliteConfig::new("./crucible.db"))?;
/// let store = create_note_store(pool).await?;
/// let repo = SqliteKnowledgeRepository::new(Arc::new(store));
///
/// let note = repo.get_note_by_name("Index").await?;
/// ```
pub struct SqliteKnowledgeRepository {
    store: Arc<SqliteNoteStore>,
}

impl SqliteKnowledgeRepository {
    /// Create a new repository backed by a SqliteNoteStore
    pub fn new(store: Arc<SqliteNoteStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl KnowledgeRepository for SqliteKnowledgeRepository {
    async fn get_note_by_name(&self, name: &str) -> CrucibleResult<Option<ParsedNote>> {
        use crucible_core::storage::NoteStore;

        // Get all notes and find one matching by path or title
        let notes =
            self.store.list().await.map_err(|e| {
                CrucibleError::DatabaseError(format!("Failed to list notes: {}", e))
            })?;

        // Find note where path contains name or title matches
        let name_lower = name.to_lowercase();
        let matching = notes.into_iter().find(|note| {
            note.path.to_lowercase().contains(&name_lower)
                || note.title.to_lowercase().contains(&name_lower)
        });

        match matching {
            Some(record) => {
                // Create a minimal ParsedNote from the stored data
                let mut note = ParsedNote::new(std::path::PathBuf::from(&record.path));

                // Build YAML frontmatter with title and tags
                let mut yaml_parts = vec![format!("title: \"{}\"", record.title)];
                if !record.tags.is_empty() {
                    let tags_yaml = record
                        .tags
                        .iter()
                        .map(|t| format!("  - {}", t))
                        .collect::<Vec<_>>()
                        .join("\n");
                    yaml_parts.push(format!("tags:\n{}", tags_yaml));
                }
                let raw_yaml = yaml_parts.join("\n");
                note.frontmatter = Some(Frontmatter::new(raw_yaml, FrontmatterFormat::Yaml));

                // Add links_to as wikilinks
                for (i, link_path) in record.links_to.iter().enumerate() {
                    note.content.wikilinks.push(Wikilink::new(link_path, i));
                }

                Ok(Some(note))
            }
            None => Ok(None),
        }
    }

    async fn list_notes(&self, path: Option<&str>) -> CrucibleResult<Vec<NoteInfo>> {
        use crucible_core::storage::NoteStore;

        let notes =
            self.store.list().await.map_err(|e| {
                CrucibleError::DatabaseError(format!("Failed to list notes: {}", e))
            })?;

        let filtered: Vec<NoteInfo> = notes
            .into_iter()
            .filter(|note| {
                // If path filter specified, check if note path contains it
                path.is_none_or(|p| note.path.contains(p))
            })
            .map(|record| {
                // Extract name from path (filename without extension)
                let name = std::path::Path::new(&record.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&record.path)
                    .to_string();

                NoteInfo {
                    name,
                    path: record.path,
                    title: Some(record.title),
                    tags: record.tags,
                    created_at: None,
                    updated_at: Some(record.updated_at),
                }
            })
            .collect();

        Ok(filtered)
    }

    async fn search_vectors(&self, vector: Vec<f32>) -> CrucibleResult<Vec<SearchResult>> {
        use crucible_core::storage::NoteStore;

        // Use the NoteStore's search capability
        let results = self
            .store
            .search(&vector, 10, None)
            .await
            .map_err(|e| CrucibleError::DatabaseError(format!("Search failed: {}", e)))?;

        // Convert NoteStore SearchResults to KnowledgeRepository SearchResults
        let converted: Vec<SearchResult> = results
            .into_iter()
            .map(|r| SearchResult {
                document_id: DocumentId(r.note.path),
                score: r.score as f64,
                highlights: None,
                snippet: None,
            })
            .collect();

        Ok(converted)
    }
}

/// Create a KnowledgeRepository backed by SQLite.
///
/// This is a convenience factory function.
pub fn create_knowledge_repository(store: Arc<SqliteNoteStore>) -> Arc<dyn KnowledgeRepository> {
    Arc::new(SqliteKnowledgeRepository::new(store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SqliteConfig;
    use crate::connection::SqlitePool;
    use crate::note_store::create_note_store;
    use chrono::Utc;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::note_store::NoteRecord;
    use crucible_core::storage::NoteStore;

    async fn setup_test_repo() -> (SqliteKnowledgeRepository, Arc<SqliteNoteStore>) {
        let config = SqliteConfig::memory();
        let pool = SqlitePool::new(config).unwrap();
        let store = create_note_store(pool).await.unwrap();
        let store_arc = Arc::new(store);

        // Add test notes
        NoteStore::upsert(
            store_arc.as_ref(),
            NoteRecord {
                path: "notes/index.md".to_string(),
                content_hash: BlockHash::zero(),
                embedding: Some(vec![1.0, 0.0, 0.0]),
                title: "Index".to_string(),
                tags: vec!["home".to_string(), "nav".to_string()],
                links_to: vec!["notes/rust.md".to_string(), "notes/python.md".to_string()],
                properties: Default::default(),
                updated_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        NoteStore::upsert(
            store_arc.as_ref(),
            NoteRecord {
                path: "notes/rust.md".to_string(),
                content_hash: BlockHash::zero(),
                embedding: Some(vec![0.0, 1.0, 0.0]),
                title: "Rust Programming".to_string(),
                tags: vec!["programming".to_string(), "rust".to_string()],
                links_to: vec![],
                properties: Default::default(),
                updated_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        NoteStore::upsert(
            store_arc.as_ref(),
            NoteRecord {
                path: "guides/python.md".to_string(),
                content_hash: BlockHash::zero(),
                embedding: Some(vec![0.0, 0.0, 1.0]),
                title: "Python Guide".to_string(),
                tags: vec!["programming".to_string(), "python".to_string()],
                links_to: vec![],
                properties: Default::default(),
                updated_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let repo = SqliteKnowledgeRepository::new(Arc::clone(&store_arc));
        (repo, store_arc)
    }

    #[tokio::test]
    async fn test_get_note_by_name_exact_title() {
        let (repo, _) = setup_test_repo().await;

        let note = repo.get_note_by_name("Index").await.unwrap();
        assert!(note.is_some());
        let note = note.unwrap();
        assert_eq!(note.title(), "Index");
    }

    #[tokio::test]
    async fn test_get_note_by_name_partial_path() {
        let (repo, _) = setup_test_repo().await;

        let note = repo.get_note_by_name("rust").await.unwrap();
        assert!(note.is_some());
        let note = note.unwrap();
        assert_eq!(note.title(), "Rust Programming");
    }

    #[tokio::test]
    async fn test_get_note_by_name_case_insensitive() {
        let (repo, _) = setup_test_repo().await;

        let note = repo.get_note_by_name("PYTHON").await.unwrap();
        assert!(note.is_some());
    }

    #[tokio::test]
    async fn test_get_note_by_name_not_found() {
        let (repo, _) = setup_test_repo().await;

        let note = repo.get_note_by_name("nonexistent").await.unwrap();
        assert!(note.is_none());
    }

    #[tokio::test]
    async fn test_get_note_preserves_wikilinks() {
        let (repo, _) = setup_test_repo().await;

        let note = repo.get_note_by_name("Index").await.unwrap().unwrap();
        assert_eq!(note.content.wikilinks.len(), 2);
        assert!(note
            .content
            .wikilinks
            .iter()
            .any(|w| w.target == "notes/rust.md"));
    }

    #[tokio::test]
    async fn test_list_notes_all() {
        let (repo, _) = setup_test_repo().await;

        let notes = repo.list_notes(None).await.unwrap();
        assert_eq!(notes.len(), 3);
    }

    #[tokio::test]
    async fn test_list_notes_filtered_by_path() {
        let (repo, _) = setup_test_repo().await;

        let notes = repo.list_notes(Some("notes/")).await.unwrap();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| n.path.starts_with("notes/")));
    }

    #[tokio::test]
    async fn test_list_notes_preserves_info() {
        let (repo, _) = setup_test_repo().await;

        let notes = repo.list_notes(None).await.unwrap();
        let rust_note = notes.iter().find(|n| n.path == "notes/rust.md").unwrap();

        assert_eq!(rust_note.name, "rust");
        assert_eq!(rust_note.title, Some("Rust Programming".to_string()));
        assert!(rust_note.tags.contains(&"programming".to_string()));
        assert!(rust_note.tags.contains(&"rust".to_string()));
    }

    #[tokio::test]
    async fn test_search_vectors() {
        let (repo, _) = setup_test_repo().await;

        // Search for vector similar to rust.md's embedding
        let results = repo.search_vectors(vec![0.0, 1.0, 0.0]).await.unwrap();
        assert!(!results.is_empty());

        // First result should be rust.md (exact match)
        assert_eq!(results[0].document_id.0, "notes/rust.md");
    }

    #[tokio::test]
    async fn test_search_vectors_scores() {
        let (repo, _) = setup_test_repo().await;

        let results = repo.search_vectors(vec![1.0, 0.0, 0.0]).await.unwrap();
        assert!(!results.is_empty());

        // Scores should be in descending order (higher is more similar)
        for i in 0..results.len().saturating_sub(1) {
            assert!(
                results[i].score >= results[i + 1].score,
                "Results should be sorted by score descending"
            );
        }
    }
}
