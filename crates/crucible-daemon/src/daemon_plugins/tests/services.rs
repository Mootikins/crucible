//! Plugin service tasks: extraction attribution, and abort-before-respawn on
//! reload/disable. Reloading discord used to leave two gateway loops both
//! consuming events — duplicate bot responses until daemon restart.
use super::super::*;
use std::time::Duration;

/// Write a plugin whose single service tags each spawn generation and then
/// ticks a per-generation counter forever. Generation N increments
/// `ticks_N` every 20ms — an aborted generation's counter goes silent.
fn write_ticker_plugin(root: &std::path::Path, name: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        format!(
            r#"return {{
                name = "{name}",
                version = "0.1.0",
                services = {{
                    tick = {{
                        description = "per-generation counter",
                        fn = function()
                            local gen = (_G.gen or 0) + 1
                            _G.gen = gen
                            local key = "ticks_" .. gen
                            while true do
                                _G[key] = (_G[key] or 0) + 1
                                cru.timer.sleep(0.02)
                            end
                        end,
                    }},
                }},
            }}"#
        ),
    )
    .unwrap();
}

fn read_counter(loader: &DaemonPluginLoader, global: &str) -> i64 {
    loader
        .executor
        .lua()
        .globals()
        .get::<Option<i64>>(global)
        .unwrap()
        .unwrap_or(0)
}

/// Condition-based wait: poll `global` until it reaches `min`. Wrapped in a
/// timeout so a wiring regression fails in seconds instead of hanging nextest.
async fn wait_for_at_least(loader: &DaemonPluginLoader, global: &str, min: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while read_counter(loader, global) < min {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("'{global}' never reached {min}"));
}

/// Extraction must carry the owning plugin's name — a bare
/// `(service_name, Function)` pair left the spawner nothing to record the
/// JoinHandle against, which is why no spawn site could ever abort one.
#[tokio::test]
async fn extracted_service_fns_carry_their_owning_plugins_name() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_ticker_plugin(tmp.path(), "svcowner");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");

    let fns = loader.take_service_fns();
    assert_eq!(fns.len(), 1, "one declared service");
    assert_eq!(fns[0].plugin, "svcowner");
    assert_eq!(fns[0].service, "tick");
}

/// Reloading a plugin must abort its running service tasks before the new
/// generation spawns — through the production spawn helper, not a
/// hand-spawned future: a mechanism-only test would keep passing even if no
/// production site ever recorded a handle.
#[tokio::test]
async fn reloading_a_plugin_replaces_its_service_task_instead_of_stacking() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_ticker_plugin(tmp.path(), "ticker");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    crate::server::plugins::spawn_plugin_services(&mut loader);
    wait_for_at_least(&loader, "ticks_1", 1).await;

    loader.reload_plugin("ticker").await.expect("reload");
    crate::server::plugins::spawn_plugin_services(&mut loader);
    wait_for_at_least(&loader, "ticks_2", 1).await;

    // The reload aborted generation 1 at its next await point; on this
    // single-threaded test runtime it can never be polled again, so its
    // counter is final. Let generation 2 advance several cycles to prove the
    // clock is still running while generation 1 stays silent.
    let gen1_final = read_counter(&loader, "ticks_1");
    let gen2_base = read_counter(&loader, "ticks_2");
    wait_for_at_least(&loader, "ticks_2", gen2_base + 3).await;
    assert_eq!(
        read_counter(&loader, "ticks_1"),
        gen1_final,
        "the first generation's service kept running after reload"
    );

    // Exactly the new generation's handle is tracked, and it is live.
    let handles = loader.service_tasks.get("ticker").expect("tracked");
    assert_eq!(handles.len(), 1, "one handle per generation, not stacked");
    assert!(!handles[0].is_finished());
}

/// The disabled-plugin reload bail is the operator kill switch for a
/// misbehaving plugin — precisely the path where a leaked service task is
/// worst. It must abort services too.
#[tokio::test]
async fn disabling_then_reloading_a_plugin_aborts_its_service() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_ticker_plugin(tmp.path(), "ticker");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    crate::server::plugins::spawn_plugin_services(&mut loader);
    wait_for_at_least(&loader, "ticks_1", 1).await;

    loader.plugin_manager.disable("ticker").expect("disable");
    assert!(
        loader.reload_plugin("ticker").await.is_err(),
        "reloading a disabled plugin must refuse"
    );

    assert!(
        loader
            .service_tasks
            .get("ticker")
            .is_none_or(|v| v.is_empty()),
        "no service handle survives the kill switch"
    );
    // Aborted at its sleep await; on this single-threaded runtime it cannot
    // run again, so two samples across two tick periods must match.
    let stopped = read_counter(&loader, "ticks_1");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        read_counter(&loader, "ticks_1"),
        stopped,
        "the disabled plugin's service kept ticking"
    );
}
