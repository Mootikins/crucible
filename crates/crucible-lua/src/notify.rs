//! Notification API for Crucible Lua plugins
//!
//! Provides `cru.log.notify()` and `cru.log.notify_once()` following Neovim
//! patterns. They live on the `cru.log` table: a notification is a message
//! with a level, and `cru.log.levels` is next to it.
//!
//! ```lua
//! -- Simple notification (toast, auto-dismisses)
//! cru.log.notify("Session saved")
//!
//! -- With log level
//! cru.log.notify("Connection failed", cru.log.levels.ERROR)
//!
//! -- With options
//! cru.log.notify("Indexing...", cru.log.levels.INFO, {
//!     progress = { current = 45, total = 100 }
//! })
//!
//! -- Warning (persists until dismissed)
//! cru.log.notify("Context at 85%", cru.log.levels.WARN)
//!
//! -- Show only once per message
//! cru.log.notify_once("Deprecated API", cru.log.levels.WARN)
//! ```

use crucible_core::types::{Notification, NotificationKind};
use mlua::{Lua, Result as LuaResult, Table, Value};

const NOTIFICATIONS_KEY: &str = "__crucible_notifications__";
const NOTIFIED_ONCE_KEY: &str = "__crucible_notified_once__";

pub fn register_notify_module(lua: &Lua, cru: &Table) -> LuaResult<()> {
    register_log_levels(lua, cru)?;
    // `notify`, `notify_once` and `messages` hang off the `cru.log` table the
    // call above just built.
    let log_table: Table = cru.get("log")?;
    register_notify_function(lua, &log_table)?;
    register_notify_once_function(lua, &log_table)?;
    register_messages_module(lua, &log_table)?;
    Ok(())
}

fn register_log_levels(lua: &Lua, cru: &Table) -> LuaResult<()> {
    let log_fn: Option<mlua::Function> = cru.get("log").ok();

    let log_table = lua.create_table()?;

    let levels = lua.create_table()?;
    levels.set("TRACE", 0)?;
    levels.set("DEBUG", 1)?;
    levels.set("INFO", 2)?;
    levels.set("WARN", 3)?;
    levels.set("ERROR", 4)?;
    levels.set("OFF", 5)?;

    log_table.set("levels", levels)?;

    if let Some(fn_ref) = log_fn {
        let wrapped_fn = lua.create_function(move |_, args: mlua::Variadic<mlua::Value>| {
            if args.len() < 3 {
                return Err(mlua::Error::external("log requires 2 arguments"));
            }
            let level = match &args[1] {
                mlua::Value::String(s) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("log level must be a string")),
            };
            let msg = match &args[2] {
                mlua::Value::String(s) => s.to_str()?.to_string(),
                _ => return Err(mlua::Error::external("log message must be a string")),
            };
            fn_ref.call::<()>((level, msg))
        })?;
        let metatable = lua.create_table()?;
        metatable.set("__call", wrapped_fn)?;
        log_table.set_metatable(Some(metatable))?;
    }

    cru.set("log", log_table)?;

    Ok(())
}

/// The options table both notify functions read: a progress pair, or
/// nothing. Every other key is ignored, so the declaration names only this
/// one.
const NOTIFY_OPTS: &str = "{ progress: { current: number, total: number }? }";

/// The arguments both notify functions take.
///
/// `Value`, not `String`/`i32`, because the closures decide what to do with a
/// wrong type themselves: a non-string message RAISES with a named message,
/// and a non-number level falls back to INFO rather than raising. Typed
/// arguments would hand both decisions to mlua's own coercion. The
/// declaration narrows all three, which is what an author reads.
type NotifyArgs = (Value, Option<Value>, Option<Table>);

/// Read `(level, opts)` the way both closures always have: a number of either
/// Lua kind, INFO otherwise; a table, or no options.
fn level_and_opts(level: &Option<Value>, opts: &Option<Table>) -> (i32, Option<Table>) {
    let level = match level {
        Some(Value::Integer(n)) => *n as i32,
        Some(Value::Number(n)) => *n as i32,
        _ => 2,
    };
    (level, opts.clone())
}

fn register_notify_function(lua: &Lua, log: &Table) -> LuaResult<()> {
    let mut ns = crate::host_registry::Ns::over(lua, "cru.log", log.clone());
    ns.func(
        "notify",
        &format!("(message: string, level: number?, opts: {NOTIFY_OPTS}?) -> ()"),
        |lua, (message, level, opts): NotifyArgs| {
            let msg = match message {
                Value::String(s) => s.to_str()?.to_string(),
                _ => {
                    return Err(mlua::Error::external(
                        "notify: first argument must be a string",
                    ))
                }
            };

            let (level, opts) = level_and_opts(&level, &opts);
            let notification = build_notification(&msg, level, opts.as_ref())?;
            queue_notification(lua, notification)?;

            Ok(())
        },
    )
    .map_err(mlua::Error::external)?;
    Ok(())
}

fn register_notify_once_function(lua: &Lua, log: &Table) -> LuaResult<()> {
    let mut ns = crate::host_registry::Ns::over(lua, "cru.log", log.clone());
    // Answers whether this call notified: `false` means the message was
    // already shown, which is the only way a caller can tell.
    ns.func(
        "notify_once",
        &format!("(message: string, level: number?, opts: {NOTIFY_OPTS}?) -> boolean"),
        |lua, (message, level, opts): NotifyArgs| {
            let msg = match message {
                Value::String(s) => s.to_str()?.to_string(),
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

            let (level, opts) = level_and_opts(&level, &opts);
            let notification = build_notification(&msg, level, opts.as_ref())?;
            queue_notification(lua, notification)?;

            Ok(true)
        },
    )
    .map_err(mlua::Error::external)?;
    Ok(())
}

fn register_messages_module(lua: &Lua, log: &Table) -> LuaResult<()> {
    let messages = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.log.messages", messages.clone());

    // Each one parks an action for the TUI to pick up; none answers with
    // anything, and none can fail.
    for action in ["toggle", "show", "hide", "clear"] {
        ns.func(action, "() -> ()", move |lua, ()| {
            set_messages_action(lua, action)
        })
        .map_err(mlua::Error::external)?;
    }

    // Not `ns.publish()`: this table hangs off `cru.log`, and `publish` would
    // put it at `cru.messages`.
    log.set("messages", messages)?;
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
#[cfg(test)]
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
#[cfg(test)]
pub fn get_messages_action(lua: &Lua) -> LuaResult<Option<String>> {
    let globals = lua.globals();
    let action: Option<String> = globals.get("__crucible_messages_action__").ok();
    if action.is_some() {
        globals.set("__crucible_messages_action__", Value::Nil)?;
    }
    Ok(action)
}

#[cfg(test)]
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
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn notify_queues_toast() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(r#"cru.log.notify("Hello world")"#).exec().unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].message, "Hello world");
        assert!(matches!(notifications[0].kind, NotificationKind::Toast));
    }

    #[test]
    fn notify_with_level_creates_warning() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(r#"cru.log.notify("Danger!", cru.log.levels.WARN)"#)
            .exec()
            .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(matches!(notifications[0].kind, NotificationKind::Warning));
    }

    #[test]
    fn notify_with_progress() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(
            r#"cru.log.notify("Indexing...", cru.log.levels.INFO, { progress = { current = 45, total = 100 } })"#,
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
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(
            r#"
            cru.log.notify_once("Only once")
            cru.log.notify_once("Only once")
            cru.log.notify_once("Only once")
        "#,
        )
        .exec()
        .unwrap();

        let notifications = get_pending_notifications(&lua).unwrap();
        assert_eq!(notifications.len(), 1);
    }

    #[test]
    fn notify_once_returns_boolean() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        let first: bool = lua
            .load(r#"return cru.log.notify_once("Test")"#)
            .eval()
            .unwrap();
        let second: bool = lua
            .load(r#"return cru.log.notify_once("Test")"#)
            .eval()
            .unwrap();

        assert!(first);
        assert!(!second);
    }

    #[test]
    fn log_levels_available() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        let info: i32 = lua.load(r#"return cru.log.levels.INFO"#).eval().unwrap();
        let warn: i32 = lua.load(r#"return cru.log.levels.WARN"#).eval().unwrap();
        let error: i32 = lua.load(r#"return cru.log.levels.ERROR"#).eval().unwrap();

        assert_eq!(info, 2);
        assert_eq!(warn, 3);
        assert_eq!(error, 4);
    }

    #[test]
    fn messages_toggle() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(r#"cru.log.messages.toggle()"#).exec().unwrap();

        let action = get_messages_action(&lua).unwrap();
        assert_eq!(action, Some("toggle".to_string()));

        let action_again = get_messages_action(&lua).unwrap();
        assert_eq!(action_again, None);
    }

    #[test]
    fn messages_show_hide_clear() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(r#"cru.log.messages.show()"#).exec().unwrap();
        assert_eq!(get_messages_action(&lua).unwrap(), Some("show".to_string()));

        lua.load(r#"cru.log.messages.hide()"#).exec().unwrap();
        assert_eq!(get_messages_action(&lua).unwrap(), Some("hide".to_string()));

        lua.load(r#"cru.log.messages.clear()"#).exec().unwrap();
        assert_eq!(
            get_messages_action(&lua).unwrap(),
            Some("clear".to_string())
        );
    }

    #[test]
    fn pending_notifications_cleared_after_retrieval() {
        let (lua, _) = TestLuaBuilder::new().build_with_notify();

        lua.load(r#"cru.log.notify("First")"#).exec().unwrap();
        lua.load(r#"cru.log.notify("Second")"#).exec().unwrap();

        let first_batch = get_pending_notifications(&lua).unwrap();
        assert_eq!(first_batch.len(), 2);

        let second_batch = get_pending_notifications(&lua).unwrap();
        assert_eq!(second_batch.len(), 0);
    }
}
