//! Test support utilities for crucible-lua
//!
//! Provides a builder pattern for constructing Lua test environments
//! with specific module registrations.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use crucible_core::storage::{NoteStore, PropertyStore, StorageResult};
use mlua::{Lua, Table};

use crate::notify::register_notify_module;
use crate::{
    register_hooks_module, register_oil_module, register_oq_module, register_session_module,
    register_sessions_module, register_sessions_module_with_api, register_storage_module,
    register_storage_module_with_store, register_tools_module, register_tools_module_with_api,
    register_ui_module, register_ui_module_with_api, register_vault_module,
    register_vault_module_with_store, register_vault_module_with_store_scoped, CurrentSession,
    DaemonSessionApi, DaemonToolsApi,
};

/// Builder for constructing Lua test environments with specific module registrations.
///
/// Each `with_*` method registers the corresponding module, setting up any required
/// globals (the cru table) automatically.
///
/// # Examples
///
/// ```ignore
/// let lua = TestLuaBuilder::new().with_oil().build();
/// let lua = TestLuaBuilder::new().with_vault().build();
/// let (lua, hooks) = TestLuaBuilder::new().build_with_hooks();
/// ```
pub struct TestLuaBuilder {
    lua: Lua,
}

impl TestLuaBuilder {
    pub fn new() -> Self {
        Self { lua: Lua::new() }
    }

    fn ensure_cru_table(&self) {
        let globals = self.lua.globals();
        if !globals.contains_key("cru").unwrap() {
            globals
                .set("cru", self.lua.create_table().unwrap())
                .unwrap();
        }
    }

    /// Register the oil module (cru.oil).
    /// Sets up: cru global table.
    pub fn with_oil(self) -> Self {
        self.ensure_cru_table();
        register_oil_module(&self.lua).expect("Should register oil module");
        self
    }

    /// Register the vault module (cru.kiln).
    /// Sets up: cru global table.
    pub fn with_vault(self) -> Self {
        self.ensure_cru_table();
        register_vault_module(&self.lua).expect("Should register vault module");
        crate::register_embed_module(&self.lua).expect("Should register cru.embed");
        self
    }

    /// Register the vault module with a NoteStore backend.
    /// Sets up: cru global table.
    pub fn with_vault_store(self, store: Arc<dyn NoteStore>) -> Self {
        self.ensure_cru_table();
        register_vault_module_with_store(&self.lua, store).expect("Should register vault module");
        self
    }

    /// Register the vault module with a NoteStore backend and an explicit
    /// authority — the shape the daemon wires (`upgrade_with_storage` derives
    /// the authority from the kiln path). Use this over `with_vault_store`
    /// when the test cares about scope; the unscoped wrapper passes an empty
    /// workspace path, which every unstamped note is visible to.
    pub fn with_vault_store_scoped(
        self,
        store: Arc<dyn NoteStore>,
        authority: crucible_core::storage::Scope,
    ) -> Self {
        self.ensure_cru_table();
        register_vault_module_with_store_scoped(&self.lua, store, authority)
            .expect("Should register scoped vault module");
        self
    }

    /// Register the stdlib module.
    /// Sets up: cru namespace with mock log and timer.
    pub fn with_stdlib(self) -> Self {
        self.lua.load("cru = cru or {}").exec().unwrap();
        self.lua
            .load(r#"cru.log = function(level, msg) end"#)
            .exec()
            .unwrap();
        self.lua
            .load(r#"cru.timer = { sleep = function(secs) end }"#)
            .exec()
            .unwrap();
        crate::register_prelude(&self.lua).unwrap();
        crate::register_test_harness(&self.lua).unwrap();
        self
    }

    /// Register the json_query (oq) module.
    pub fn with_json_query(self) -> Self {
        register_oq_module(&self.lua).unwrap();
        self
    }

    /// Register the sessions module (cru.session).
    /// Sets up: cru global table.
    pub fn with_sessions(self) -> Self {
        self.ensure_cru_table();
        register_sessions_module(&self.lua).expect("Should register sessions module");
        self
    }

    /// Register the sessions module with a DaemonSessionApi backend.
    /// Sets up: cru global table.
    pub fn with_sessions_api(self, api: Arc<dyn DaemonSessionApi>) -> Self {
        self.ensure_cru_table();
        register_sessions_module_with_api(&self.lua, api)
            .expect("Should register sessions with API");
        self
    }

    /// Register the ui module (cru.ui) with stubs.
    /// Sets up: cru global table.
    pub fn with_ui(self) -> Self {
        self.ensure_cru_table();
        register_ui_module(&self.lua).expect("Should register ui module");
        self
    }

    /// Register the ui module with a DaemonSessionApi backend.
    /// Sets up: cru global table.
    pub fn with_ui_api(self, api: Arc<dyn DaemonSessionApi>) -> Self {
        self.ensure_cru_table();
        register_ui_module_with_api(&self.lua, api).expect("Should register ui with API");
        self
    }

    /// Register the storage module (cru.storage) with stubs.
    /// Sets up: cru global table.
    pub fn with_storage(self) -> Self {
        self.ensure_cru_table();
        register_storage_module(&self.lua).expect("Should register storage module");
        self
    }

    /// Register the storage module with a PropertyStore backend.
    /// Sets up: cru global table.
    pub fn with_storage_store(self, store: Arc<dyn PropertyStore>) -> Self {
        self.ensure_cru_table();
        register_storage_module(&self.lua).expect("Should register storage stubs");
        register_storage_module_with_store(&self.lua, store)
            .expect("Should register storage with store");
        self
    }

    /// Register the tools module (cru.tools).
    /// Sets up: cru global table.
    pub fn with_tools(self) -> Self {
        self.ensure_cru_table();
        register_tools_module(&self.lua).expect("Should register tools module");
        self
    }

    /// Register the tools module with a DaemonToolsApi backend.
    /// Sets up: cru global table.
    pub fn with_tools_api(self, api: Arc<dyn DaemonToolsApi>) -> Self {
        self.ensure_cru_table();
        register_tools_module_with_api(&self.lua, api).expect("Should register tools with API");
        self
    }

    /// Build the Lua instance.
    pub fn build(self) -> Lua {
        self.lua
    }

    /// Build with the notify module, returning (Lua, crucible_table).
    /// Sets up: crucible table with log subtable.
    pub fn build_with_notify(self) -> (Lua, Table) {
        let cru = self.lua.create_table().unwrap();
        register_notify_module(&self.lua, &cru).unwrap();
        self.lua.globals().set("cru", cru.clone()).unwrap();
        (self.lua, cru)
    }

    /// Build with the hooks module, returning (Lua, cru_table).
    pub fn build_with_hooks(self) -> (Lua, Table) {
        let cru = self.lua.create_table().unwrap();
        self.lua.globals().set("cru", cru.clone()).unwrap();
        register_hooks_module(&self.lua, &cru).unwrap();
        (self.lua, cru)
    }

    /// Build with the current-session holder, returning (Lua, CurrentSession).
    /// Sets up: cru global table.
    pub fn build_with_current_session(self) -> (Lua, CurrentSession) {
        self.ensure_cru_table();
        let mgr = register_session_module(&self.lua).unwrap();
        (self.lua, mgr)
    }
}

/// An in-memory `PropertyStore` for tests.
pub struct MemoryPropertyStore {
    data: Mutex<HashMap<(String, String, String), String>>,
}

impl MemoryPropertyStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PropertyStore for MemoryPropertyStore {
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
