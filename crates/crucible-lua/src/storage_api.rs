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
//! The plugin name comes from the VM's plugin context at call time — Rust-side
//! app data the loader and the handler dispatcher bracket, out of Lua's reach.

use crate::error::LuaError;
use crate::host_registry::Ns;
use crucible_core::storage::PropertyStore;
use mlua::{Lua, Table, Value};
use std::sync::Arc;

/// The declared type of each `cru.storage` function.
///
/// One set of declarations covers BOTH registrations, because both answer at
/// the same path and a plugin author cannot tell which one is mounted. Each
/// type therefore states the union of the two behaviours, and the union is
/// honest rather than convenient:
///
/// - `set` answers `true` when a store wrote the property, and `nil` when no
///   store is attached. The stub's `nil` is a real answer — "nothing was
///   written" — not a placeholder, so the type says `boolean?`.
/// - `get` answers the value or `nil`; the stub always takes the `nil` branch.
/// - `list` and `find` answer a table either way; the stub's is empty.
/// - `delete` answers a boolean either way; the stub always answers `false`.
///
/// The stub generator runs against a VM with no kiln open, so these are the
/// types plugin authors read. Declaring the store-backed behaviour alone
/// would promise a `boolean` that the idle VM never returns.
const SET: &str = "(entity_id: string, key: string, value: string) -> boolean?";
const GET: &str = "(entity_id: string, key: string) -> string?";
const LIST: &str = "(entity_id: string) -> { [string]: string }";
const FIND: &str = "(key: string, value: string) -> { string }";
const DELETE: &str = "(entity_id: string, key: string) -> boolean";

/// Register the storage module with stub functions that return nil/empty.
///
/// Called during executor setup. Stubs are replaced by
/// `register_storage_module_with_store` when a kiln opens and storage is available.
pub fn register_storage_module(lua: &Lua) -> Result<(), LuaError> {
    let mut storage = Ns::new(lua, "cru.storage")?;

    storage.async_func(
        "set",
        SET,
        |_, (_entity_id, _key, _value): (String, String, String)| async move { Ok(Value::Nil) },
    )?;

    storage.async_func(
        "get",
        GET,
        |_, (_entity_id, _key): (String, String)| async move { Ok(Value::Nil) },
    )?;

    storage.async_func("list", LIST, |lua, _entity_id: String| async move {
        Ok(Value::Table(lua.create_table()?))
    })?;

    storage.async_func("find", FIND, |lua, (_key, _value): (String, String)| async move {
        Ok(Value::Table(lua.create_table()?))
    })?;

    storage.async_func(
        "delete",
        DELETE,
        |_, (_entity_id, _key): (String, String)| async move { Ok(Value::Boolean(false)) },
    )?;

    storage.publish()?;
    Ok(())
}

/// Read the current plugin namespace from the VM's plugin context.
///
/// Returns `Err` when no plugin runs — a call from the user's own `init.lua`,
/// which owns no plugin namespace. The context lives in Rust-side app data, so
/// one plugin can no longer name another plugin's namespace
/// (see [`crate::plugin_context`]).
fn get_plugin_namespace(lua: &Lua) -> Result<String, mlua::Error> {
    let plugin_name = crate::plugin_context::current_plugin_name(lua).ok_or_else(|| {
        mlua::Error::runtime("cru.storage requires a plugin context (no plugin is running)")
    })?;
    Ok(format!("plugin:{}", plugin_name))
}

/// Convert a `StorageResult` into an `mlua::Result`, mapping storage errors to Lua runtime errors.
fn storage_err<T>(result: crucible_core::storage::StorageResult<T>) -> Result<T, mlua::Error> {
    result.map_err(|e| mlua::Error::runtime(format!("Storage error: {e}")))
}

/// Upgrade the storage module with a real PropertyStore backend.
///
/// The plugin namespace is determined dynamically from the VM's plugin context
/// at call time, so this only needs to be called once (not per-plugin).
pub fn register_storage_module_with_store(
    lua: &Lua,
    store: Arc<dyn PropertyStore>,
) -> Result<(), LuaError> {
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let storage: Table = cru.get("storage")?;
    // The table is already mounted on `cru`, so this replaces its five stubs
    // in place. Nothing publishes: a fresh table would leave a plugin that
    // captured `cru.storage` holding the stubs for ever.
    let mut storage = Ns::over(lua, "cru.storage", storage);

    let s = Arc::clone(&store);
    storage.async_func(
        "set",
        SET,
        move |lua, (entity_id, key, value): (String, String, String)| {
            let s = Arc::clone(&s);
            async move {
                let ns = get_plugin_namespace(&lua)?;
                storage_err(s.property_set(&entity_id, &ns, &key, &value).await)?;
                Ok(Value::Boolean(true))
            }
        },
    )?;

    let s = Arc::clone(&store);
    storage.async_func(
        "get",
        GET,
        move |lua, (entity_id, key): (String, String)| {
            let s = Arc::clone(&s);
            async move {
                let ns = get_plugin_namespace(&lua)?;
                match storage_err(s.property_get(&entity_id, &ns, &key).await)? {
                    Some(val) => Ok(Value::String(lua.create_string(&val)?)),
                    None => Ok(Value::Nil),
                }
            }
        },
    )?;

    let s = Arc::clone(&store);
    storage.async_func("list", LIST, move |lua, entity_id: String| {
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

    let s = Arc::clone(&store);
    storage.async_func("find", FIND, move |lua, (key, value): (String, String)| {
        let s = Arc::clone(&s);
        async move {
            let ns = get_plugin_namespace(&lua)?;
            let ids = storage_err(s.property_find(&ns, &key, &value).await)?;
            let table = lua.create_table()?;
            for (i, id) in ids.iter().enumerate() {
                table.set(i + 1, id.as_str())?; // Lua arrays are 1-indexed
            }
            Ok(Value::Table(table))
        }
    })?;

    let s = Arc::clone(&store);
    storage.async_func(
        "delete",
        DELETE,
        move |lua, (entity_id, key): (String, String)| {
            let s = Arc::clone(&s);
            async move {
                let ns = get_plugin_namespace(&lua)?;
                let deleted = storage_err(s.property_delete(&entity_id, &ns, &key).await)?;
                Ok(Value::Boolean(deleted))
            }
        },
    )?;

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
    use crate::test_support::{MemoryPropertyStore, TestLuaBuilder};

    fn setup_lua_with_store() -> mlua::Lua {
        let store: Arc<dyn PropertyStore> = Arc::new(MemoryPropertyStore::new());
        let lua = TestLuaBuilder::new().with_storage_store(store).build();
        // Set the plugin context so namespace resolution works. Lua cannot do
        // this: the context is Rust-side app data, which is the point.
        crate::plugin_context::set_owner(
            &lua,
            crate::plugin_context::Owner::Plugin("test-plugin".to_string()),
        );
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
        let store: Arc<dyn PropertyStore> = Arc::new(MemoryPropertyStore::new());
        let lua = TestLuaBuilder::new().with_storage_store(store).build();
        // Deliberately NOT setting a plugin context

        let result: Result<Value, _> = lua
            .load(r#"return cru.storage.get("e1", "k")"#)
            .eval_async()
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("plugin context"),
            "Error should mention the plugin context: {}",
            err_msg
        );
    }

    /// A plugin cannot take another plugin's storage namespace.
    ///
    /// The namespace used to come from `cru._current_plugin`, an ordinary
    /// writable global read at call time, so one assignment gave a plugin
    /// every other plugin's storage. The assignment is inert now: it writes a
    /// Lua global nothing reads.
    #[tokio::test]
    async fn a_plugin_cannot_forge_another_plugins_storage_namespace() {
        let store: Arc<dyn PropertyStore> = Arc::new(MemoryPropertyStore::new());
        let lua = TestLuaBuilder::new()
            .with_storage_store(Arc::clone(&store))
            .build();
        crate::plugin_context::set_owner(
            &lua,
            crate::plugin_context::Owner::Plugin("alpha".to_string()),
        );

        lua.load(
            r#"
            cru._current_plugin = "beta"
            cru.storage.set("e1", "k", "written-by-alpha")
            "#,
        )
        .exec_async()
        .await
        .expect("the write succeeds");

        // Assert the namespace, not an error: the write must land in alpha's.
        assert_eq!(
            store
                .property_get("e1", "plugin:alpha", "k")
                .await
                .expect("read alpha"),
            Some("written-by-alpha".to_string()),
            "the write must land in the running plugin's namespace"
        );
        assert_eq!(
            store
                .property_get("e1", "plugin:beta", "k")
                .await
                .expect("read beta"),
            None,
            "a forged `cru._current_plugin` must not reach another plugin's namespace"
        );
    }
}
