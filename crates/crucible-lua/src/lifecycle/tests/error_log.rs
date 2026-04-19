use super::{
    create_test_plugin_with_source, setup_emitter_manager, setup_emitter_manager_with_paths,
};
use crate::lifecycle::{PluginErrorEntry, PluginErrorLog};
use tempfile::TempDir;

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
    let manager = setup_emitter_manager();
    {
        let mut log = manager.error_log();
        log.push(PluginErrorEntry {
            plugin: "test-plugin".to_string(),
            error: "test error".to_string(),
            context: "test context".to_string(),
            timestamp: std::time::Instant::now(),
        });
    }

    let recent = manager
        .lua
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
fn test_error_during_emitter_emit_captured() {
    let temp = TempDir::new().unwrap();
    create_test_plugin_with_source(
        temp.path(),
        "error-plugin",
        "1.0.0",
        r#"
        return {
            on_load = function()
                cru.emitter.global():on("error_event", function()
                    error("intentional test error")
                end, "error-plugin")
            end,
        }
    "#,
    );

    let mut manager = setup_emitter_manager_with_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("error-plugin").unwrap();

    manager
        .lua
        .load("cru.emitter.global():emit('error_event')")
        .exec()
        .unwrap();

    let log = manager.error_log();
    assert!(!log.is_empty(), "error should be captured in error_log");
    let entry = &log.recent(1)[0];
    assert_eq!(
        entry.plugin, "error-plugin",
        "error should be attributed to correct plugin"
    );
    assert!(
        entry.context.contains("error_event"),
        "context should mention the event name"
    );
}

#[test]
fn test_emitter_error_captured_in_log() {
    let manager = setup_emitter_manager();
    manager
        .lua
        .load(
            r#"
        cru.emitter.global():on("test_event", function()
            error("intentional error")
        end, "test-plugin")
        cru.emitter.global():emit("test_event")
    "#,
        )
        .exec()
        .unwrap();

    let log = manager.error_log();
    assert_eq!(log.len(), 1);
    let recent = log.recent(1);
    assert_eq!(recent[0].plugin, "test-plugin");
    assert!(recent[0].error.contains("intentional error"));
    assert!(recent[0].context.contains("test_event"));
}

#[test]
fn test_error_log_attributes_to_plugin() {
    let manager = setup_emitter_manager();
    manager
        .lua
        .load(
            r#"
        cru.emitter.global():on("msg", function()
            error("boom")
        end, "my-plugin")
        cru.emitter.global():emit("msg")
    "#,
        )
        .exec()
        .unwrap();

    let log = manager.error_log();
    assert!(!log.is_empty());
    let recent = log.recent(1);
    assert_eq!(recent[0].plugin, "my-plugin");
    assert!(recent[0].context.contains("msg"));
}
