//! PropertyStore trait for EAV-namespaced properties.
//!
//! Used by the plugin storage API (`cru.storage`) to let plugins read/write
//! namespaced key-value properties against entity IDs in the EAV `properties` table.

use async_trait::async_trait;

use crate::storage::StorageResult;

/// EAV property storage for plugin-namespaced key-value data.
///
/// Each property is scoped to `(entity_id, namespace, key)` with a text value.
/// Plugins use `namespace = "plugin:{plugin_name}"` to isolate their data.
#[async_trait]
pub trait PropertyStore: Send + Sync {
    /// Set a property value. Upserts (insert or update).
    async fn property_set(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<()>;

    /// Get a property value. Returns `None` if not found.
    async fn property_get(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<Option<String>>;

    /// List all properties for an entity in a namespace.
    ///
    /// Returns `(key, value)` pairs.
    async fn property_list(
        &self,
        entity_id: &str,
        namespace: &str,
    ) -> StorageResult<Vec<(String, String)>>;

    /// Find entity IDs that have a property matching `key=value` in a namespace.
    async fn property_find(
        &self,
        namespace: &str,
        key: &str,
        value: &str,
    ) -> StorageResult<Vec<String>>;

    /// Delete a property. Returns `true` if a row was deleted.
    async fn property_delete(
        &self,
        entity_id: &str,
        namespace: &str,
        key: &str,
    ) -> StorageResult<bool>;
}
