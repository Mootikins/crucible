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

/// How far along a slot's work is, when it is work rather than a state.
///
/// Modelled on LSP `$/progress`, where the server reports and the *client*
/// decides how to render — a spinner, a bar, a toast. The slot key is already
/// the token there, so `set_status` is begin-and-report and `clear_status` is
/// end; no new lifecycle is needed.
///
/// Both cases are real and neither substitutes for the other: an image pull
/// knows its fraction, an image build does not, and reporting a fake fraction
/// for the second is worse than admitting it is unknown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Progress {
    /// Work is underway with no meaningful fraction — render a spinner.
    Indeterminate,
    /// Fraction complete, clamped to 0.0..=1.0.
    Fraction(f64),
}

/// One status slot.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusEntry {
    /// Plugin that set it, so a stale slot can be attributed and cleared.
    pub plugin: String,
    /// Text to render. Short — this is a status slot, not a log.
    pub text: String,
    /// Severity, for the renderer to style. `info` unless stated.
    pub level: String,
    /// Progress of the work this slot describes, if it is work at all.
    ///
    /// `None` is a state ("sandboxed: alpine"), not a stalled bar.
    pub progress: Option<Progress>,
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
        // `progress = true` is indeterminate; a number is a fraction. Out of
        // range is clamped rather than refused: a plugin miscounting steps
        // should show a full bar, not fail the operation it is reporting on.
        let progress = match opts.get::<mlua::Value>("progress") {
            Ok(mlua::Value::Boolean(true)) => Some(Progress::Indeterminate),
            Ok(mlua::Value::Number(n)) => Some(Progress::Fraction(n.clamp(0.0, 1.0))),
            Ok(mlua::Value::Integer(n)) => Some(Progress::Fraction((n as f64).clamp(0.0, 1.0))),
            _ => None,
        };

        set_registry.set(
            &session,
            &key,
            StatusEntry {
                plugin,
                text,
                level,
                progress,
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
            progress: None,
        }
    }

    fn lua_with_status(reg: StatusRegistry) -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        register_status_module(&lua, &crucible, reg).unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    /// A slot describing work that takes minutes — an image build — is the
    /// case the status API could not express: it could say "building" and then
    /// nothing until it finished.
    #[test]
    fn a_slot_can_report_an_indeterminate_or_fractional_progress() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());

        lua.load(
            r#"crucible.set_status{ session="s1", key="build", text="building", progress=true }"#,
        )
        .exec()
        .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Indeterminate));

        lua.load(
            r#"crucible.set_status{ session="s1", key="build", text="pulling", progress=0.25 }"#,
        )
        .exec()
        .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Fraction(0.25)));
    }

    /// A state is not stalled work; omitting progress must not render a bar.
    #[test]
    fn a_slot_without_progress_reports_none() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        lua.load(r#"crucible.set_status{ session="s1", key="oci", text="sandboxed: alpine" }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, None);
    }

    /// A plugin miscounting its steps should show a full bar, not fail the
    /// operation it is reporting on.
    #[test]
    fn an_out_of_range_fraction_is_clamped_rather_than_refused() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        lua.load(r#"crucible.set_status{ session="s1", key="k", text="t", progress=4.2 }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Fraction(1.0)));

        lua.load(r#"crucible.set_status{ session="s1", key="k", text="t", progress=-1 }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Fraction(0.0)));
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
