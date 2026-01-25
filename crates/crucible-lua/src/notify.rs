//! Notification API for Crucible Lua plugins
//!
//! Provides `crucible.notify()` and `crucible.notify_once()` following Neovim patterns.
//!
//! ```lua
//! -- Simple notification (toast, auto-dismisses)
//! crucible.notify("Session saved")
//!
//! -- With log level
//! crucible.notify("Connection failed", crucible.log.levels.ERROR)
//!
//! -- With options
//! crucible.notify("Indexing...", crucible.log.levels.INFO, {
//!     progress = { current = 45, total = 100 }
//! })
//!
//! -- Warning (persists until dismissed)
//! crucible.notify("Context at 85%", crucible.log.levels.WARN)
//!
//! -- Show only once per message
//! crucible.notify_once("Deprecated API", crucible.log.levels.WARN)
//! ```

use crucible_core::types::{Notification, NotificationKind};
use mlua::{Lua, Result as LuaResult, Table, Value};

const NOTIFICATIONS_KEY: &str = "__crucible_notifications__";
const NOTIFIED_ONCE_KEY: &str = "__crucible_notified_once__";

pub fn register_notify_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    register_log_levels(lua, crucible)?;
    register_notify_function(lua, crucible)?;
    register_notify_once_function(lua, crucible)?;
    register_messages_module(lua, crucible)?;
    Ok(())
}

fn register_log_levels(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let log_table: Table = crucible
        .get("log")
        .unwrap_or_else(|_| lua.create_table().unwrap());

    let levels = lua.create_table()?;
    levels.set("TRACE", 0)?;
    levels.set("DEBUG", 1)?;
    levels.set("INFO", 2)?;
    levels.set("WARN", 3)?;
    levels.set("ERROR", 4)?;
    levels.set("OFF", 5)?;

    log_table.set("levels", levels)?;
    crucible.set("log", log_table)?;

    Ok(())
}

fn register_notify_function(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let notify_fn = lua.create_function(|lua, args: mlua::Variadic<Value>| {
        let msg = match args.first() {
            Some(Value::String(s)) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::external(
                    "notify: first argument must be a string",
                ))
            }
        };

        let level = match args.get(1) {
            Some(Value::Integer(n)) => *n as i32,
            Some(Value::Number(n)) => *n as i32,
            _ => 2,
        };

        let opts: Option<&Table> = match args.get(2) {
            Some(Value::Table(t)) => Some(t),
            _ => None,
        };

        let notification = build_notification(&msg, level, opts)?;
        queue_notification(lua, notification)?;

        Ok(())
    })?;

    crucible.set("notify", notify_fn)?;
    Ok(())
}

fn register_notify_once_function(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let notify_once_fn = lua.create_function(|lua, args: mlua::Variadic<Value>| {
        let msg = match args.first() {
            Some(Value::String(s)) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::external(
                    "notify_once: first argument must be a string",
                ))
            }
        };

        let globals = lua.globals();
        let notified: Table = globals
            .get(NOTIFIED_ONCE_KEY)
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let already_notified: bool = notified.get(msg.as_str()).unwrap_or(false);
        if already_notified {
            return Ok(false);
        }

        notified.set(msg.as_str(), true)?;
        globals.set(NOTIFIED_ONCE_KEY, notified)?;

        let level = match args.get(1) {
            Some(Value::Integer(n)) => *n as i32,
            Some(Value::Number(n)) => *n as i32,
            _ => 2,
        };

        let opts: Option<&Table> = match args.get(2) {
            Some(Value::Table(t)) => Some(t),
            _ => None,
        };

        let notification = build_notification(&msg, level, opts)?;
        queue_notification(lua, notification)?;

        Ok(true)
    })?;

    crucible.set("notify_once", notify_once_fn)?;
    Ok(())
}

fn register_messages_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let messages = lua.create_table()?;

    let toggle_fn = lua.create_function(|lua, ()| set_messages_action(lua, "toggle"))?;
    messages.set("toggle", toggle_fn)?;

    let show_fn = lua.create_function(|lua, ()| set_messages_action(lua, "show"))?;
    messages.set("show", show_fn)?;

    let hide_fn = lua.create_function(|lua, ()| set_messages_action(lua, "hide"))?;
    messages.set("hide", hide_fn)?;

    let clear_fn = lua.create_function(|lua, ()| set_messages_action(lua, "clear"))?;
    messages.set("clear", clear_fn)?;

    crucible.set("messages", messages)?;
    Ok(())
}

fn set_messages_action(lua: &Lua, action: &str) -> LuaResult<()> {
    let globals = lua.globals();
    globals.set("__crucible_messages_action__", action)?;
    Ok(())
}

fn build_notification(msg: &str, level: i32, opts: Option<&Table>) -> LuaResult<Notification> {
    let kind = if let Some(opts) = opts {
        if let Ok(progress) = opts.get::<Table>("progress") {
            let current: usize = progress.get("current").unwrap_or(0);
            let total: usize = progress.get("total").unwrap_or(100);
            NotificationKind::Progress { current, total }
        } else {
            level_to_kind(level)
        }
    } else {
        level_to_kind(level)
    };

    Ok(match kind {
        NotificationKind::Toast => Notification::toast(msg),
        NotificationKind::Warning => Notification::warning(msg),
        NotificationKind::Progress { current, total } => {
            Notification::progress(current, total, msg)
        }
    })
}

fn level_to_kind(level: i32) -> NotificationKind {
    match level {
        3 | 4 => NotificationKind::Warning,
        _ => NotificationKind::Toast,
    }
}

fn queue_notification(lua: &Lua, notification: Notification) -> LuaResult<()> {
    let globals = lua.globals();
    let queue: Table = globals
        .get(NOTIFICATIONS_KEY)
        .unwrap_or_else(|_| lua.create_table().unwrap());

    let entry = lua.create_table()?;
    entry.set("id", notification.id.as_str())?;
    entry.set("message", notification.message.as_str())?;
    entry.set("kind", kind_to_string(&notification.kind))?;

    if let NotificationKind::Progress { current, total } = notification.kind {
        entry.set("current", current)?;
        entry.set("total", total)?;
    }

    let len = queue.raw_len();
    queue.raw_set(len + 1, entry)?;
    globals.set(NOTIFICATIONS_KEY, queue)?;

    Ok(())
}

fn kind_to_string(kind: &NotificationKind) -> &'static str {
    match kind {
        NotificationKind::Toast => "toast",
        NotificationKind::Warning => "warning",
        NotificationKind::Progress { .. } => "progress",
    }
}

/// Retrieve and clear pending notifications from Lua execution
pub fn get_pending_notifications(lua: &Lua) -> LuaResult<Vec<Notification>> {
    let globals = lua.globals();
    let queue: Table = match globals.get(NOTIFICATIONS_KEY) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let mut notifications = Vec::new();
    for i in 1..=queue.raw_len() {
        if let Ok(entry) = queue.raw_get::<Table>(i) {
            if let Ok(notification) = table_to_notification(&entry) {
                notifications.push(notification);
            }
        }
    }

    globals.set(NOTIFICATIONS_KEY, lua.create_table()?)?;
    Ok(notifications)
}

/// Get pending messages panel action (toggle/show/hide/clear)
pub fn get_messages_action(lua: &Lua) -> LuaResult<Option<String>> {
    let globals = lua.globals();
    let action: Option<String> = globals.get("__crucible_messages_action__").ok();
    if action.is_some() {
        globals.set("__crucible_messages_action__", Value::Nil)?;
    }
    Ok(action)
}

fn table_to_notification(entry: &Table) -> LuaResult<Notification> {
    let message: String = entry.get("message")?;
    let kind_str: String = entry.get("kind")?;

    let notification = match kind_str.as_str() {
        "warning" => Notification::warning(&message),
        "progress" => {
            let current: usize = entry.get("current").unwrap_or(0);
            let total: usize = entry.get("total").unwrap_or(100);
            Notification::progress(current, total, &message)
        }
        _ => Notification::toast(&message),
    };

    Ok(notification)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> (Lua, Table) {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();

        let log_table = lua.create_table().unwrap();
        crucible.set("log", log_table).unwrap();

        register_notify_module(&lua, &crucible).unwrap();
        lua.globals().set("crucible", crucible.clone()).unwrap();

        (lua, crucible)
    }

    #[test]
    fn notify_queues_toast() {
        let (lua, _) = setup_lua();

        lua.load(r#"crucible.notify("Hello world")"#)
            .exec()
            .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].message, "Hello world");
        assert!(matches!(notifications[0].kind, NotificationKind::Toast));
    }

    #[test]
    fn notify_with_level_creates_warning() {
        let (lua, _) = setup_lua();

        lua.load(r#"crucible.notify("Danger!", crucible.log.levels.WARN)"#)
            .exec()
            .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(matches!(notifications[0].kind, NotificationKind::Warning));
    }

    #[test]
    fn notify_with_progress() {
        let (lua, _) = setup_lua();

        lua.load(
            r#"crucible.notify("Indexing...", crucible.log.levels.INFO, { progress = { current = 45, total = 100 } })"#,
        )
        .exec()
        .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(matches!(
            notifications[0].kind,
            NotificationKind::Progress {
                current: 45,
                total: 100
            }
        ));
    }

    #[test]
    fn notify_once_deduplicates() {
        let (lua, _) = setup_lua();

        lua.load(
            r#"
            crucible.notify_once("Only once")
            crucible.notify_once("Only once")
            crucible.notify_once("Only once")
        "#,
        )
        .exec()
        .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
    }

    #[test]
    fn notify_once_returns_boolean() {
        let (lua, _) = setup_lua();

        let first: bool = lua
            .load(r#"return crucible.notify_once("Test")"#)
            .eval()
            .unwrap();
        let second: bool = lua
            .load(r#"return crucible.notify_once("Test")"#)
            .eval()
            .unwrap();

        assert!(first);
        assert!(!second);
    }

    #[test]
    fn log_levels_available() {
        let (lua, _) = setup_lua();

        let info: i32 = lua
            .load(r#"return crucible.log.levels.INFO"#)
            .eval()
            .unwrap();
        let warn: i32 = lua
            .load(r#"return crucible.log.levels.WARN"#)
            .eval()
            .unwrap();
        let error: i32 = lua
            .load(r#"return crucible.log.levels.ERROR"#)
            .eval()
            .unwrap();

        assert_eq!(info, 2);
        assert_eq!(warn, 3);
        assert_eq!(error, 4);
    }

    #[test]
    fn messages_toggle() {
        let (lua, _) = setup_lua();

        lua.load(r#"crucible.messages.toggle()"#).exec().unwrap();

        let action = get_messages_action(&lua).unwrap();
        assert_eq!(action, Some("toggle".to_string()));

        let action_again = get_messages_action(&lua).unwrap();
        assert_eq!(action_again, None);
    }

    #[test]
    fn messages_show_hide_clear() {
        let (lua, _) = setup_lua();

        lua.load(r#"crucible.messages.show()"#).exec().unwrap();
        assert_eq!(get_messages_action(&lua).unwrap(), Some("show".to_string()));

        lua.load(r#"crucible.messages.hide()"#).exec().unwrap();
        assert_eq!(get_messages_action(&lua).unwrap(), Some("hide".to_string()));

        lua.load(r#"crucible.messages.clear()"#).exec().unwrap();
        assert_eq!(
            get_messages_action(&lua).unwrap(),
            Some("clear".to_string())
        );
    }

    #[test]
    fn pending_notifications_cleared_after_retrieval() {
        let (lua, _) = setup_lua();

        lua.load(r#"crucible.notify("First")"#).exec().unwrap();
        lua.load(r#"crucible.notify("Second")"#).exec().unwrap();

        let first_batch = get_pending_notifications(&lua).unwrap();
        assert_eq!(first_batch.len(), 2);

        let second_batch = get_pending_notifications(&lua).unwrap();
        assert_eq!(second_batch.len(), 0);
    }
}
