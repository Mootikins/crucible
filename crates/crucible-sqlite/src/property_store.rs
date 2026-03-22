//! PropertyStore implementation for SQLite
//!
//! Provides EAV property storage using the existing `properties` table.
//! Plugin data is stored with `source = 'plugin'` and namespaced keys.

use async_trait::async_trait;
use rusqlite::{params, OptionalExtension};

use crate::connection::SqlitePool;
use crate::error_ext::SqliteResultExt;
use crate::note_store::SqliteNoteStore;
use crucible_core::storage::{PropertyStore, StorageResult};

#[async_trait]
impl PropertyStore for SqliteNoteStore {
    async fn property_set(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<()> {
        let pool = self.pool().clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();
        let value = value.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO properties (entity_id, namespace, key, value, source, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 'plugin', datetime('now'))
                     ON CONFLICT(entity_id, namespace, key) DO UPDATE SET
                         value = excluded.value,
                         source = excluded.source,
                         updated_at = datetime('now')",
                    params![entity_id, namespace, key, value],
                )
                .sql()?;
                Ok(())
            })
        })
        .await??;
        Ok(())
    }

    async fn property_get(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<Option<String>> {
        let pool = self.pool().clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let result = conn
                    .query_row(
                        "SELECT value FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2 AND key = ?3",
                        params![entity_id, namespace, key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .sql()?;
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_list(
        &self,
        entity_id: &str,
        namespace: &str,
    ) -> StorageResult<Vec<(String, String)>> {
        let pool = self.pool().clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT key, value FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2
                         ORDER BY key",
                    )
                    .sql()?;
                let rows = stmt
                    .query_map(params![entity_id, namespace], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .sql()?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row.sql()?);
                }
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_find(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<Vec<String>> {
        let pool = self.pool().clone();
        let namespace = namespace.to_string();
        let key = key.to_string();
        let value = value.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT entity_id FROM properties
                         WHERE namespace = ?1 AND key = ?2 AND value = ?3
                         ORDER BY entity_id",
                    )
                    .sql()?;
                let rows = stmt
                    .query_map(params![namespace, key, value], |row| {
                        row.get::<_, String>(0)
                    })
                    .sql()?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row.sql()?);
                }
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_delete(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<bool> {
        let pool = self.pool().clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();

        let deleted = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let changes = conn
                    .execute(
                        "DELETE FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2 AND key = ?3",
                        params![entity_id, namespace, key],
                    )
                    .sql()?;
                Ok(changes > 0)
            })
        })
        .await??;
        Ok(deleted)
    }
}

/// Standalone property store wrapping a pool directly.
///
/// Used when PropertyStore access is needed without an `SqliteNoteStore`.
#[derive(Clone)]
pub struct SqlitePropertyStore {
    pool: SqlitePool,
}

impl SqlitePropertyStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PropertyStore for SqlitePropertyStore {
    async fn property_set(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<()> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();
        let value = value.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute(
                    "INSERT INTO properties (entity_id, namespace, key, value, source, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 'plugin', datetime('now'))
                     ON CONFLICT(entity_id, namespace, key) DO UPDATE SET
                         value = excluded.value,
                         source = excluded.source,
                         updated_at = datetime('now')",
                    params![entity_id, namespace, key, value],
                )
                .sql()?;
                Ok(())
            })
        })
        .await??;
        Ok(())
    }

    async fn property_get(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<Option<String>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let result = conn
                    .query_row(
                        "SELECT value FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2 AND key = ?3",
                        params![entity_id, namespace, key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .sql()?;
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_list(
        &self,
        entity_id: &str,
        namespace: &str,
    ) -> StorageResult<Vec<(String, String)>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT key, value FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2
                         ORDER BY key",
                    )
                    .sql()?;
                let rows = stmt
                    .query_map(params![entity_id, namespace], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .sql()?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row.sql()?);
                }
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_find(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<Vec<String>> {
        let pool = self.pool.clone();
        let namespace = namespace.to_string();
        let key = key.to_string();
        let value = value.to_string();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT entity_id FROM properties
                         WHERE namespace = ?1 AND key = ?2 AND value = ?3
                         ORDER BY entity_id",
                    )
                    .sql()?;
                let rows = stmt
                    .query_map(params![namespace, key, value], |row| {
                        row.get::<_, String>(0)
                    })
                    .sql()?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row.sql()?);
                }
                Ok(result)
            })
        })
        .await??;
        Ok(result)
    }

    async fn property_delete(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<bool> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.to_string();
        let key = key.to_string();

        let deleted = tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let changes = conn
                    .execute(
                        "DELETE FROM properties
                         WHERE entity_id = ?1 AND namespace = ?2 AND key = ?3",
                        params![entity_id, namespace, key],
                    )
                    .sql()?;
                Ok(changes > 0)
            })
        })
        .await??;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteConfig;

    async fn test_store() -> SqlitePropertyStore {
        let pool = SqlitePool::new(SqliteConfig::memory()).unwrap();
        // Apply the schema that includes the properties table
        pool.with_connection(|conn| {
            crate::schema::apply_migrations(conn)
                .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?;
            // Create test entities for FK satisfaction
            for id in &["entity1", "e1", "e2", "e3"] {
                conn.execute(
                    "INSERT OR IGNORE INTO entities (id, type) VALUES (?1, 'note')",
                    params![id],
                )
                .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?;
            }
            Ok(())
        })
        .unwrap();
        SqlitePropertyStore::new(pool)
    }

    #[tokio::test]
    async fn set_and_get_property() {
        let store = test_store().await;
        store
            .property_set("entity1", "plugin:test", "key1", "value1")
            .await
            .unwrap();
        let val = store
            .property_get("entity1", "plugin:test", "key1")
            .await
            .unwrap();
        assert_eq!(val, Some("value1".to_string()));
    }

    #[tokio::test]
    async fn get_missing_property_returns_none() {
        let store = test_store().await;
        let val = store
            .property_get("entity1", "plugin:test", "missing")
            .await
            .unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn set_overwrites_existing() {
        let store = test_store().await;
        store
            .property_set("e1", "plugin:test", "k", "v1")
            .await
            .unwrap();
        store
            .property_set("e1", "plugin:test", "k", "v2")
            .await
            .unwrap();
        let val = store.property_get("e1", "plugin:test", "k").await.unwrap();
        assert_eq!(val, Some("v2".to_string()));
    }

    #[tokio::test]
    async fn list_properties() {
        let store = test_store().await;
        store
            .property_set("e1", "plugin:test", "a", "1")
            .await
            .unwrap();
        store
            .property_set("e1", "plugin:test", "b", "2")
            .await
            .unwrap();
        store
            .property_set("e1", "plugin:other", "c", "3")
            .await
            .unwrap();

        let props = store.property_list("e1", "plugin:test").await.unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0], ("a".to_string(), "1".to_string()));
        assert_eq!(props[1], ("b".to_string(), "2".to_string()));
    }

    #[tokio::test]
    async fn find_entities_by_property() {
        let store = test_store().await;
        store
            .property_set("e1", "plugin:test", "status", "active")
            .await
            .unwrap();
        store
            .property_set("e2", "plugin:test", "status", "active")
            .await
            .unwrap();
        store
            .property_set("e3", "plugin:test", "status", "inactive")
            .await
            .unwrap();

        let ids = store
            .property_find("plugin:test", "status", "active")
            .await
            .unwrap();
        assert_eq!(ids, vec!["e1".to_string(), "e2".to_string()]);
    }

    #[tokio::test]
    async fn delete_property() {
        let store = test_store().await;
        store
            .property_set("e1", "plugin:test", "k", "v")
            .await
            .unwrap();

        let deleted = store
            .property_delete("e1", "plugin:test", "k")
            .await
            .unwrap();
        assert!(deleted);

        let val = store.property_get("e1", "plugin:test", "k").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_false() {
        let store = test_store().await;
        let deleted = store
            .property_delete("e1", "plugin:test", "missing")
            .await
            .unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn namespaces_are_isolated() {
        let store = test_store().await;
        store
            .property_set("e1", "plugin:alpha", "k", "a")
            .await
            .unwrap();
        store
            .property_set("e1", "plugin:beta", "k", "b")
            .await
            .unwrap();

        let a = store.property_get("e1", "plugin:alpha", "k").await.unwrap();
        let b = store.property_get("e1", "plugin:beta", "k").await.unwrap();
        assert_eq!(a, Some("a".to_string()));
        assert_eq!(b, Some("b".to_string()));
    }
}
