//! Shared helpers for tui_e2e_tests submodules.

use std::path::PathBuf;
use std::time::Duration;

use super::tui_e2e_harness::TuiTestConfig;

pub(super) fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub(super) fn find_binary() -> Option<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
    let workspace_root = manifest_path
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not find workspace root");

    let release_path = workspace_root.join("target/release/cru");
    if release_path.exists() {
        return Some(release_path);
    }

    let debug_path = workspace_root.join("target/debug/cru");
    if debug_path.exists() {
        return Some(debug_path);
    }

    None
}

/// Skip test if binary is not built
macro_rules! require_binary {
    () => {
        if $crate::shared::find_binary().is_none() {
            eprintln!("SKIPPED: cru binary not built. Run `cargo build` first.");
            return;
        }
    };
}

pub(super) use require_binary;

/// Build a TuiTestConfig that uses the dev's LLM provider config.
///
/// If `CRUCIBLE_TEST_CONFIG` is set, passes `--config <path>` to `cru chat`.
/// Otherwise falls back to default config (which auto-detects Ollama at localhost:11434).
///
/// Usage:
///   CRUCIBLE_TEST_CONFIG=~/.config/crucible/config.toml cargo nextest run -- --ignored
pub(super) fn provider_test_config() -> TuiTestConfig {
    let mut config = TuiTestConfig::new("chat")
        .with_env("RUST_LOG", "warn")
        .with_timeout(Duration::from_secs(15));

    if let Ok(cfg_path) = std::env::var("CRUCIBLE_TEST_CONFIG") {
        config.args = vec!["--config".to_string(), cfg_path];
    }

    config
}
