#![allow(dead_code)] // helpers used by disabled test modules awaiting reconstruction

use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::Node;
use crucible_oil::ansi::strip_ansi;
use crucible_oil::focus::FocusContext;

use super::vt100_runtime::Vt100TestRuntime;

/// Resolve `assets/fixtures/<name>` from the crate manifest, not the cwd.
///
/// The one place a test may name a fixture. A relative `../../assets/...`
/// silently depends on which directory the runner happens to start in.
///
/// **Panics if the fixture is absent, by design.** Every fixture under
/// `assets/fixtures` is committed, so a missing one is a broken checkout, not a
/// condition to tiptoe around. The replay tests used to open with
/// `if !path.exists() { eprintln!("Skipping…"); return; }` — which reports
/// success while asserting nothing. Deleting all four recordings left nine such
/// tests passing in 0.02s apiece. Resolving through here makes that
/// unrepresentable: there is no way to name a fixture and receive a path that
/// does not exist.
pub fn fixture_path(name: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .join("assets/fixtures")
        .join(name);

    assert!(
        path.exists(),
        "fixture {} is missing. It is committed to the repo, so this is a \
         broken checkout — not a test to skip.",
        path.display()
    );

    path
}

/// Read `assets/fixtures/<name>`, panicking with the resolved path on failure.
pub fn read_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

pub fn view_with_default_ctx(app: &OilChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

/// Render app through the real terminal path (Terminal<Vec<u8>> → vt100)
/// and return stripped screen contents. This is the canonical test render
/// function — it exercises the same code path as production.
pub fn vt_render(app: &mut OilChatApp) -> String {
    vt_render_sized(app, 80, 24)
}

/// Like vt_render but with custom terminal dimensions.
pub fn vt_render_sized(app: &mut OilChatApp, width: u16, height: u16) -> String {
    let mut vt = Vt100TestRuntime::new(width, height);
    vt.render_frame(app);
    strip_ansi(&vt.screen_contents())
}
