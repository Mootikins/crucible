//! Per-session status published by plugins.
//!
//! `cru.plugin.set_status{...}` gives a plugin a durable, session-scoped slot in
//! the UI. Before this, a plugin could only call `cru.log.notify` — transient,
//! easily missed, and gone by the time it matters.
//!
//! That gap is why container isolation was unverifiable from the UI: a session
//! either was or wasn't sandboxed and nothing on screen said which. Status is
//! keyed so the chrome owner renders any plugin's slots generically, without
//! knowing what plugins exist.

use crucible_core::status_color::{plugin_hue, StatusColorGroup};
use crucible_core::types::StatusDisplayItem;
use mlua::{Lua, Table};
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
    /// Named color resolved by each client through its active status theme.
    pub color_group: StatusColorGroup,
    /// Smaller values appear first. Pins are laid out separately by clients.
    pub priority: u8,
    /// Engine method selected by this item, if it is interactive.
    pub action: Option<String>,
    /// A pinned item must remain visible when the status strip overflows.
    pub pinned: bool,
    /// Progress of the work this slot describes, if it is work at all.
    ///
    /// `None` is a state ("sandboxed: alpine"), not a stalled bar.
    pub progress: Option<Progress>,
}

impl StatusEntry {
    pub fn display(&self, id: String) -> StatusDisplayItem {
        StatusDisplayItem {
            id,
            text: self.text.clone(),
            priority: self.priority,
            color_group: self.color_group.name().into(),
            action: self.action.clone(),
            pinned: self.pinned,
            plugin: self.plugin.clone(),
        }
    }
}

/// Status slots per session, keyed within a session.
///
/// Written by the plugin Lua runtime, read by the RPC layer for TUI and web.
/// Same shape as the handler and isolation registries.
#[derive(Debug, Clone, Default)]
pub struct StatusRegistry {
    entries: Arc<Mutex<HashMap<String, HashMap<String, StatusEntry>>>>,
    on_change: crate::host_hook::HostHook<crate::statusline_exprs::ChangeNotifier>,
}

impl StatusRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn set_change_notifier(&self, notifier: crate::statusline_exprs::ChangeNotifier) -> bool {
        self.on_change.install(notifier)
    }

    fn notify(&self, session_id: &str) {
        if let Some(notify) = self.on_change.get() {
            notify(session_id);
        }
    }

    pub fn set(&self, session_id: &str, key: &str, entry: StatusEntry) {
        let changed = if let Ok(mut g) = self.entries.lock() {
            g.entry(session_id.to_string())
                .or_default()
                .insert(key.to_string(), entry.clone())
                != Some(entry)
        } else {
            false
        };
        if changed {
            self.notify(session_id);
        }
    }

    /// Replace one session's complete authored list atomically.
    pub fn publish(&self, session_id: &str, items: Vec<(String, StatusEntry)>) {
        let changed = if let Ok(mut g) = self.entries.lock() {
            let replacement: HashMap<_, _> = items.into_iter().collect();
            g.insert(session_id.to_string(), replacement.clone()) != Some(replacement)
        } else {
            false
        };
        if changed {
            self.notify(session_id);
        }
    }

    /// Remove one slot. Setting empty text is *not* the same as clearing —
    /// a plugin that wants the slot gone should say so.
    pub fn clear(&self, session_id: &str, key: &str) {
        let removed = if let Ok(mut g) = self.entries.lock() {
            if let Some(session) = g.get_mut(session_id) {
                session.remove(key).is_some()
            } else {
                false
            }
        } else {
            false
        };
        if removed {
            self.notify(session_id);
        }
    }

    /// Every slot for a session, sorted by priority and key so the render order is stable
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
        out.sort_by(|a, b| a.1.priority.cmp(&b.1.priority).then_with(|| a.0.cmp(&b.0)));
        out
    }

    pub fn display_items(&self, session_id: &str) -> Vec<StatusDisplayItem> {
        self.get(session_id)
            .into_iter()
            .map(|(id, entry)| entry.display(id))
            .collect()
    }

    /// Drop a session's slots. Called at session end so a finished session's
    /// status can't be shown against a live one.
    pub fn release(&self, session_id: &str) {
        if let Ok(mut g) = self.entries.lock() {
            g.remove(session_id);
        }
    }
}

/// Register `cru.plugin.set_status` / `cru.plugin.clear_status`.
///
/// ```lua
/// cru.plugin.set_status{
///   session = session.id,
///   key     = "oci",
///   text    = "sandboxed: alpine:latest",
///   level   = "info",       -- info | warn | error
/// }
/// ```
pub fn register_status_module(
    lua: &Lua,
    registry: StatusRegistry,
) -> Result<(), crate::error::LuaError> {
    // `cru.plugin` already carries members other modules registered, so the
    // namespace opens OVER the existing table and never publishes a fresh one.
    let plugin = crate::lua_util::get_or_create_module(lua, "plugin")?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.plugin", plugin);

    // One options TABLE, not a string: `session`, `key` and `text` are
    // required — the closure raises a named error for each — while `plugin`,
    // `level`, `color` and `progress` have defaults. `progress` is `true` for
    // indeterminate or a fraction, so it is neither boolean nor number alone.
    let set_registry = registry.clone();
    ns.func(
        "set_status",
        "(status: { session: string, key: string, text: string, plugin: string?, \
         level: string?, color: string?, progress: (boolean | number)? }) -> ()",
        move |_, opts: Table| {
            let session: String = opts.get("session").map_err(|_| {
                mlua::Error::runtime(
                    "cru.plugin.set_status: `session` is required (use session.id)",
                )
            })?;
            let key: String = opts
                .get("key")
                .map_err(|_| mlua::Error::runtime("cru.plugin.set_status: `key` is required"))?;
            let text: String = opts
                .get("text")
                .map_err(|_| mlua::Error::runtime("cru.plugin.set_status: `text` is required"))?;
            let plugin: String = opts.get("plugin").unwrap_or_else(|_| "unknown".to_string());
            let level: String = opts.get("level").unwrap_or_else(|_| "info".to_string());
            let color_group = match opts.get::<String>("color") {
                Ok(name) => StatusColorGroup::from_name(&name),
                Err(_) => match level.as_str() {
                    "warn" | "warning" => StatusColorGroup::Warn,
                    "error" | "danger" => StatusColorGroup::Danger,
                    "ok" | "success" => StatusColorGroup::Ok,
                    _ => plugin_hue(&plugin),
                },
            };
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
                    color_group,
                    priority: 128,
                    action: None,
                    pinned: false,
                    progress,
                },
            );
            Ok(())
        },
    )?;

    // `session` and `key` name the slot; nothing else is read. Setting empty
    // text is *not* the same as clearing, which is why this exists.
    let publish_registry = registry.clone();
    let clear_registry = registry;
    ns.func(
        "clear_status",
        "(slot: { session: string, key: string }) -> ()",
        move |_, opts: Table| {
            let session: String = opts.get("session").map_err(|_| {
                mlua::Error::runtime("cru.plugin.clear_status: `session` is required")
            })?;
            let key: String = opts
                .get("key")
                .map_err(|_| mlua::Error::runtime("cru.plugin.clear_status: `key` is required"))?;
            clear_registry.clear(&session, &key);
            Ok(())
        },
    )?;

    // A single list authoring surface for the TUI and web. `item` keeps Lua
    // config readable; `publish` validates the whole replacement before it
    // reaches the registry, so a malformed list never erases the old one.
    let statusline = crate::lua_util::get_or_create_module(lua, "statusline")?;
    let mut sl = crate::host_registry::Ns::over(lua, "cru.statusline", statusline);
    sl.func(
        "item",
        "(item: { id: string, text: string, priority: number?, color: string?, action: string?, pinned: boolean?, plugin: string? }) -> any",
        |_, item: Table| Ok(item),
    )?;
    sl.func(
        "publish",
        "(session: string, items: any) -> ()",
        move |_, (session, items): (String, Table)| {
            let mut published = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for value in items.sequence_values::<Table>() {
                let item = value?;
                let id: String = item
                    .get("id")
                    .map_err(|_| mlua::Error::runtime("status item id is required"))?;
                if id.is_empty() || !seen.insert(id.clone()) {
                    return Err(mlua::Error::runtime(format!(
                        "duplicate or empty status item id: {id}"
                    )));
                }
                let raw_text: String = item
                    .get("text")
                    .map_err(|_| mlua::Error::runtime("status item text is required"))?;
                let text = crate::statusline_exprs::sanitize_uncapped(&raw_text)
                    .chars()
                    .take(50)
                    .collect();
                let plugin: String = item.get("plugin").unwrap_or_else(|_| "unknown".into());
                let color_group = item
                    .get::<String>("color")
                    .map(|name| StatusColorGroup::from_name(&name))
                    .unwrap_or_else(|_| plugin_hue(&plugin));
                let priority = item.get::<i64>("priority").unwrap_or(128).clamp(0, 255) as u8;
                let action: Option<String> = item.get("action").ok();
                let pinned = item.get::<bool>("pinned").unwrap_or(false)
                    || matches!(action.as_deref(), Some("plugin_approval" | "plugin_turn"));
                published.push((
                    id,
                    StatusEntry {
                        plugin,
                        text,
                        level: "info".into(),
                        color_group,
                        priority,
                        action,
                        pinned,
                        progress: None,
                    },
                ));
            }
            publish_registry.publish(&session, published);
            Ok(())
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::status_color::{plugin_hue, StatusColorGroup};

    #[test]
    fn plugin_status_uses_named_color_groups() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        lua.load(
            r#"cru.plugin.set_status{ session="s1", key="a", plugin="goal", text="running" }"#,
        )
        .exec()
        .unwrap();
        assert_eq!(reg.get("s1")[0].1.color_group, plugin_hue("goal"));
        lua.load(r#"cru.plugin.set_status{ session="s1", key="a", plugin="goal", text="running", color="warn" }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.color_group, StatusColorGroup::Warn);
        lua.load(r#"cru.plugin.set_status{ session="s1", key="a", plugin="goal", text="running", color="unknown" }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.color_group, StatusColorGroup::Info);
    }

    #[test]
    fn lua_publishes_one_ordered_status_list_and_forces_control_pins() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        lua.load(
            r#"
            local sl = cru.statusline
            sl.publish("s1", {
              sl.item{ id="later", text="later", priority=40, plugin="weather" },
              sl.item{ id="approval", text="goal · ask", priority=20,
                       color="warn", action="plugin_approval", pinned=false },
              sl.item{ id="first", text="first", priority=10, color="hue-3" },
            })
        "#,
        )
        .exec()
        .unwrap();
        let items = reg.get("s1");
        assert_eq!(
            items.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["first", "approval", "later"]
        );
        assert_eq!(items[0].1.priority, 10);
        assert_eq!(items[0].1.color_group, StatusColorGroup::Hue3);
        assert!(items[1].1.pinned);
        assert_eq!(items[1].1.action.as_deref(), Some("plugin_approval"));
        lua.load(
            r#"cru.statusline.publish("s1", { cru.statusline.item{ id="new", text="new" } })"#,
        )
        .exec()
        .unwrap();
        assert_eq!(reg.get("s1").len(), 1, "publish replaces the whole list");
        assert!(reg.get("other").is_empty());
    }

    #[test]
    fn status_changes_notify_the_session_after_the_list_is_visible() {
        let reg = StatusRegistry::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let notices = seen.clone();
        let reader = reg.clone();
        assert!(reg.set_change_notifier(std::sync::Arc::new(move |session| {
            notices
                .lock()
                .unwrap()
                .push((session.to_owned(), reader.get(session).len()));
        })));
        reg.publish("s1", vec![("one".into(), entry("one"))]);
        reg.clear("s1", "one");
        assert_eq!(*seen.lock().unwrap(), [("s1".into(), 1), ("s1".into(), 0)]);
    }

    fn entry(text: &str) -> StatusEntry {
        StatusEntry {
            plugin: "oci".to_string(),
            text: text.to_string(),
            level: "info".to_string(),
            color_group: plugin_hue("oci"),
            priority: 128,
            action: None,
            pinned: false,
            progress: None,
        }
    }

    fn lua_with_status(reg: StatusRegistry) -> Lua {
        let lua = Lua::new();
        register_status_module(&lua, reg).unwrap();
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
            r#"cru.plugin.set_status{ session="s1", key="build", text="building", progress=true }"#,
        )
        .exec()
        .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Indeterminate));

        lua.load(
            r#"cru.plugin.set_status{ session="s1", key="build", text="pulling", progress=0.25 }"#,
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
        lua.load(r#"cru.plugin.set_status{ session="s1", key="oci", text="sandboxed: alpine" }"#)
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
        lua.load(r#"cru.plugin.set_status{ session="s1", key="k", text="t", progress=4.2 }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.progress, Some(Progress::Fraction(1.0)));

        lua.load(r#"cru.plugin.set_status{ session="s1", key="k", text="t", progress=-1 }"#)
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
