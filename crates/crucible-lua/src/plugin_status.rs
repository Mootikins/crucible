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
use crucible_core::types::{
    StatusDisplayItem, StatusProgress, PLUGIN_APPROVAL_ACTION, PLUGIN_TURNS_ID_PREFIX,
};
use mlua::{Lua, Table};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

/// One status slot.
#[derive(Debug, Clone, PartialEq)]
pub struct StatusEntry {
    /// Plugin that set it, so a stale slot can be attributed and cleared.
    pub plugin: String,
    /// Text to render. Short — this is a status slot, not a log.
    pub text: String,
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
    pub progress: Option<StatusProgress>,
}

impl StatusEntry {
    pub fn display(&self, id: String) -> StatusDisplayItem {
        StatusDisplayItem {
            id,
            text: self.text.clone(),
            priority: self.priority,
            color_group: self.color_group,
            action: self.action.clone(),
            pinned: self.pinned,
            plugin: self.plugin.clone(),
            kind: crucible_core::types::StatusItemKind::Published,
            progress: self.progress,
        }
    }
}

/// Status items per session, one list for each author, keyed within it.
///
/// The author is the Lua source that runs the call: a plugin's name, or
/// `init.lua`, `builtin` or `lua.eval`. A plugin names no author itself, so
/// it can neither write into another plugin's list nor erase it. Written by
/// the plugin Lua runtime, read by the RPC layer for TUI and web.
#[derive(Debug, Clone, Default)]
pub struct StatusRegistry {
    entries: Arc<Mutex<HashMap<String, SessionLists>>>,
    on_change: crate::host_hook::HostHook<crate::statusline_exprs::ChangeNotifier>,
}

/// The lists of one session: author, then item key.
type SessionLists = BTreeMap<String, BTreeMap<String, StatusEntry>>;

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

    /// Change the lists of `session_id`, and notify when `change` says it
    /// changed something. The lock is free before the notifier runs, because
    /// the notifier reads the lists.
    fn change(&self, session_id: &str, change: impl FnOnce(&mut SessionLists) -> bool) {
        let changed = match self.entries.lock() {
            Ok(mut g) => change(g.entry(session_id.to_string()).or_default()),
            Err(_) => false,
        };
        if changed {
            self.notify(session_id);
        }
    }

    /// Set one item of `author`'s list.
    pub fn set(&self, session_id: &str, author: &str, key: &str, entry: StatusEntry) {
        self.change(session_id, |lists| {
            lists
                .entry(author.to_string())
                .or_default()
                .insert(key.to_string(), entry.clone())
                != Some(entry)
        });
    }

    /// Replace `author`'s whole list in one step. The lists of other authors
    /// stay as they are.
    pub fn publish(&self, session_id: &str, author: &str, items: Vec<(String, StatusEntry)>) {
        self.change(session_id, |lists| {
            let replacement: BTreeMap<_, _> = items.into_iter().collect();
            let previous = if replacement.is_empty() {
                lists.remove(author)
            } else {
                lists.insert(author.to_string(), replacement.clone())
            };
            previous.unwrap_or_default() != replacement
        });
    }

    /// Remove one item of `author`'s list. Setting empty text is *not* the
    /// same as clearing — a plugin that wants the slot gone should say so.
    pub fn clear(&self, session_id: &str, author: &str, key: &str) {
        self.change(session_id, |lists| {
            lists
                .get_mut(author)
                .is_some_and(|list| list.remove(key).is_some())
        });
    }

    /// Every item of a session, as `(id, entry)`. The id is `author/key`,
    /// so two authors that use one key stay two items. Sorted by priority
    /// and id so the render order is stable rather than hash order — a
    /// status bar that reshuffles on every update is worse than no status
    /// bar.
    pub fn get(&self, session_id: &str) -> Vec<(String, StatusEntry)> {
        let Ok(g) = self.entries.lock() else {
            return Vec::new();
        };
        let Some(lists) = g.get(session_id) else {
            return Vec::new();
        };
        let mut out: Vec<_> = lists
            .iter()
            .flat_map(|(author, list)| {
                list.iter()
                    .map(move |(key, entry)| (format!("{author}/{key}"), entry.clone()))
            })
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

    /// Drop every list of `author` in every session, and notify each session
    /// that lost items. Called when a plugin goes inert: an item that
    /// outlives its plugin stays painted in every attached client.
    pub fn release_plugin(&self, author: &str) {
        let sessions: Vec<String> = match self.entries.lock() {
            Ok(mut g) => g
                .iter_mut()
                .filter_map(|(session, lists)| lists.remove(author).map(|_| session.clone()))
                .collect(),
            Err(_) => Vec::new(),
        };
        for session in sessions {
            self.notify(&session);
        }
    }

    /// Drop a session's slots. Called at session end so a finished session's
    /// status can't be shown against a live one.
    pub fn release(&self, session_id: &str) {
        if let Ok(mut g) = self.entries.lock() {
            g.remove(session_id);
        }
    }
}

/// The author of a status call: the Lua source that runs it.
fn author(lua: &Lua) -> String {
    crate::plugin_context::current_source(lua).to_string()
}

/// Refuse a key or an action that belongs to the engine's plugin-turn items.
fn refuse_engine_names(function: &str, key: &str, action: Option<&str>) -> mlua::Result<()> {
    if key.starts_with(PLUGIN_TURNS_ID_PREFIX) {
        return Err(mlua::Error::runtime(format!(
            "{function}: the id prefix `{PLUGIN_TURNS_ID_PREFIX}` is reserved for the engine"
        )));
    }
    if action == Some(PLUGIN_APPROVAL_ACTION) {
        return Err(mlua::Error::runtime(format!(
            "{function}: the action `{PLUGIN_APPROVAL_ACTION}` is reserved for the engine"
        )));
    }
    Ok(())
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
    // required — the closure raises a named error for each — while `level`,
    // `color` and `progress` have defaults. The running source is the
    // author; the table cannot name one. `progress` is `true` for
    // indeterminate or a fraction, so it is neither boolean nor number alone.
    let set_registry = registry.clone();
    ns.func(
        "set_status",
        "(status: { session: string, key: string, text: string, \
         level: string?, color: string?, progress: (boolean | number)? }) -> ()",
        move |lua, opts: Table| {
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
            refuse_engine_names("cru.plugin.set_status", &key, None)?;
            let plugin = author(lua);
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
                Ok(mlua::Value::Boolean(true)) => Some(StatusProgress::INDETERMINATE),
                Ok(mlua::Value::Number(n)) => Some(StatusProgress::Fraction(n.clamp(0.0, 1.0))),
                Ok(mlua::Value::Integer(n)) => {
                    Some(StatusProgress::Fraction((n as f64).clamp(0.0, 1.0)))
                }
                _ => None,
            };

            set_registry.set(
                &session,
                &plugin.clone(),
                &key,
                StatusEntry {
                    plugin,
                    text,
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
        move |lua, opts: Table| {
            let session: String = opts.get("session").map_err(|_| {
                mlua::Error::runtime("cru.plugin.clear_status: `session` is required")
            })?;
            let key: String = opts
                .get("key")
                .map_err(|_| mlua::Error::runtime("cru.plugin.clear_status: `key` is required"))?;
            clear_registry.clear(&session, &author(lua), &key);
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
        "(item: { id: string, text: string, priority: number?, color: string?, action: string?, pinned: boolean? }) -> any",
        |_, item: Table| Ok(item),
    )?;
    sl.func(
        "publish",
        "(session: string, items: any) -> ()",
        move |lua, (session, items): (String, Table)| {
            let plugin = author(lua);
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
                let color_group = item
                    .get::<String>("color")
                    .map(|name| StatusColorGroup::from_name(&name))
                    .unwrap_or_else(|_| plugin_hue(&plugin));
                let priority = item.get::<i64>("priority").unwrap_or(128).clamp(0, 255) as u8;
                let action: Option<String> = item.get("action").ok();
                refuse_engine_names("cru.statusline.publish", &id, action.as_deref())?;
                // Lua decides `pinned` for its own items. The engine pins the
                // plugin-turn items (`ask`, `stop`, a running turn) itself,
                // from the approval knob; an action name here says nothing
                // about that state.
                let pinned = item.get::<bool>("pinned").unwrap_or(false);
                published.push((
                    id,
                    StatusEntry {
                        plugin: plugin.clone(),
                        text,
                        color_group,
                        priority,
                        action,
                        pinned,
                        progress: None,
                    },
                ));
            }
            publish_registry.publish(&session, &plugin, published);
            Ok(())
        },
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::status_color::{plugin_hue, StatusColorGroup};

    /// Publish `text` as the one item of `plugin`, with the context that the
    /// loader enters when that plugin's code runs.
    fn publish_as(lua: &Lua, plugin: &str, item: &str) -> mlua::Result<()> {
        let previous = crate::plugin_context::enter_plugin(lua, plugin);
        let result = lua
            .load(format!(
                r#"cru.statusline.publish("s1", {{ cru.statusline.item{item} }})"#
            ))
            .exec();
        crate::plugin_context::set_source(lua, previous);
        result
    }

    /// Two plugins publish in one session. A publish replaces only the list
    /// of the plugin that runs, and the item names that plugin, not the
    /// `plugin` field that the item declares.
    #[test]
    fn a_publish_replaces_only_the_list_of_the_running_plugin() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        publish_as(
            &lua,
            "sync",
            r#"{ id = "state", text = "sync idle", plugin = "forged" }"#,
        )
        .unwrap();
        publish_as(&lua, "index", r#"{ id = "state", text = "index ready" }"#).unwrap();
        publish_as(&lua, "sync", r#"{ id = "state", text = "sync busy" }"#).unwrap();

        let mut seen: Vec<_> = reg
            .display_items("s1")
            .into_iter()
            .map(|item| (item.plugin, item.text))
            .collect();
        seen.sort();
        assert_eq!(
            seen,
            [
                ("index".to_string(), "index ready".to_string()),
                ("sync".to_string(), "sync busy".to_string()),
            ]
        );
    }

    /// The engine's plugin-turn item is the engine's. A plugin cannot
    /// publish its id prefix or its action, so no plugin item can copy an
    /// `ask` or a `stop` that the knob does not hold.
    #[test]
    fn a_plugin_cannot_publish_the_engine_item() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        for item in [
            r#"{ id = "plugin_turns:goal", text = "goal · ask" }"#,
            r#"{ id = "ask", text = "goal · ask", action = "plugin_approval", pinned = true }"#,
        ] {
            let error = publish_as(&lua, "goal", item).expect_err(item).to_string();
            assert!(error.contains("reserved for the engine"), "{error}");
        }
        assert!(reg.display_items("s1").is_empty());
        let key = lua
            .load(
                r#"cru.plugin.set_status{ session = "s1", key = "plugin_turns:goal", text = "x" }"#,
            )
            .exec()
            .expect_err("the prefix is reserved in set_status too")
            .to_string();
        assert!(key.contains("reserved for the engine"), "{key}");
    }

    #[test]
    fn plugin_status_uses_named_color_groups() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        let previous = crate::plugin_context::enter_plugin(&lua, "goal");
        lua.load(r#"cru.plugin.set_status{ session="s1", key="a", text="running" }"#)
            .exec()
            .unwrap();
        assert_eq!(
            reg.get("s1")[0].1.color_group,
            plugin_hue("goal"),
            "the hue of the running plugin"
        );
        lua.load(r#"cru.plugin.set_status{ session="s1", key="a", plugin="goal", text="running", color="warn" }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.color_group, StatusColorGroup::Warn);
        lua.load(r#"cru.plugin.set_status{ session="s1", key="a", plugin="goal", text="running", color="unknown" }"#)
            .exec()
            .unwrap();
        assert_eq!(reg.get("s1")[0].1.color_group, StatusColorGroup::Info);
        crate::plugin_context::set_source(&lua, previous);
    }

    #[test]
    fn lua_publishes_one_ordered_status_list_and_keeps_its_own_pins() {
        let reg = StatusRegistry::new();
        let lua = lua_with_status(reg.clone());
        lua.load(
            r#"
            local sl = cru.statusline
            sl.publish("s1", {
              sl.item{ id="later", text="later", priority=40, plugin="weather" },
              sl.item{ id="approval", text="goal · asks", priority=20,
                       color="warn", action="open_goal", pinned=false },
              sl.item{ id="pin", text="pinned", priority=30, pinned=true },
              sl.item{ id="first", text="first", priority=10, color="hue-3" },
            })
        "#,
        )
        .exec()
        .unwrap();
        let items = reg.get("s1");
        assert_eq!(
            items.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            [
                "init.lua/first",
                "init.lua/approval",
                "init.lua/pin",
                "init.lua/later"
            ]
        );
        assert_eq!(items[0].1.priority, 10);
        assert_eq!(items[0].1.color_group, StatusColorGroup::Hue3);
        assert!(!items[1].1.pinned, "an action name does not pin");
        assert!(items[2].1.pinned);
        assert_eq!(items[1].1.action.as_deref(), Some("open_goal"));
        lua.load(
            r#"cru.statusline.publish("s1", { cru.statusline.item{ id="new", text="new" } })"#,
        )
        .exec()
        .unwrap();
        assert_eq!(
            reg.get("s1").len(),
            1,
            "publish replaces the author's whole list"
        );
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
        reg.publish("s1", "oci", vec![("one".into(), entry("one"))]);
        reg.clear("s1", "oci", "one");
        assert_eq!(*seen.lock().unwrap(), [("s1".into(), 1), ("s1".into(), 0)]);
    }

    fn entry(text: &str) -> StatusEntry {
        StatusEntry {
            plugin: "oci".to_string(),
            text: text.to_string(),
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
        assert_eq!(
            reg.get("s1")[0].1.progress,
            Some(StatusProgress::INDETERMINATE)
        );

        lua.load(
            r#"cru.plugin.set_status{ session="s1", key="build", text="pulling", progress=0.25 }"#,
        )
        .exec()
        .unwrap();
        assert_eq!(
            reg.get("s1")[0].1.progress,
            Some(StatusProgress::Fraction(0.25))
        );
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
        assert_eq!(
            reg.get("s1")[0].1.progress,
            Some(StatusProgress::Fraction(1.0))
        );

        lua.load(r#"cru.plugin.set_status{ session="s1", key="k", text="t", progress=-1 }"#)
            .exec()
            .unwrap();
        assert_eq!(
            reg.get("s1")[0].1.progress,
            Some(StatusProgress::Fraction(0.0))
        );
    }

    #[test]
    fn slots_are_scoped_to_their_session() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", "oci", entry("sandboxed"));
        assert_eq!(reg.get("s1").len(), 1);
        assert!(
            reg.get("s2").is_empty(),
            "one session's status must not appear against another"
        );
    }

    #[test]
    fn setting_the_same_key_replaces_rather_than_appends() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", "oci", entry("starting"));
        reg.set("s1", "oci", "oci", entry("sandboxed"));
        let slots = reg.get("s1");
        assert_eq!(slots.len(), 1, "a key is a slot, not a log");
        assert_eq!(slots[0].1.text, "sandboxed");
    }

    #[test]
    fn slots_render_in_stable_key_order() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", "zebra", entry("z"));
        reg.set("s1", "oci", "alpha", entry("a"));
        reg.set("s1", "oci", "middle", entry("m"));
        let keys: Vec<_> = reg.get("s1").into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec!["oci/alpha", "oci/middle", "oci/zebra"],
            "unstable order makes a status bar reshuffle on every update"
        );
    }

    #[test]
    fn clearing_removes_only_the_named_slot() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", "oci", entry("sandboxed"));
        reg.set("s1", "oci", "other", entry("something"));
        reg.clear("s1", "oci", "oci");
        let keys: Vec<_> = reg.get("s1").into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec!["oci/other"]);
    }

    /// A plugin that goes inert takes its items with it, in each session,
    /// and each of those sessions hears of it. Other plugins keep theirs.
    #[test]
    fn releasing_a_plugin_drops_its_lists_and_notifies() {
        let reg = StatusRegistry::new();
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let notices = seen.clone();
        assert!(reg.set_change_notifier(std::sync::Arc::new(move |session| {
            notices.lock().unwrap().push(session.to_owned());
        })));
        reg.set("s1", "oci", "oci", entry("sandboxed"));
        reg.set("s2", "oci", "oci", entry("sandboxed"));
        reg.set("s1", "sync", "state", entry("idle"));
        seen.lock().unwrap().clear();

        reg.release_plugin("oci");
        let mut notified = seen.lock().unwrap().clone();
        notified.sort();
        assert_eq!(notified, ["s1", "s2"]);
        let left: Vec<_> = reg.get("s1").into_iter().map(|(id, _)| id).collect();
        assert_eq!(left, ["sync/state"]);
        assert!(reg.get("s2").is_empty());
    }

    #[test]
    fn releasing_a_session_drops_its_slots() {
        let reg = StatusRegistry::new();
        reg.set("s1", "oci", "oci", entry("sandboxed"));
        reg.release("s1");
        assert!(
            reg.get("s1").is_empty(),
            "a finished session's status must not show against a live one"
        );
    }
}
