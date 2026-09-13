use super::vm_with_error_log;
use crate::lifecycle::{PluginErrorEntry, PluginErrorLog, PluginManager};
use mlua::Lua;

#[test]
fn test_error_log_push_and_recent() {
    let mut log = PluginErrorLog::new(10);
    for i in 0..5u32 {
        log.push(PluginErrorEntry {
            plugin: "test-plugin".to_string(),
            error: format!("error-{}", i),
            context: "test".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }
    assert_eq!(log.len(), 5);
    let recent = log.recent(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].error, "error-2");
    assert_eq!(recent[1].error, "error-3");
    assert_eq!(recent[2].error, "error-4");
}

#[test]
fn test_error_log_ring_buffer_bounded() {
    let mut log = PluginErrorLog::new(100);
    for i in 0..105u32 {
        log.push(PluginErrorEntry {
            plugin: "test-plugin".to_string(),
            error: format!("error-{}", i),
            context: "test".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }
    assert_eq!(log.len(), 100, "ring buffer should be capped at capacity");
    // Oldest entries (error-0..error-4) should be evicted
    let oldest = log.recent(100)[0].error.clone();
    assert_eq!(
        oldest, "error-5",
        "oldest surviving entry should be error-5"
    );
}

#[test]
fn test_error_log_clear() {
    let mut log = PluginErrorLog::new(10);
    for i in 0..5u32 {
        log.push(PluginErrorEntry {
            plugin: "test-plugin".to_string(),
            error: format!("error-{}", i),
            context: "test".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }
    assert_eq!(log.len(), 5);
    log.clear();
    assert_eq!(log.len(), 0);
    assert!(log.is_empty());
}

#[test]
fn test_cru_errors_recent_returns_entries() {
    let (lua, log) = vm_with_error_log();
    log.lock().unwrap().push(PluginErrorEntry {
        plugin: "test-plugin".to_string(),
        error: "test error".to_string(),
        context: "test context".to_string(),
        timestamp: std::time::Instant::now(),
    });

    let recent = lua
        .load("return cru.errors.recent(1)")
        .eval::<mlua::Table>()
        .unwrap();
    assert_eq!(recent.len().unwrap(), 1);

    let entry = recent.get::<mlua::Table>(1).unwrap();
    assert_eq!(entry.get::<String>("plugin").unwrap(), "test-plugin");
    assert_eq!(entry.get::<String>("error").unwrap(), "test error");
    assert_eq!(entry.get::<String>("context").unwrap(), "test context");
    assert!(entry.get::<f64>("age_secs").unwrap() >= 0.0);
}

#[test]
fn test_emitter_error_captured_in_log() {
    let (lua, log) = vm_with_error_log();
    lua.load(
        r#"
        cru.emitter.global():on("test_event", function()
            error("intentional error")
        end, "test-plugin")
        cru.emitter.global():emit("test_event")
    "#,
    )
    .exec()
    .unwrap();

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1);
    let recent = log.recent(1);
    assert_eq!(recent[0].plugin, "test-plugin");
    assert!(recent[0].error.contains("intentional error"));
    assert!(recent[0].context.contains("test_event"));
}

#[test]
fn test_error_log_attributes_to_plugin() {
    let (lua, log) = vm_with_error_log();
    lua.load(
        r#"
        cru.emitter.global():on("msg", function()
            error("boom")
        end, "my-plugin")
        cru.emitter.global():emit("msg")
    "#,
    )
    .exec()
    .unwrap();

    let log = log.lock().unwrap();
    assert!(!log.is_empty());
    let recent = log.recent(1);
    assert_eq!(recent[0].plugin, "my-plugin");
    assert!(recent[0].context.contains("msg"));
}

/// A raising `on_load` lands in the log of the VM that holds the hook, so
/// `cru.errors.recent` on that VM answers it. The manager has no log of its
/// own: the daemon VM is the only VM that runs plugin code.
#[test]
fn a_hook_that_raises_is_recorded_in_the_vms_error_log() {
    let (lua, log) = vm_with_error_log();
    let on_load: mlua::Function = lua
        .load(r#"return function() error("on_load boom") end"#)
        .eval()
        .unwrap();
    let key = lua.create_registry_value(on_load).unwrap();

    let mut manager = PluginManager::new();
    manager.set_lifecycle_hooks("hooked", Some(key), None);
    manager.call_on_load_hook(&lua, "hooked");

    let log = log.lock().unwrap();
    let recent = log.recent(1);
    assert_eq!(recent.len(), 1, "the raise was not recorded");
    assert_eq!(recent[0].plugin, "hooked");
    assert!(
        recent[0].error.contains("on_load boom"),
        "{}",
        recent[0].error
    );
    assert_eq!(recent[0].context, "handler:on_load:hooked");
}

/// A VM without a log drops the entry. The hook still ran, and the call
/// did not raise.
#[test]
fn a_hook_that_raises_on_a_vm_without_a_log_is_not_fatal() {
    let lua = Lua::new();
    let on_unload: mlua::Function = lua
        .load(r#"return function() _G.fired = true; error("late") end"#)
        .eval()
        .unwrap();
    let key = lua.create_registry_value(on_unload).unwrap();

    let mut manager = PluginManager::new();
    manager.set_lifecycle_hooks("quiet", None, Some(key));
    manager.call_on_unload_hook(&lua, "quiet");

    let fired: bool = lua.globals().get("fired").unwrap();
    assert!(fired, "the hook did not run");
    assert!(PluginErrorLog::of(&lua).is_none());
}
