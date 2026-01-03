//! PropertyStorage implementation for SQLite

use crate::connection::SqlitePool;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::storage::eav_graph_traits::{
    AttributeValue, Property, PropertyNamespace, PropertyStorage,
};
use crucible_core::storage::StorageResult;
use rusqlite::params;

/// SQLite implementation of PropertyStorage
#[derive(Clone)]
pub struct SqlitePropertyStorage {
    pool: SqlitePool,
}

impl SqlitePropertyStorage {
    /// Create a new PropertyStorage with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PropertyStorage for SqlitePropertyStorage {
    async fn batch_upsert_properties(&self, properties: Vec<Property>) -> StorageResult<usize> {
        if properties.is_empty() {
            return Ok(0);
        }

        let pool = self.pool.clone();
        let count = properties.len();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let tx = conn.unchecked_transaction()?;

                {
                    let mut stmt = tx.prepare(
                        r#"
                        INSERT INTO properties (entity_id, namespace, key, value, source, confidence, created_at, updated_at)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                        ON CONFLICT(entity_id, namespace, key) DO UPDATE SET
                            value = excluded.value,
                            source = excluded.source,
                            confidence = excluded.confidence,
                            updated_at = excluded.updated_at
                        "#,
                    )?;

                    for prop in &properties {
                        let value_json = serde_json::to_string(&prop.value)
                            .map_err(|e| crate::error::SqliteError::Serialization(e.to_string()))?;

                        stmt.execute(params![
                            prop.entity_id,
                            prop.namespace.as_str(),
                            prop.key,
                            value_json,
                            "parser", // Default source
                            1.0f64,   // Default confidence
                            prop.created_at.to_rfc3339(),
                            prop.updated_at.to_rfc3339(),
                        ])?;
                    }
                }

                tx.commit()?;
                Ok(count)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT entity_id, namespace, key, value, created_at, updated_at
                    FROM properties
                    WHERE entity_id = ?1
                    "#,
                )?;

                let properties = stmt
                    .query_map([&entity_id], row_to_property)?
                    .filter_map(Result::ok)
                    .collect();

                Ok(properties)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<Vec<Property>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.as_str().to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT entity_id, namespace, key, value, created_at, updated_at
                    FROM properties
                    WHERE entity_id = ?1 AND namespace = ?2
                    "#,
                )?;

                let properties = stmt
                    .query_map(params![entity_id, namespace], row_to_property)?
                    .filter_map(Result::ok)
                    .collect();

                Ok(properties)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn get_property(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
        key: &str,
    ) -> StorageResult<Option<Property>> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.as_str().to_string();
        let key = key.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(
                    r#"
                    SELECT entity_id, namespace, key, value, created_at, updated_at
                    FROM properties
                    WHERE entity_id = ?1 AND namespace = ?2 AND key = ?3
                    "#,
                )?;

                let property = stmt
                    .query_row(params![entity_id, namespace, key], row_to_property)
                    .optional()?;

                Ok(property)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_properties(&self, entity_id: &str) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let deleted = conn.execute(
                    "DELETE FROM properties WHERE entity_id = ?1",
                    [&entity_id],
                )?;
                Ok(deleted)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }

    async fn delete_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<usize> {
        let pool = self.pool.clone();
        let entity_id = entity_id.to_string();
        let namespace = namespace.as_str().to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let deleted = conn.execute(
                    "DELETE FROM properties WHERE entity_id = ?1 AND namespace = ?2",
                    params![entity_id, namespace],
                )?;
                Ok(deleted)
            })
        })
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))?
        .map_err(Into::into)
    }
}

/// Convert a database row to a Property
fn row_to_property(row: &rusqlite::Row) -> rusqlite::Result<Property> {
    let entity_id: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let key: String = row.get(2)?;
    let value_json: String = row.get(3)?;
    let created_at: String = row.get(4)?;
    let updated_at: String = row.get(5)?;

    let value: AttributeValue = serde_json::from_str(&value_json).unwrap_or(AttributeValue::Text(value_json));

    let namespace = PropertyNamespace(std::borrow::Cow::Owned(namespace_str));

    Ok(Property {
        entity_id,
        namespace,
        key,
        value,
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

    async fn setup() -> (SqlitePool, SqliteEntityStorage, SqlitePropertyStorage) {
        let pool = SqlitePool::memory().unwrap();
        let entity_storage = SqliteEntityStorage::new(pool.clone());
        let property_storage = SqlitePropertyStorage::new(pool.clone());

        // Create a test entity
        let entity = Entity::new("test:1".to_string(), EntityType::Note);
        entity_storage.store_entity(entity).await.unwrap();

        (pool, entity_storage, property_storage)
    }

    #[tokio::test]
    async fn test_property_crud() {
        let (_pool, _entity, storage) = setup().await;
        let now = Utc::now();

        // Create properties
        let properties = vec![
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: AttributeValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "priority".to_string(),
                value: AttributeValue::Number(5.0),
                created_at: now,
                updated_at: now,
            },
        ];

        let count = storage.batch_upsert_properties(properties).await.unwrap();
        assert_eq!(count, 2);

        // Read all
        let all = storage.get_properties("test:1").await.unwrap();
        assert_eq!(all.len(), 2);

        // Read by namespace
        let frontmatter = storage
            .get_properties_by_namespace("test:1", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(frontmatter.len(), 2);

        // Read single
        let author = storage
            .get_property("test:1", &PropertyNamespace::frontmatter(), "author")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(author.value, AttributeValue::Text("John Doe".to_string()));

        // Delete by namespace
        let deleted = storage
            .delete_properties_by_namespace("test:1", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(deleted, 2);

        let remaining = storage.get_properties("test:1").await.unwrap();
        assert_eq!(remaining.len(), 0);
    }

    #[tokio::test]
    async fn test_property_upsert() {
        let (_pool, _entity, storage) = setup().await;
        let now = Utc::now();

        // Insert
        let prop = Property {
            entity_id: "test:1".to_string(),
            namespace: PropertyNamespace::core(),
            key: "version".to_string(),
            value: AttributeValue::Number(1.0),
            created_at: now,
            updated_at: now,
        };
        storage.batch_upsert_properties(vec![prop]).await.unwrap();

        // Upsert (update)
        let updated_prop = Property {
            entity_id: "test:1".to_string(),
            namespace: PropertyNamespace::core(),
            key: "version".to_string(),
            value: AttributeValue::Number(2.0),
            created_at: now,
            updated_at: Utc::now(),
        };
        storage.batch_upsert_properties(vec![updated_prop]).await.unwrap();

        // Verify only one property exists
        let props = storage
            .get_properties_by_namespace("test:1", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].value, AttributeValue::Number(2.0));
    }

    #[tokio::test]
    async fn test_property_all_value_types() {
        let (_pool, _entity, storage) = setup().await;
        let now = Utc::now();

        // Test all AttributeValue variants
        let properties = vec![
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "text_val".to_string(),
                value: AttributeValue::Text("hello".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "number_val".to_string(),
                value: AttributeValue::Number(42.5),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "bool_val".to_string(),
                value: AttributeValue::Bool(true),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "date_val".to_string(),
                value: AttributeValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "json_val".to_string(),
                value: AttributeValue::Json(serde_json::json!({"key": "value"})),
                created_at: now,
                updated_at: now,
            },
        ];

        storage.batch_upsert_properties(properties).await.unwrap();

        // Verify all were stored correctly
        let retrieved = storage
            .get_properties_by_namespace("test:1", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(retrieved.len(), 5);
    }

    #[tokio::test]
    async fn test_property_get_nonexistent() {
        let (_pool, _entity, storage) = setup().await;

        let result = storage
            .get_property("test:1", &PropertyNamespace::core(), "does_not_exist")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_property_delete_by_namespace() {
        let (_pool, _entity, storage) = setup().await;
        let now = Utc::now();

        // Create properties in different namespaces
        let properties = vec![
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "fm_key".to_string(),
                value: AttributeValue::Text("fm_value".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "test:1".to_string(),
                namespace: PropertyNamespace::core(),
                key: "core_key".to_string(),
                value: AttributeValue::Text("core_value".to_string()),
                created_at: now,
                updated_at: now,
            },
        ];
        storage.batch_upsert_properties(properties).await.unwrap();

        // Delete frontmatter namespace
        let deleted = storage
            .delete_properties_by_namespace("test:1", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        // Frontmatter should be gone, core should remain
        let fm = storage
            .get_properties_by_namespace("test:1", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert!(fm.is_empty());

        let core = storage
            .get_properties_by_namespace("test:1", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(core.len(), 1);
    }

    #[tokio::test]
    async fn test_property_with_unicode_and_special_chars() {
        let (_pool, _entity, storage) = setup().await;
        let now = Utc::now();

        let prop = Property {
            entity_id: "test:1".to_string(),
            namespace: PropertyNamespace::plugin("日本語プラグイン"),
            key: "キー'\"".to_string(),
            value: AttributeValue::Text("値🔥; DROP TABLE --".to_string()),
            created_at: now,
            updated_at: now,
        };
        storage.batch_upsert_properties(vec![prop]).await.unwrap();

        let retrieved = storage
            .get_property(
                "test:1",
                &PropertyNamespace::plugin("日本語プラグイン"),
                "キー'\"",
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            retrieved.value,
            AttributeValue::Text("値🔥; DROP TABLE --".to_string())
        );
    }
}
