//! BlockStorage implementation for SQLite

use crate::connection::SqlitePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::storage::eav_graph_traits::{Block, BlockStorage};
use crucible_core::storage::StorageResult;
use rusqlite::params;

/// SQLite implementation of BlockStorage
#[derive(Clone)]
pub struct SqliteBlockStorage {
    pool: SqlitePool,
}

impl SqliteBlockStorage {
    /// Create a new BlockStorage with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BlockStorage for SqliteBlockStorage {
    async fn store_block(&self, block: Block) -> StorageResult<String> {
        let pool = self.pool.clone();
        let id = block.id.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute(
                    r#"
                    INSERT INTO blocks (id, entity_id, block_index, block_type, content, content_hash, parent_block_id, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                    ON CONFLICT(id) DO UPDATE SET
                        entity_id = excluded.entity_id,
                        block_index = excluded.block_index,
                        block_type = excluded.block_type,
                        content = excluded.content,
                        content_hash = excluded.content_hash,
                        parent_block_id = excluded.parent_block_id,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        block.id,
                        block.entity_id,
                        block.position,
                        block.block_type,
                        block.content,
                        block.content_hash,
                        block.parent_block_id,
                        block.created_at.to_rfc3339(),
                        block.updated_at.to_rfc3339(),
                    ],
                )?;

                Ok(id)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_block(&self, id: &str) -> StorageResult<Option<Block>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, entity_id, block_index, block_type, content, content_hash, parent_block_id, created_at, updated_at
                    FROM blocks
                    WHERE id = ?1
                    "#,
                )?;

                let block = stmt.query_row([&id], row_to_block).optional()?;

                Ok(block)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_blocks(&self, entity_id: &str) -> StorageResult<Vec<Block>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, entity_id, block_index, block_type, content, content_hash, parent_block_id, created_at, updated_at
                    FROM blocks
                    WHERE entity_id = ?1
                    ORDER BY block_index
                    "#,
                )?;

                let blocks = stmt
                    .query_map([&entity_id], row_to_block)?
                    .filter_map(Result::ok)
                    .collect();

                Ok(blocks)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_child_blocks(&self, parent_block_id: &str) -> StorageResult<Vec<Block>> {
        let pool = self.pool.clone();
        let parent_block_id = parent_block_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, entity_id, block_index, block_type, content, content_hash, parent_block_id, created_at, updated_at
                    FROM blocks
                    WHERE parent_block_id = ?1
                    ORDER BY block_index
                    "#,
                )?;

                let blocks = stmt
                    .query_map([&parent_block_id], row_to_block)?
                    .filter_map(Result::ok)
                    .collect();

                Ok(blocks)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn update_block(&self, id: &str, block: Block) -> StorageResult<()> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let rows = conn.execute(
                    r#"
                    UPDATE blocks SET
                        entity_id = ?2,
                        block_index = ?3,
                        block_type = ?4,
                        content = ?5,
                        content_hash = ?6,
                        parent_block_id = ?7,
                        updated_at = ?8
                    WHERE id = ?1
                    "#,
                    params![
                        id,
                        block.entity_id,
                        block.position,
                        block.block_type,
                        block.content,
                        block.content_hash,
                        block.parent_block_id,
                        block.updated_at.to_rfc3339(),
                    ],
                )?;

                if rows == 0 {
                    return Err(crate::error::SqliteError::NotFound(format!(
                        "Block {} does not exist",
                        id
                    )));
                }

                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_block(&self, id: &str, recursive: bool) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                if recursive {
                    // Delete block and all children (cascades via FK)
                    // First count all blocks that will be deleted
                    let count: usize = conn.query_row(
                        r#"
                        WITH RECURSIVE descendants(id) AS (
                            SELECT id FROM blocks WHERE id = ?1
                            UNION ALL
                            SELECT b.id FROM blocks b
                            JOIN descendants d ON b.parent_block_id = d.id
                        )
                        SELECT COUNT(*) FROM descendants
                        "#,
                        [&id],
                        |row| row.get(0),
                    )?;

                    conn.execute("DELETE FROM blocks WHERE id = ?1", [&id])?;
                    Ok(count)
                } else {
                    // Delete just this block
                    let deleted = conn.execute("DELETE FROM blocks WHERE id = ?1", [&id])?;
                    Ok(deleted)
                }
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_blocks(&self, entity_id: &str) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Count first because CASCADE deletes aren't counted by execute()
                let count: usize = conn.query_row(
                    "SELECT COUNT(*) FROM blocks WHERE entity_id = ?1",
                    [&entity_id],
                    |row| row.get(0),
                )?;

                conn.execute("DELETE FROM blocks WHERE entity_id = ?1", [&entity_id])?;
                Ok(count)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }
}

/// Convert a database row to a Block
fn row_to_block(row: &rusqlite::Row) -> rusqlite::Result<Block> {
    let id: String = row.get(0)?;
    let entity_id: String = row.get(1)?;
    let position: i32 = row.get(2)?;
    let block_type: String = row.get(3)?;
    let content: String = row.get(4)?;
    let content_hash: Option<String> = row.get(5)?;
    let parent_block_id: Option<String> = row.get(6)?;
    let created_at: String = row.get(7)?;
    let updated_at: String = row.get(8)?;

    Ok(Block {
        id,
        entity_id,
        parent_block_id,
        content,
        block_type,
        position,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        content_hash,
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

    async fn setup() -> SqliteBlockStorage {
        let pool = SqlitePool::memory().unwrap();
        let entity_storage = SqliteEntityStorage::new(pool.clone());
        let block_storage = SqliteBlockStorage::new(pool.clone());

        // Create test entity
        entity_storage
            .store_entity(Entity::new("note:1".to_string(), EntityType::Note))
            .await
            .unwrap();

        block_storage
    }

    #[tokio::test]
    async fn test_block_crud() {
        let storage = setup().await;
        let now = Utc::now();

        // Create
        let block = Block {
            id: "block:1".to_string(),
            entity_id: "note:1".to_string(),
            parent_block_id: None,
            content: "# Heading".to_string(),
            block_type: "heading".to_string(),
            position: 0,
            created_at: now,
            updated_at: now,
            content_hash: Some("abc123".to_string()),
        };

        let id = storage.store_block(block).await.unwrap();
        assert_eq!(id, "block:1");

        // Read
        let retrieved = storage.get_block("block:1").await.unwrap().unwrap();
        assert_eq!(retrieved.content, "# Heading");
        assert_eq!(retrieved.block_type, "heading");

        // Update
        let mut updated = retrieved.clone();
        updated.content = "# Updated Heading".to_string();
        storage.update_block("block:1", updated).await.unwrap();

        let after_update = storage.get_block("block:1").await.unwrap().unwrap();
        assert_eq!(after_update.content, "# Updated Heading");

        // Delete
        let deleted = storage.delete_block("block:1", false).await.unwrap();
        assert_eq!(deleted, 1);

        let gone = storage.get_block("block:1").await.unwrap();
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn test_block_hierarchy() {
        let storage = setup().await;
        let now = Utc::now();

        // Create parent block
        let parent = Block {
            id: "block:parent".to_string(),
            entity_id: "note:1".to_string(),
            parent_block_id: None,
            content: "Parent".to_string(),
            block_type: "section".to_string(),
            position: 0,
            created_at: now,
            updated_at: now,
            content_hash: None,
        };
        storage.store_block(parent).await.unwrap();

        // Create child blocks
        for i in 0..3 {
            let child = Block {
                id: format!("block:child{}", i),
                entity_id: "note:1".to_string(),
                parent_block_id: Some("block:parent".to_string()),
                content: format!("Child {}", i),
                block_type: "paragraph".to_string(),
                position: i,
                created_at: now,
                updated_at: now,
                content_hash: None,
            };
            storage.store_block(child).await.unwrap();
        }

        // Get children
        let children = storage.get_child_blocks("block:parent").await.unwrap();
        assert_eq!(children.len(), 3);

        // Get all blocks for entity
        let all = storage.get_blocks("note:1").await.unwrap();
        assert_eq!(all.len(), 4);

        // Delete all
        let deleted = storage.delete_blocks("note:1").await.unwrap();
        assert_eq!(deleted, 4);
    }
}
