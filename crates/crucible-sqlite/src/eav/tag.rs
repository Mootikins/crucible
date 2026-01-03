//! TagStorage implementation for SQLite

use crate::connection::SqlitePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::storage::eav_graph_traits::{EntityTag, Tag, TagStorage};
use crucible_core::storage::StorageResult;
use rusqlite::params;
use uuid::Uuid;

/// SQLite implementation of TagStorage
#[derive(Clone)]
pub struct SqliteTagStorage {
    pool: SqlitePool,
}

impl SqliteTagStorage {
    /// Create a new TagStorage with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TagStorage for SqliteTagStorage {
    async fn store_tag(&self, tag: Tag) -> StorageResult<String> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let id = if tag.id.is_empty() {
                    format!("tag:{}", Uuid::new_v4())
                } else {
                    tag.id.clone()
                };

                // Calculate path from name (e.g., "project/ai" -> "project/ai")
                let path = tag.name.clone();
                let depth = tag.name.matches('/').count() as i32;

                conn.execute(
                    r#"
                    INSERT INTO tags (id, name, parent_id, path, depth, created_at, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    ON CONFLICT(name) DO UPDATE SET
                        parent_id = excluded.parent_id,
                        path = excluded.path,
                        depth = excluded.depth,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        id,
                        tag.name,
                        tag.parent_tag_id,
                        path,
                        depth,
                        tag.created_at.to_rfc3339(),
                        tag.updated_at.to_rfc3339(),
                    ],
                )?;

                // Return the name (which is unique) as identifier
                Ok(tag.name)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_tag(&self, name: &str) -> StorageResult<Option<Tag>> {
        let pool = self.pool.clone();
        let name = name.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, name, parent_id, created_at, updated_at
                    FROM tags
                    WHERE name = ?1
                    "#,
                )?;

                let tag = stmt.query_row([&name], row_to_tag).optional()?;

                Ok(tag)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_child_tags(&self, parent_tag_name: &str) -> StorageResult<Vec<Tag>> {
        let pool = self.pool.clone();
        let parent_name = parent_tag_name.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // First get the parent tag ID
                let parent_id: Option<String> = conn
                    .query_row(
                        "SELECT id FROM tags WHERE name = ?1",
                        [&parent_name],
                        |row| row.get(0),
                    )
                    .optional()?;

                match parent_id {
                    Some(pid) => {
                        let mut stmt = conn.prepare(
                            r#"
                            SELECT id, name, parent_id, created_at, updated_at
                            FROM tags
                            WHERE parent_id = ?1
                            ORDER BY name
                            "#,
                        )?;

                        let tags = stmt
                            .query_map([&pid], row_to_tag)?
                            .filter_map(Result::ok)
                            .collect();

                        Ok(tags)
                    }
                    None => Ok(Vec::new()),
                }
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn associate_tag(&self, entity_tag: EntityTag) -> StorageResult<()> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute(
                    r#"
                    INSERT INTO entity_tags (entity_id, tag_id, created_at)
                    VALUES (?1, ?2, ?3)
                    ON CONFLICT(entity_id, tag_id) DO NOTHING
                    "#,
                    params![
                        entity_tag.entity_id,
                        entity_tag.tag_id,
                        entity_tag.created_at.to_rfc3339(),
                    ],
                )?;

                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_entity_tags(&self, entity_id: &str) -> StorageResult<Vec<Tag>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT t.id, t.name, t.parent_id, t.created_at, t.updated_at
                    FROM tags t
                    JOIN entity_tags et ON et.tag_id = t.id
                    WHERE et.entity_id = ?1
                    ORDER BY t.name
                    "#,
                )?;

                let tags = stmt
                    .query_map([&entity_id], row_to_tag)?
                    .filter_map(Result::ok)
                    .collect();

                Ok(tags)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_entities_by_tag(&self, tag_id: &str) -> StorageResult<Vec<String>> {
        let pool = self.pool.clone();
        let tag_id = tag_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Get entities with this tag OR any descendant tag
                let mut stmt = conn.prepare(
                    r#"
                    WITH RECURSIVE tag_tree(id) AS (
                        SELECT id FROM tags WHERE id = ?1
                        UNION ALL
                        SELECT t.id FROM tags t
                        JOIN tag_tree tt ON t.parent_id = tt.id
                    )
                    SELECT DISTINCT et.entity_id
                    FROM entity_tags et
                    JOIN tag_tree tt ON et.tag_id = tt.id
                    ORDER BY et.entity_id
                    "#,
                )?;

                let entities: Vec<String> = stmt
                    .query_map([&tag_id], |row| row.get(0))?
                    .filter_map(Result::ok)
                    .collect();

                Ok(entities)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn dissociate_tag(&self, entity_id: &str, tag_id: &str) -> StorageResult<()> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let tag_id = tag_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute(
                    "DELETE FROM entity_tags WHERE entity_id = ?1 AND tag_id = ?2",
                    params![entity_id, tag_id],
                )?;

                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_tag(&self, id: &str, delete_associations: bool) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                if delete_associations {
                    // Delete tag associations first
                    conn.execute(
                        "DELETE FROM entity_tags WHERE tag_id = ?1",
                        [&id],
                    )?;
                }

                // Delete the tag (will cascade to children via FK)
                let deleted = conn.execute("DELETE FROM tags WHERE id = ?1", [&id])?;

                Ok(deleted)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }
}

/// Convert a database row to a Tag
fn row_to_tag(row: &rusqlite::Row) -> rusqlite::Result<Tag> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let parent_tag_id: Option<String> = row.get(2)?;
    let created_at: String = row.get(3)?;
    let updated_at: String = row.get(4)?;

    Ok(Tag {
        id,
        name,
        parent_tag_id,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
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

    async fn setup() -> (SqlitePool, SqliteTagStorage) {
        let pool = SqlitePool::memory().unwrap();
        let entity_storage = SqliteEntityStorage::new(pool.clone());
        let tag_storage = SqliteTagStorage::new(pool.clone());

        // Create test entity
        entity_storage
            .store_entity(Entity::new("note:1".to_string(), EntityType::Note))
            .await
            .unwrap();

        (pool, tag_storage)
    }

    #[tokio::test]
    async fn test_tag_crud() {
        let (_pool, storage) = setup().await;
        let now = Utc::now();

        // Create tag
        let tag = Tag {
            id: "tag:1".to_string(),
            name: "project".to_string(),
            parent_tag_id: None,
            created_at: now,
            updated_at: now,
        };

        let name = storage.store_tag(tag).await.unwrap();
        assert_eq!(name, "project");

        // Read
        let retrieved = storage.get_tag("project").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "project");
        assert!(retrieved.parent_tag_id.is_none());

        // Delete
        let deleted = storage.delete_tag("tag:1", true).await.unwrap();
        assert_eq!(deleted, 1);

        let gone = storage.get_tag("project").await.unwrap();
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn test_tag_hierarchy() {
        let (_pool, storage) = setup().await;
        let now = Utc::now();

        // Create parent tag
        let parent = Tag {
            id: "tag:parent".to_string(),
            name: "project".to_string(),
            parent_tag_id: None,
            created_at: now,
            updated_at: now,
        };
        storage.store_tag(parent).await.unwrap();

        // Create child tags
        let child1 = Tag {
            id: "tag:child1".to_string(),
            name: "project/ai".to_string(),
            parent_tag_id: Some("tag:parent".to_string()),
            created_at: now,
            updated_at: now,
        };
        storage.store_tag(child1).await.unwrap();

        let child2 = Tag {
            id: "tag:child2".to_string(),
            name: "project/web".to_string(),
            parent_tag_id: Some("tag:parent".to_string()),
            created_at: now,
            updated_at: now,
        };
        storage.store_tag(child2).await.unwrap();

        // Get children
        let children = storage.get_child_tags("project").await.unwrap();
        assert_eq!(children.len(), 2);
    }

    #[tokio::test]
    async fn test_tag_association() {
        let (_pool, storage) = setup().await;
        let now = Utc::now();

        // Create tag
        let tag = Tag {
            id: "tag:test".to_string(),
            name: "test".to_string(),
            parent_tag_id: None,
            created_at: now,
            updated_at: now,
        };
        storage.store_tag(tag).await.unwrap();

        // Associate with entity
        let entity_tag = EntityTag {
            entity_id: "note:1".to_string(),
            tag_id: "tag:test".to_string(),
            created_at: now,
        };
        storage.associate_tag(entity_tag).await.unwrap();

        // Get entity tags
        let tags = storage.get_entity_tags("note:1").await.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "test");

        // Get entities by tag
        let entities = storage.get_entities_by_tag("tag:test").await.unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0], "note:1");

        // Dissociate
        storage.dissociate_tag("note:1", "tag:test").await.unwrap();
        let tags_after = storage.get_entity_tags("note:1").await.unwrap();
        assert_eq!(tags_after.len(), 0);
    }
}
