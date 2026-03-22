//! Test support utilities for crucible-lua
//!
//! Provides a builder pattern for constructing Lua test environments
//! with specific module registrations.

use std::sync::Arc;

use crucible_core::storage::{GraphView, NoteStore, PropertyStore};
use mlua::{Lua, Table};

use crate::notify::register_notify_module;
use crate::{
    register_graph_module, register_hooks_module, register_lua_stdlib, register_oil_module,
    register_oq_module, register_session_module, register_sessions_module,
    register_sessions_module_with_api, register_statusline_module, register_storage_module,
    register_storage_module_with_store, register_tools_module, register_tools_module_with_api,
    register_vault_module, register_vault_module_with_graph, register_vault_module_with_store,
    DaemonSessionApi, DaemonToolsApi, SessionManager,
};

/// Builder for constructing Lua test environments with specific module registrations.
///
/// Each `with_*` method registers the corresponding module, setting up any required
/// globals (cru/crucible tables) automatically.
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

    fn ensure_crucible_table(&self) {
        let globals = self.lua.globals();
        if !globals.contains_key("crucible").unwrap() {
            globals
                .set("crucible", self.lua.create_table().unwrap())
                .unwrap();
        }
    }

    /// Register the oil module (cru.oil).
    /// Sets up: crucible global table.
    pub fn with_oil(self) -> Self {
        self.ensure_crucible_table();
        register_oil_module(&self.lua).expect("Should register oil module");
        self
    }

    /// Register the vault module (cru.kiln).
    /// Sets up: cru + crucible global tables.
    pub fn with_vault(self) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_vault_module(&self.lua).expect("Should register vault module");
        self
    }

    /// Register the vault module with a NoteStore backend.
    /// Sets up: cru + crucible global tables.
    pub fn with_vault_store(self, store: Arc<dyn NoteStore>) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_vault_module_with_store(&self.lua, store).expect("Should register vault module");
        self
    }

    /// Register the vault module with graph support.
    /// Sets up: cru + crucible global tables, vault + graph modules.
    pub fn with_vault_graph(self, view: Arc<dyn GraphView>) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_vault_module(&self.lua).expect("Should register vault module");
        register_vault_module_with_graph(&self.lua, view).expect("Should register graph functions");
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
        register_lua_stdlib(&self.lua).unwrap();
        self
    }

    /// Register the json_query (oq) module.
    pub fn with_json_query(self) -> Self {
        register_oq_module(&self.lua).unwrap();
        self
    }

    /// Register the statusline module.
    pub fn with_statusline(self) -> Self {
        register_statusline_module(&self.lua).expect("Should register statusline module");
        self
    }

    /// Register the sessions module (cru.sessions).
    /// Sets up: cru + crucible global tables.
    pub fn with_sessions(self) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_sessions_module(&self.lua).expect("Should register sessions module");
        self
    }

    /// Register the sessions module with a DaemonSessionApi backend.
    /// Sets up: cru + crucible global tables.
    pub fn with_sessions_api(self, api: Arc<dyn DaemonSessionApi>) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_sessions_module_with_api(&self.lua, api)
            .expect("Should register sessions with API");
        self
    }

    /// Register the storage module (cru.storage) with stubs.
    /// Sets up: cru + crucible global tables.
    pub fn with_storage(self) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_storage_module(&self.lua).expect("Should register storage module");
        self
    }

    /// Register the storage module with a PropertyStore backend.
    /// Sets up: cru + crucible global tables.
    pub fn with_storage_store(self, store: Arc<dyn PropertyStore>) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_storage_module(&self.lua).expect("Should register storage stubs");
        register_storage_module_with_store(&self.lua, store)
            .expect("Should register storage with store");
        self
    }

    /// Register the graph module.
    pub fn with_graph(self) -> Self {
        register_graph_module(&self.lua).unwrap();
        self
    }

    /// Register the tools module (cru.tools).
    /// Sets up: cru + crucible global tables.
    pub fn with_tools(self) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
        register_tools_module(&self.lua).expect("Should register tools module");
        self
    }

    /// Register the tools module with a DaemonToolsApi backend.
    /// Sets up: cru + crucible global tables.
    pub fn with_tools_api(self, api: Arc<dyn DaemonToolsApi>) -> Self {
        self.ensure_cru_table();
        self.ensure_crucible_table();
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
        let crucible = self.lua.create_table().unwrap();
        let log_table = self.lua.create_table().unwrap();
        crucible.set("log", log_table).unwrap();
        register_notify_module(&self.lua, &crucible).unwrap();
        self.lua
            .globals()
            .set("crucible", crucible.clone())
            .unwrap();
        (self.lua, crucible)
    }

    /// Build with the hooks module, returning (Lua, crucible_table).
    pub fn build_with_hooks(self) -> (Lua, Table) {
        let crucible = self.lua.create_table().unwrap();
        self.lua
            .globals()
            .set("crucible", crucible.clone())
            .unwrap();
        register_hooks_module(&self.lua, &crucible).unwrap();
        (self.lua, crucible)
    }

    /// Build with the session manager, returning (Lua, SessionManager).
    /// Sets up: crucible + cru global tables.
    pub fn build_with_session_manager(self) -> (Lua, SessionManager) {
        self.ensure_crucible_table();
        self.ensure_cru_table();
        let mgr = register_session_module(&self.lua).unwrap();
        (self.lua, mgr)
    }
}
