//! `cru plugin check` on the loader's VM: what only this VM can prove.
//!
//! `crucible-lua` builds a check VM with `cru.on` and no daemon, so it has
//! no `cru.schedule` and no `cru.timer`. The two counters the check reads
//! for those are proved here, on the VM the plugins run on.
use super::super::*;
use crucible_lua::{check_plugin_on, CheckerChoice, Finding, LuaSource};
use tempfile::TempDir;

/// A plugin directory named `name` under a fresh root, with `init.luau`.
/// The name matters: the check enters `LuaSource::Plugin(<directory name>)`.
fn plugin_dir(name: &str, init: &str) -> (TempDir, PathBuf) {
    let root = TempDir::new().unwrap();
    let dir = root.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("init.luau"), init).unwrap();
    (root, dir)
}

fn load_findings(report: &crucible_lua::CheckReport) -> Vec<&str> {
    report
        .findings
        .iter()
        .filter_map(|f| match f {
            Finding::Load { message } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

/// A schedule and a task made in the module body are two top-level
/// registrations, named for the call that made them, and the check clears
/// both before it returns.
///
/// A tokio runtime is required: `cru.schedule` and `cru.timer.spawn` both
/// spawn a task.
#[tokio::test]
async fn check_counts_a_top_level_schedule_and_timer_and_leaves_none_behind() {
    let (_root, dir) = plugin_dir(
        "ticker",
        "cru.schedule({ every = 60 }, function() end)\n\
         cru.timer.spawn(function() end)\n\
         return {}\n",
    );
    let loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let vm = loader.executor();

    let report =
        check_plugin_on(&dir, None, false, &CheckerChoice::None, vm).expect("check runs");

    let loads = load_findings(&report);
    assert!(
        loads.iter().any(|m| m.contains("2 registration(s) at top level")
            && m.contains("cru.schedule")
            && m.contains("cru.timer.spawn")),
        "{:?}",
        report.findings
    );
    assert!(!report.passed(), "a top-level effect fails the check");

    let source = LuaSource::Plugin("ticker".into());
    assert_eq!(
        crucible_lua::schedule::count_source(vm.lua(), &source),
        0,
        "the check must cancel the schedule it counted"
    );
    assert_eq!(
        crucible_lua::timer::count_source(vm.lua(), &source),
        0,
        "the check must abort the task it counted"
    );
}
