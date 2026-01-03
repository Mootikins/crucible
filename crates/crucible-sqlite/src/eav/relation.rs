//! RelationStorage implementation for SQLite

use crate::connection::SqlitePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::storage::eav_graph_traits::{Relation, RelationStorage};
use crucible_core::storage::StorageResult;
use rusqlite::params;
use serde_json::Value;
use uuid::Uuid;

/// SQLite implementation of RelationStorage
#[derive(Clone)]
pub struct SqliteRelationStorage {
    pool: SqlitePool,
}

impl SqliteRelationStorage {
    /// Create a new RelationStorage with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RelationStorage for SqliteRelationStorage {
    async fn store_relation(&self, relation: Relation) -> StorageResult<String> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let id = if relation.id.is_empty() {
                    format!("rel:{}", Uuid::new_v4())
                } else {
                    relation.id.clone()
                };

                let metadata_json = relation.metadata.to_string();
                let block_hash_blob = relation.block_hash.map(|h| h.to_vec());

                conn.execute(
                    r#"
                    INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                    "#,
                    params![
                        id,
                        relation.from_entity_id,
                        relation.to_entity_id,
                        relation.relation_type,
                        metadata_json,
                        relation.context,
                        relation.block_offset,
                        block_hash_blob,
                        relation.heading_occurrence,
                        relation.created_at.to_rfc3339(),
                    ],
                )?;

                Ok(id)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn batch_store_relations(&self, relations: &[Relation]) -> StorageResult<()> {
        if relations.is_empty() {
            return Ok(());
        }

        let pool = self.pool.clone();
        let relations = relations.to_vec();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let tx = conn.unchecked_transaction()?;

                {
                    let mut stmt = tx.prepare(
                        r#"
                        INSERT INTO relations (id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                        "#,
                    )?;

                    for relation in &relations {
                        let id = if relation.id.is_empty() {
                            format!("rel:{}", Uuid::new_v4())
                        } else {
                            relation.id.clone()
                        };

                        let metadata_json = relation.metadata.to_string();
                        let block_hash_blob = relation.block_hash.map(|h| h.to_vec());

                        stmt.execute(params![
                            id,
                            relation.from_entity_id,
                            relation.to_entity_id,
                            relation.relation_type,
                            metadata_json,
                            relation.context,
                            relation.block_offset,
                            block_hash_blob,
                            relation.heading_occurrence,
                            relation.created_at.to_rfc3339(),
                        ])?;
                    }
                }

                tx.commit()?;
                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_relation(&self, id: &str) -> StorageResult<Option<Relation>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at
                    FROM relations
                    WHERE id = ?1
                    "#,
                )?;

                let relation = stmt
                    .query_row([&id], row_to_relation)
                    .optional()?;

                Ok(relation)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_relations(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let relation_type = relation_type.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let relations: Vec<Relation> = if let Some(rt) = relation_type {
                    let mut stmt = conn.prepare(
                        r#"
                        SELECT id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at
                        FROM relations
                        WHERE from_entity_id = ?1 AND relation_type = ?2
                        "#,
                    )?;
                    let rows = stmt.query_map(params![entity_id, rt], row_to_relation)?;
                    rows.filter_map(Result::ok).collect()
                } else {
                    let mut stmt = conn.prepare(
                        r#"
                        SELECT id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at
                        FROM relations
                        WHERE from_entity_id = ?1
                        "#,
                    )?;
                    let rows = stmt.query_map([&entity_id], row_to_relation)?;
                    rows.filter_map(Result::ok).collect()
                };

                Ok(relations)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_backlinks(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let relation_type = relation_type.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let relations: Vec<Relation> = if let Some(rt) = relation_type {
                    let mut stmt = conn.prepare(
                        r#"
                        SELECT id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at
                        FROM relations
                        WHERE to_entity_id = ?1 AND relation_type = ?2
                        "#,
                    )?;
                    let rows = stmt.query_map(params![entity_id, rt], row_to_relation)?;
                    rows.filter_map(Result::ok).collect()
                } else {
                    let mut stmt = conn.prepare(
                        r#"
                        SELECT id, from_entity_id, to_entity_id, relation_type, metadata, context, block_offset, block_hash, heading_occurrence, created_at
                        FROM relations
                        WHERE to_entity_id = ?1
                        "#,
                    )?;
                    let rows = stmt.query_map([&entity_id], row_to_relation)?;
                    rows.filter_map(Result::ok).collect()
                };

                Ok(relations)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let deleted = conn.execute(
                    "DELETE FROM relations WHERE from_entity_id = ?1",
                    [&entity_id],
                )?;
                Ok(deleted)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_relation(&self, id: &str) -> StorageResult<()> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute("DELETE FROM relations WHERE id = ?1", [&id])?;
                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn find_block_by_hash(
        &self,
        entity_id: &str,
        hash: &[u8; 32],
    ) -> StorageResult<Option<String>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let hash_hex = hex::encode(hash);

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Look for blocks with matching content hash
                let block_id: Option<String> = conn
                    .query_row(
                        r#"
                        SELECT id FROM blocks
                        WHERE entity_id = ?1 AND content_hash = ?2
                        LIMIT 1
                        "#,
                        params![entity_id, hash_hex],
                        |row| row.get(0),
                    )
                    .optional()?;

                Ok(block_id)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }
}

/// Convert a database row to a Relation
fn row_to_relation(row: &rusqlite::Row) -> rusqlite::Result<Relation> {
    let id: String = row.get(0)?;
    let from_entity_id: String = row.get(1)?;
    let to_entity_id: Option<String> = row.get(2)?;
    let relation_type: String = row.get(3)?;
    let metadata_json: String = row.get(4)?;
    let context: Option<String> = row.get(5)?;
    let block_offset: Option<u32> = row.get(6)?;
    let block_hash_blob: Option<Vec<u8>> = row.get(7)?;
    let heading_occurrence: Option<u32> = row.get(8)?;
    let created_at: String = row.get(9)?;

    let metadata: Value = serde_json::from_str(&metadata_json).unwrap_or(Value::Null);

    let block_hash: Option<[u8; 32]> = block_hash_blob.and_then(|v| {
        if v.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&v);
            Some(arr)
        } else {
            None
        }
    });

    Ok(Relation {
        id,
        from_entity_id,
        to_entity_id,
        relation_type,
        metadata,
        context,
        block_offset,
        block_hash,
        heading_occurrence,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

/// Extension trait for optional query results
trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav::SqliteEntityStorage;
    use crucible_core::storage::eav_graph_traits::{Entity, EntityStorage, EntityType};

    async fn setup() -> (SqlitePool, SqliteRelationStorage) {
        let pool = SqlitePool::memory().unwrap();
        let entity_storage = SqliteEntityStorage::new(pool.clone());
        let relation_storage = SqliteRelationStorage::new(pool.clone());

        // Create test entities
        entity_storage
            .store_entity(Entity::new("note:source".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:target1".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:target2".to_string(), EntityType::Note))
            .await
            .unwrap();

        (pool, relation_storage)
    }

    #[tokio::test]
    async fn test_relation_crud() {
        let (_pool, storage) = setup().await;

        // Create
        let relation = Relation::wikilink("note:source", "note:target1")
            .with_context("See [[Target]] for details");

        let id = storage.store_relation(relation).await.unwrap();
        assert!(id.starts_with("rel:"));

        // Read
        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.from_entity_id, "note:source");
        assert_eq!(retrieved.to_entity_id, Some("note:target1".to_string()));
        assert_eq!(retrieved.relation_type, "wikilink");

        // Delete
        storage.delete_relation(&id).await.unwrap();
        let deleted = storage.get_relation(&id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_batch_relations() {
        let (_pool, storage) = setup().await;

        let relations = vec![
            Relation::wikilink("note:source", "note:target1"),
            Relation::wikilink("note:source", "note:target2"),
            Relation::embed("note:source", "note:target1"),
        ];

        storage.batch_store_relations(&relations).await.unwrap();

        // Get by type
        let wikilinks = storage
            .get_relations("note:source", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(wikilinks.len(), 2);

        let embeds = storage
            .get_relations("note:source", Some("embed"))
            .await
            .unwrap();
        assert_eq!(embeds.len(), 1);

        // Get all
        let all = storage.get_relations("note:source", None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_backlinks() {
        let (_pool, storage) = setup().await;

        let relations = vec![
            Relation::wikilink("note:source", "note:target1"),
            Relation::wikilink("note:target2", "note:target1"),
        ];

        storage.batch_store_relations(&relations).await.unwrap();

        let backlinks = storage
            .get_backlinks("note:target1", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(backlinks.len(), 2);
    }

    #[tokio::test]
    async fn test_block_link() {
        let (_pool, storage) = setup().await;

        let hash = [42u8; 32];
        let relation = Relation::wikilink("note:source", "note:target1")
            .with_block_link(5, hash, Some(2));

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.block_offset, Some(5));
        assert_eq!(retrieved.block_hash, Some(hash));
        assert_eq!(retrieved.heading_occurrence, Some(2));
    }

    #[tokio::test]
    async fn test_get_nonexistent_relation() {
        let (_pool, storage) = setup().await;

        let result = storage.get_relation("rel:does-not-exist").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_relation_with_unicode_context() {
        let (_pool, storage) = setup().await;

        let relation = Relation::wikilink("note:source", "note:target1")
            .with_context("日本語リンク [[Target]] и русский текст 🔗");

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(
            retrieved.context,
            Some("日本語リンク [[Target]] и русский текст 🔗".to_string())
        );
    }

    #[tokio::test]
    async fn test_relation_with_complex_metadata() {
        let (_pool, storage) = setup().await;

        let relation = Relation::wikilink("note:source", "note:target1").with_metadata(
            serde_json::json!({
                "alias": "Target Alias",
                "heading": "## Section",
                "nested": {"deep": {"value": 42}},
                "array": [1, "two", null]
            }),
        );

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.metadata["alias"], "Target Alias");
        assert_eq!(retrieved.metadata["nested"]["deep"]["value"], 42);
    }

    #[tokio::test]
    async fn test_empty_batch_store() {
        let (_pool, storage) = setup().await;

        // Empty batch should succeed without error
        let result = storage.batch_store_relations(&[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delete_relations_returns_count() {
        let (pool, storage) = setup().await;

        // Need additional entity for this test
        let entity_storage = SqliteEntityStorage::new(pool);
        entity_storage
            .store_entity(Entity::new("note:delete_test".to_string(), EntityType::Note))
            .await
            .unwrap();

        let relations = vec![
            Relation::wikilink("note:delete_test", "note:target1"),
            Relation::wikilink("note:delete_test", "note:target2"),
            Relation::embed("note:delete_test", "note:target1"),
        ];
        storage.batch_store_relations(&relations).await.unwrap();

        let deleted = storage.delete_relations("note:delete_test").await.unwrap();
        assert_eq!(deleted, 3);

        // Verify they're gone
        let remaining = storage
            .get_relations("note:delete_test", None)
            .await
            .unwrap();
        assert!(remaining.is_empty());
    }

    #[tokio::test]
    async fn test_relation_with_unresolved_target() {
        let (_pool, storage) = setup().await;

        // Relation with None target (unresolved wikilink)
        let relation = Relation::new("note:source", None, "wikilink")
            .with_context("[[Ambiguous Note]] - could not resolve");

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert!(retrieved.to_entity_id.is_none());
    }
}
