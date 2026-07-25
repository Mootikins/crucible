//! Per-session status published by plugins.
//!
//! `crucible.set_status{...}` gives a plugin a durable, session-scoped slot in
//! the UI. Before this, a plugin could only call `crucible.notify` — transient,
//! easily missed, and gone by the time it matters.
//!
//! That gap is why container isolation was unverifiable from the UI: a session
//! either was or wasn't sandboxed and nothing on screen said which. Status is
//! keyed so the chrome owner renders any plugin's slots generically, without
//! knowing what plugins exist.

use mlua::{Lua, Result as LuaResult, Table};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One status slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// Plugin that set it, so a stale slot can be attributed and cleared.
    pub plugin: String,
    /// Text to render. Short — this is a status slot, not a log.
    pub text: String,
    /// Severity, for the renderer to style. `info` unless stated.
    pub level: String,
}

/// Status slots per session, keyed within a session.
///
/// Written by the plugin Lua runtime, read by the RPC layer for TUI and web.
/// Same shape as the handler and isolation registries.
#[derive(Debug, Clone, Default)]
pub struct StatusRegistry {
    entries: Arc<Mutex<HashMap<String, HashMap<String, StatusEntry>>>>,
}

impl StatusRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, session_id: &str, key: &str, entry: StatusEntry) {
        if let Ok(mut g) = self.entries.lock() {
            g.entry(session_id.to_string())
                .or_default()
                .insert(key.to_string(), entry);
        }
    }

    /// Remove one slot. Setting empty text is *not* the same as clearing —
    /// a plugin that wants the slot gone should say so.
    pub fn clear(&self, session_id: &str, key: &str) {
        if let Ok(mut g) = self.entries.lock() {
            if let Some(session) = g.get_mut(session_id) {
                session.remove(key);
            }
        }
    }

    /// Every slot for a session, sorted by key so the render order is stable
    /// rather than hash order — a status bar that reshuffles on every update
    /// is worse than no status bar.
    pub fn get(&self, session_id: &str) -> Vec<(String, StatusEntry)> {
        let Ok(g) = self.entries.lock() else {
            return Vec::new();
        };
        let Some(session) = g.get(session_id) else {
            return Vec::new();
        };
        let mut out: Vec<_> = session
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Drop a session's slots. Called at session end so a finished session's
    /// status can't be shown against a live one.
    pub fn release(&self, session_id: &str) {
        if let Ok(mut g) = self.entries.lock() {
            g.remove(session_id);
        }
    }
}

/// Register `crucible.set_status` / `crucible.clear_status`.
///
/// ```lua
/// crucible.set_status{
///   session = session.id,
///   key     = "oci",
///   text    = "sandboxed: alpine:latest",
///   level   = "info",       -- info | warn | error
/// }
/// ```
pub fn register_status_module(
    lua: &Lua,
    crucible: &Table,
    registry: StatusRegistry,
) -> LuaResult<()> {
    let set_registry = registry.clone();
    let set_status = lua.create_function(move |_, opts: Table| {
        let session: String = opts.get("session").map_err(|_| {
            mlua::Error::runtime("crucible.set_status: `session` is required (use session.id)")
        })?;
        let key: String = opts
            .get("key")
            .map_err(|_| mlua::Error::runtime("crucible.set_status: `key` is required"))?;
        let text: String = opts
            .get("text")
            .map_err(|_| mlua::Error::runtime("crucible.set_status: `text` is required"))?;
        let plugin: String = opts.get("plugin").unwrap_or_else(|_| "unknown".to_string());
        let level: String = opts.get("level").unwrap_or_else(|_| "info".to_string());

        set_registry.set(
            &session,
            &key,
            StatusEntry {
                plugin,
                text,
                level,
            },
        );
        Ok(())
    })?;
    crucible.set("set_status", set_status)?;

    let clear_registry = registry;
    let clear_status = lua.create_function(move |_, opts: Table| {
        let session: String = opts
            .get("session")
            .map_err(|_| mlua::Error::runtime("crucible.clear_status: `session` is required"))?;
        let key: String = opts
            .get("key")
            .map_err(|_| mlua::Error::runtime("crucible.clear_status: `key` is required"))?;
        clear_registry.clear(&session, &key);
        Ok(())
    })?;
    crucible.set("clear_status", clear_status)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> StatusEntry {
        StatusEntry {
            plugin: "oci".to_string(),
            text: text.to_string(),
            level: "info".to_string(),
        }
    }

    #[test]
    fn slots_are_scoped_to_their_session() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", entry("sandboxed"));
        assert_eq!(reg.get("s1").len(), 1);
        assert!(
            reg.get("s2").is_empty(),
            "one session's status must not appear against another"
        );
    }

    #[test]
    fn setting_the_same_key_replaces_rather_than_appends() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", entry("starting"));
        reg.set("s1", "oci", entry("sandboxed"));
        let slots = reg.get("s1");
        assert_eq!(slots.len(), 1, "a key is a slot, not a log");
        assert_eq!(slots[0].1.text, "sandboxed");
    }

    #[test]
    fn slots_render_in_stable_key_order() {
        let reg = StatusRegistry::new();
        reg.set("s1", "zebra", entry("z"));
        reg.set("s1", "alpha", entry("a"));
        reg.set("s1", "middle", entry("m"));
        let keys: Vec<_> = reg.get("s1").into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec!["alpha", "middle", "zebra"],
            "unstable order makes a status bar reshuffle on every update"
        );
    }

    #[test]
    fn clearing_removes_only_the_named_slot() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", entry("sandboxed"));
        reg.set("s1", "other", entry("something"));
        reg.clear("s1", "oci");
        let keys: Vec<_> = reg.get("s1").into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["other"]);
    }

    #[test]
    fn releasing_a_session_drops_its_slots() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", entry("sandboxed"));
        reg.release("s1");
        assert!(
            reg.get("s1").is_empty(),
            "a finished session's status must not show against a live one"
        );
    }
}
