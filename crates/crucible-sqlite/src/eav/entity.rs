//! EntityStorage implementation for SQLite

use crate::connection::SqlitePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::storage::eav_graph_traits::{Entity, EntityStorage, EntityType};
use crucible_core::storage::StorageResult;
use rusqlite::params;
use serde_json::Value;

/// SQLite implementation of EntityStorage
#[derive(Clone)]
pub struct SqliteEntityStorage {
    pool: SqlitePool,
}

impl SqliteEntityStorage {
    /// Create a new EntityStorage with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EntityStorage for SqliteEntityStorage {
    async fn store_entity(&self, entity: Entity) -> StorageResult<String> {
        let pool = self.pool.clone();
        let id = entity.id.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let type_str = entity_type_to_str(&entity.entity_type);
                let data_json = entity.data.map(|v| v.to_string());

                conn.execute(
                    r#"
                    INSERT INTO entities (id, type, created_at, updated_at, deleted_at, version, content_hash, created_by, vault_id, data)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                    ON CONFLICT(id) DO UPDATE SET
                        type = excluded.type,
                        updated_at = excluded.updated_at,
                        deleted_at = excluded.deleted_at,
                        version = excluded.version,
                        content_hash = excluded.content_hash,
                        created_by = excluded.created_by,
                        vault_id = excluded.vault_id,
                        data = excluded.data
                    "#,
                    params![
                        entity.id,
                        type_str,
                        entity.created_at.to_rfc3339(),
                        entity.updated_at.to_rfc3339(),
                        entity.deleted_at.map(|dt| dt.to_rfc3339()),
                        entity.version,
                        entity.content_hash,
                        entity.created_by,
                        entity.vault_id,
                        data_json,
                    ],
                )?;

                Ok(id)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_entity(&self, id: &str) -> StorageResult<Option<Entity>> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT id, type, created_at, updated_at, deleted_at, version, content_hash, created_by, vault_id, data
                    FROM entities
                    WHERE id = ?1
                    "#,
                )?;

                let entity = stmt
                    .query_row([&id], |row| {
                        Ok(row_to_entity(row)?)
                    })
                    .optional()?;

                Ok(entity)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn update_entity(&self, id: &str, entity: Entity) -> StorageResult<()> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let type_str = entity_type_to_str(&entity.entity_type);
                let data_json = entity.data.map(|v| v.to_string());

                let rows_affected = conn.execute(
                    r#"
                    UPDATE entities SET
                        type = ?2,
                        updated_at = ?3,
                        deleted_at = ?4,
                        version = ?5,
                        content_hash = ?6,
                        created_by = ?7,
                        vault_id = ?8,
                        data = ?9
                    WHERE id = ?1
                    "#,
                    params![
                        id,
                        type_str,
                        entity.updated_at.to_rfc3339(),
                        entity.deleted_at.map(|dt| dt.to_rfc3339()),
                        entity.version,
                        entity.content_hash,
                        entity.created_by,
                        entity.vault_id,
                        data_json,
                    ],
                )?;

                if rows_affected == 0 {
                    return Err(crate::error::SqliteError::NotFound(format!(
                        "Entity {} does not exist",
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

    async fn delete_entity(&self, id: &str) -> StorageResult<()> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let now = Utc::now().to_rfc3339();

                conn.execute(
                    "UPDATE entities SET deleted_at = ?2 WHERE id = ?1",
                    params![id, now],
                )?;

                Ok(())
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn entity_exists(&self, id: &str) -> StorageResult<bool> {
        let pool = self.pool.clone();
        let id = id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM entities WHERE id = ?1)",
                    [&id],
                    |row| row.get(0),
                )?;

                Ok(exists)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }
}

/// Convert EntityType to string for storage
fn entity_type_to_str(et: &EntityType) -> &'static str {
    match et {
        EntityType::Note => "note",
        EntityType::Block => "block",
        EntityType::Tag => "tag",
        EntityType::Section => "section",
        EntityType::Media => "media",
        EntityType::Person => "person",
    }
}

/// Convert string to EntityType
fn str_to_entity_type(s: &str) -> EntityType {
    match s {
        "note" => EntityType::Note,
        "block" => EntityType::Block,
        "tag" => EntityType::Tag,
        "section" => EntityType::Section,
        "media" => EntityType::Media,
        "person" => EntityType::Person,
        _ => EntityType::Note, // Default fallback
    }
}

/// Convert a database row to an Entity
fn row_to_entity(row: &rusqlite::Row) -> rusqlite::Result<Entity> {
    let id: String = row.get(0)?;
    let type_str: String = row.get(1)?;
    let created_at: String = row.get(2)?;
    let updated_at: String = row.get(3)?;
    let deleted_at: Option<String> = row.get(4)?;
    let version: i32 = row.get(5)?;
    let content_hash: Option<String> = row.get(6)?;
    let created_by: Option<String> = row.get(7)?;
    let vault_id: Option<String> = row.get(8)?;
    let data_str: Option<String> = row.get(9)?;

    let data: Option<Value> = data_str.and_then(|s| serde_json::from_str(&s).ok());

    Ok(Entity {
        id,
        entity_type: str_to_entity_type(&type_str),
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        deleted_at: deleted_at.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }),
        version,
        content_hash,
        created_by,
        vault_id,
        data,
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

    #[tokio::test]
    async fn test_entity_crud() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        // Create
        let entity = Entity::new("test:1".to_string(), EntityType::Note)
            .with_content_hash("abc123")
            .with_vault_id("vault:main");

        let id = storage.store_entity(entity).await.unwrap();
        assert_eq!(id, "test:1");

        // Read
        let retrieved = storage.get_entity("test:1").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "test:1");
        assert_eq!(retrieved.entity_type, EntityType::Note);
        assert_eq!(retrieved.content_hash, Some("abc123".to_string()));
        assert_eq!(retrieved.vault_id, Some("vault:main".to_string()));

        // Exists
        assert!(storage.entity_exists("test:1").await.unwrap());
        assert!(!storage.entity_exists("test:999").await.unwrap());

        // Update
        let mut updated = retrieved.clone();
        updated.content_hash = Some("xyz789".to_string());
        updated.version = 2;
        storage.update_entity("test:1", updated).await.unwrap();

        let after_update = storage.get_entity("test:1").await.unwrap().unwrap();
        assert_eq!(after_update.content_hash, Some("xyz789".to_string()));
        assert_eq!(after_update.version, 2);

        // Delete (soft)
        storage.delete_entity("test:1").await.unwrap();
        let deleted = storage.get_entity("test:1").await.unwrap().unwrap();
        assert!(deleted.deleted_at.is_some());
    }

    #[tokio::test]
    async fn test_entity_upsert() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        // First insert
        let entity = Entity::new("test:upsert".to_string(), EntityType::Note);
        storage.store_entity(entity).await.unwrap();

        // Upsert (should update, not fail)
        let updated_entity = Entity::new("test:upsert".to_string(), EntityType::Note)
            .with_content_hash("updated_hash");
        storage.store_entity(updated_entity).await.unwrap();

        let retrieved = storage.get_entity("test:upsert").await.unwrap().unwrap();
        assert_eq!(retrieved.content_hash, Some("updated_hash".to_string()));
    }

    #[tokio::test]
    async fn test_update_nonexistent_fails() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        let entity = Entity::new("nonexistent".to_string(), EntityType::Note);
        let result = storage.update_entity("nonexistent", entity).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        let result = storage.get_entity("does:not:exist").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_entity_with_unicode_content() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        // Test with unicode characters including emoji, CJK, RTL
        let entity = Entity::new("note:unicode".to_string(), EntityType::Note)
            .with_content_hash("🔥火事📝مرحبا");
        storage.store_entity(entity).await.unwrap();

        let retrieved = storage.get_entity("note:unicode").await.unwrap().unwrap();
        assert_eq!(
            retrieved.content_hash,
            Some("🔥火事📝مرحبا".to_string())
        );
    }

    #[tokio::test]
    async fn test_entity_with_special_sql_chars() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        // Test with SQL-sensitive characters
        let entity = Entity::new("note:sql'test".to_string(), EntityType::Note)
            .with_content_hash("hash'; DROP TABLE entities; --")
            .with_vault_id("vault\"test");
        storage.store_entity(entity).await.unwrap();

        let retrieved = storage
            .get_entity("note:sql'test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            retrieved.content_hash,
            Some("hash'; DROP TABLE entities; --".to_string())
        );
        assert_eq!(retrieved.vault_id, Some("vault\"test".to_string()));
    }

    #[tokio::test]
    async fn test_entity_with_json_data() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        let mut entity = Entity::new("note:json".to_string(), EntityType::Note);
        entity.data = Some(serde_json::json!({
            "nested": {"key": "value"},
            "array": [1, 2, 3],
            "unicode": "日本語"
        }));
        storage.store_entity(entity).await.unwrap();

        let retrieved = storage.get_entity("note:json").await.unwrap().unwrap();
        assert!(retrieved.data.is_some());
        let data = retrieved.data.unwrap();
        assert_eq!(data["nested"]["key"], "value");
        assert_eq!(data["array"][1], 2);
        assert_eq!(data["unicode"], "日本語");
    }

    #[tokio::test]
    async fn test_all_entity_types() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        let types = vec![
            (EntityType::Note, "note:1"),
            (EntityType::Block, "block:1"),
            (EntityType::Tag, "tag:1"),
            (EntityType::Section, "section:1"),
            (EntityType::Media, "media:1"),
            (EntityType::Person, "person:1"),
        ];

        for (entity_type, id) in types {
            let entity = Entity::new(id.to_string(), entity_type);
            storage.store_entity(entity).await.unwrap();

            let retrieved = storage.get_entity(id).await.unwrap().unwrap();
            assert_eq!(retrieved.entity_type, entity_type);
        }
    }

    #[tokio::test]
    async fn test_delete_nonexistent_is_noop() {
        let pool = SqlitePool::memory().unwrap();
        let storage = SqliteEntityStorage::new(pool);

        // Deleting non-existent should not error (soft delete updates, no rows affected is ok)
        let result = storage.delete_entity("does:not:exist").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_entity_access() {
        use std::sync::Arc;

        let pool = SqlitePool::memory().unwrap();
        let storage = Arc::new(SqliteEntityStorage::new(pool));

        // Create initial entity
        let entity = Entity::new("note:concurrent".to_string(), EntityType::Note);
        storage.store_entity(entity).await.unwrap();

        // Spawn multiple readers
        let mut handles = vec![];
        for _ in 0..10 {
            let s = Arc::clone(&storage);
            handles.push(tokio::spawn(async move {
                s.get_entity("note:concurrent").await.unwrap()
            }));
        }

        // All reads should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_some());
        }
    }
}
