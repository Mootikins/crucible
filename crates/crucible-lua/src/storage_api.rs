//! Plugin storage API module for Lua scripts
//!
//! Provides `cru.storage.*` functions for reading/writing
//! namespaced EAV properties from Lua plugins.
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Set a property on an entity
//! cru.storage.set("entity-id", "key", "value")
//!
//! -- Get a property value (returns nil if missing)
//! local val = cru.storage.get("entity-id", "key")
//!
//! -- List all properties for an entity (returns {key=value, ...})
//! local props = cru.storage.list("entity-id")
//!
//! -- Find entities with a matching property (returns array of entity IDs)
//! local ids = cru.storage.find("status", "active")
//!
//! -- Delete a property (returns true if deleted)
//! local ok = cru.storage.delete("entity-id", "key")
//! ```
//!
//! ## Namespacing
//!
//! All operations are automatically scoped to `namespace = "plugin:{plugin_name}"`.
//! The plugin name is read from `cru._current_plugin` at call time.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_core::storage::PropertyStore;
use mlua::{Lua, Table, Value};
use std::sync::Arc;

/// Register the storage module with stub functions that return nil/empty.
///
/// Called during executor setup. Stubs are replaced by
/// `register_storage_module_with_store` when a kiln opens and storage is available.
pub fn register_storage_module(lua: &Lua) -> Result<(), LuaError> {
    let storage = lua.create_table()?;

    let set_stub = lua.create_async_function(
        |_, (_entity_id, _key, _value): (String, String, String)| async move { Ok(Value::Nil) },
    )?;
    storage.set("set", set_stub)?;

    let get_stub =
        lua.create_async_function(|_, (_entity_id, _key): (String, String)| async move {
            Ok(Value::Nil)
        })?;
    storage.set("get", get_stub)?;

    let list_stub = lua.create_async_function(|lua, _entity_id: String| async move {
        Ok(Value::Table(lua.create_table()?))
    })?;
    storage.set("list", list_stub)?;

    let find_stub =
        lua.create_async_function(|lua, (_key, _value): (String, String)| async move {
            Ok(Value::Table(lua.create_table()?))
        })?;
    storage.set("find", find_stub)?;

    let delete_stub =
        lua.create_async_function(|_, (_entity_id, _key): (String, String)| async move {
            Ok(Value::Boolean(false))
        })?;
    storage.set("delete", delete_stub)?;

    register_in_namespaces(lua, "storage", storage)?;
    Ok(())
}

/// Read the current plugin namespace from `cru._current_plugin`.
///
/// Returns `Err` if no plugin context is set (e.g., calling from outside a plugin).
fn get_plugin_namespace(lua: &Lua) -> Result<String, mlua::Error> {
    let cru: Table = lua.globals().get("cru")?;
    let plugin_name: String = cru.get("_current_plugin").map_err(|_| {
        mlua::Error::runtime("cru.storage requires a plugin context (cru._current_plugin not set)")
    })?;
    Ok(format!("plugin:{}", plugin_name))
}

/// Convert a `StorageResult` into an `mlua::Result`, mapping storage errors to Lua runtime errors.
fn storage_err<T>(result: crucible_core::storage::StorageResult<T>) -> Result<T, mlua::Error> {
    result.map_err(|e| mlua::Error::runtime(format!("Storage error: {e}")))
}

/// Upgrade the storage module with a real PropertyStore backend.
///
/// The plugin namespace is determined dynamically from `cru._current_plugin`
/// at call time, so this only needs to be called once (not per-plugin).
pub fn register_storage_module_with_store(
    lua: &Lua,
    store: Arc<dyn PropertyStore>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let storage: Table = cru.get("storage")?;

    // set(entity_id, key, value)
    let s = Arc::clone(&store);
    let set_fn = lua.create_async_function(
        move |lua, (entity_id, key, value): (String, String, String)| {
            let s = Arc::clone(&s);
            async move {
                let ns = get_plugin_namespace(&lua)?;
                storage_err(s.property_set(&entity_id, &ns, &key, &value).await)?;
                Ok(Value::Boolean(true))
            }
        },
    )?;
    storage.set("set", set_fn)?;

    // get(entity_id, key)
    let s = Arc::clone(&store);
    let get_fn = lua.create_async_function(move |lua, (entity_id, key): (String, String)| {
        let s = Arc::clone(&s);
        async move {
            let ns = get_plugin_namespace(&lua)?;
            match storage_err(s.property_get(&entity_id, &ns, &key).await)? {
                Some(val) => Ok(Value::String(lua.create_string(&val)?)),
                None => Ok(Value::Nil),
            }
        }
    })?;
    storage.set("get", get_fn)?;

    // list(entity_id)
    let s = Arc::clone(&store);
    let list_fn = lua.create_async_function(move |lua, entity_id: String| {
        let s = Arc::clone(&s);
        async move {
            let ns = get_plugin_namespace(&lua)?;
            let props = storage_err(s.property_list(&entity_id, &ns).await)?;
            let table = lua.create_table()?;
            for (key, value) in props {
                table.set(key, value)?;
            }
            Ok(Value::Table(table))
        }
    })?;
    storage.set("list", list_fn)?;

    // find(key, value)
    let s = Arc::clone(&store);
    let find_fn = lua.create_async_function(move |lua, (key, value): (String, String)| {
        let s = Arc::clone(&s);
        async move {
            let ns = get_plugin_namespace(&lua)?;
            let ids = storage_err(s.property_find(&ns, &key, &value).await)?;
            let table = lua.create_table()?;
            for (i, id) in ids.iter().enumerate() {
                table.set(i + 1, id.as_str())?;
            }
            Ok(Value::Table(table))
        }
    })?;
    storage.set("find", find_fn)?;

    // delete(entity_id, key)
    let s = Arc::clone(&store);
    let delete_fn = lua.create_async_function(move |lua, (entity_id, key): (String, String)| {
        let s = Arc::clone(&s);
        async move {
            let ns = get_plugin_namespace(&lua)?;
            let deleted = storage_err(s.property_delete(&entity_id, &ns, &key).await)?;
            Ok(Value::Boolean(deleted))
        }
    })?;
    storage.set("delete", delete_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn register_storage_module_creates_namespace() {
        let lua = TestLuaBuilder::new().with_storage().build();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let storage: Table = cru.get("storage").expect("cru.storage should exist");

        assert!(storage.contains_key("set").unwrap());
        assert!(storage.contains_key("get").unwrap());
        assert!(storage.contains_key("list").unwrap());
        assert!(storage.contains_key("find").unwrap());
        assert!(storage.contains_key("delete").unwrap());
    }

    #[test]
    fn storage_also_registered_as_crucible() {
        let lua = TestLuaBuilder::new().with_storage().build();

        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let storage: Table = crucible
            .get("storage")
            .expect("crucible.storage should exist");

        assert!(storage.contains_key("set").unwrap());
    }

    #[tokio::test]
    async fn storage_get_stub_returns_nil() {
        let lua = TestLuaBuilder::new().with_storage().build();

        let result: Value = lua
            .load(r#"return cru.storage.get("entity", "key")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn storage_list_stub_returns_empty() {
        let lua = TestLuaBuilder::new().with_storage().build();

        let result: Table = lua
            .load(r#"return cru.storage.list("entity")"#)
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }

    #[tokio::test]
    async fn storage_delete_stub_returns_false() {
        let lua = TestLuaBuilder::new().with_storage().build();

        let result: bool = lua
            .load(r#"return cru.storage.delete("entity", "key")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(!result);
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;
    use async_trait::async_trait;
    use crucible_core::storage::StorageResult;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Mock PropertyStore for testing
    struct MockPropertyStore {
        data: Mutex<HashMap<(String, String, String), String>>,
    }

    impl MockPropertyStore {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl PropertyStore for MockPropertyStore {
        async fn property_set(
            &self,
            entity_id: &str,
            namespace: &str,
            key: &str,
            value: &str,
        ) -> StorageResult<()> {
            let mut data = self.data.lock().unwrap();
            data.insert(
                (
                    entity_id.to_string(),
                    namespace.to_string(),
                    key.to_string(),
                ),
                value.to_string(),
            );
            Ok(())
        }

        async fn property_get(
            &self,
            entity_id: &str,
            namespace: &str,
            key: &str,
        ) -> StorageResult<Option<String>> {
            let data = self.data.lock().unwrap();
            Ok(data
                .get(&(
                    entity_id.to_string(),
                    namespace.to_string(),
                    key.to_string(),
                ))
                .cloned())
        }

        async fn property_list(
            &self,
            entity_id: &str,
            namespace: &str,
        ) -> StorageResult<Vec<(String, String)>> {
            let data = self.data.lock().unwrap();
            let mut result = Vec::new();
            for ((eid, ns, key), value) in data.iter() {
                if eid == entity_id && ns == namespace {
                    result.push((key.clone(), value.clone()));
                }
            }
            result.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(result)
        }

        async fn property_find(
            &self,
            namespace: &str,
            key: &str,
            value: &str,
        ) -> StorageResult<Vec<String>> {
            let data = self.data.lock().unwrap();
            let mut result: Vec<String> = data
                .iter()
                .filter(|((_, ns, k), v)| ns == namespace && k == key && v.as_str() == value)
                .map(|((eid, _, _), _)| eid.clone())
                .collect();
            result.sort();
            result.dedup();
            Ok(result)
        }

        async fn property_delete(
            &self,
            entity_id: &str,
            namespace: &str,
            key: &str,
        ) -> StorageResult<bool> {
            let mut data = self.data.lock().unwrap();
            Ok(data
                .remove(&(
                    entity_id.to_string(),
                    namespace.to_string(),
                    key.to_string(),
                ))
                .is_some())
        }
    }

    fn setup_lua_with_store() -> mlua::Lua {
        let store: Arc<dyn PropertyStore> = Arc::new(MockPropertyStore::new());
        let lua = TestLuaBuilder::new().with_storage_store(store).build();
        // Set the plugin context so namespace resolution works
        lua.load(r#"cru._current_plugin = "test-plugin""#)
            .exec()
            .unwrap();
        lua
    }

    #[tokio::test]
    async fn set_and_get_via_lua() {
        let lua = setup_lua_with_store();

        let result: bool = lua
            .load(r#"return cru.storage.set("e1", "mykey", "myval")"#)
            .eval_async()
            .await
            .unwrap();
        assert!(result);

        let val: String = lua
            .load(r#"return cru.storage.get("e1", "mykey")"#)
            .eval_async()
            .await
            .unwrap();
        assert_eq!(val, "myval");
    }

    #[tokio::test]
    async fn get_missing_returns_nil() {
        let lua = setup_lua_with_store();

        let result: Value = lua
            .load(r#"return cru.storage.get("e1", "missing")"#)
            .eval_async()
            .await
            .unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[tokio::test]
    async fn list_returns_table() {
        let lua = setup_lua_with_store();

        lua.load(r#"cru.storage.set("e1", "a", "1")"#)
            .eval_async::<Value>()
            .await
            .unwrap();
        lua.load(r#"cru.storage.set("e1", "b", "2")"#)
            .eval_async::<Value>()
            .await
            .unwrap();

        let result: Table = lua
            .load(r#"return cru.storage.list("e1")"#)
            .eval_async()
            .await
            .unwrap();
        assert_eq!(result.get::<String>("a").unwrap(), "1");
        assert_eq!(result.get::<String>("b").unwrap(), "2");
    }

    #[tokio::test]
    async fn find_returns_entity_ids() {
        let lua = setup_lua_with_store();

        lua.load(r#"cru.storage.set("e1", "status", "active")"#)
            .eval_async::<Value>()
            .await
            .unwrap();
        lua.load(r#"cru.storage.set("e2", "status", "active")"#)
            .eval_async::<Value>()
            .await
            .unwrap();

        let result: Table = lua
            .load(r#"return cru.storage.find("status", "active")"#)
            .eval_async()
            .await
            .unwrap();
        assert_eq!(result.len().unwrap(), 2);
    }

    #[tokio::test]
    async fn delete_returns_true_when_existed() {
        let lua = setup_lua_with_store();

        lua.load(r#"cru.storage.set("e1", "k", "v")"#)
            .eval_async::<Value>()
            .await
            .unwrap();

        let deleted: bool = lua
            .load(r#"return cru.storage.delete("e1", "k")"#)
            .eval_async()
            .await
            .unwrap();
        assert!(deleted);

        let val: Value = lua
            .load(r#"return cru.storage.get("e1", "k")"#)
            .eval_async()
            .await
            .unwrap();
        assert!(matches!(val, Value::Nil));
    }

    #[tokio::test]
    async fn no_plugin_context_gives_error() {
        let store: Arc<dyn PropertyStore> = Arc::new(MockPropertyStore::new());
        let lua = TestLuaBuilder::new().with_storage_store(store).build();
        // Deliberately NOT setting cru._current_plugin

        let result: Result<Value, _> = lua
            .load(r#"return cru.storage.get("e1", "k")"#)
            .eval_async()
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("_current_plugin"),
            "Error should mention _current_plugin: {}",
            err_msg
        );
    }
}
