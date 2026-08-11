//! The shipped `review` plugin, loaded through the real daemon plugin runtime.
//!
//! Review has two surfaces over one engine: `review.*` JSON-RPC for the web
//! panel, and these tools for an agent. The agent half is the one the design
//! actually requires — a delegating agent reviewing the sub-session it
//! spawned cannot speak JSON-RPC — and it is also the half that fails
//! silently: a plugin whose spec does not parse is simply absent, and the
//! agent is never told the tools it was promised do not exist.
//!
//! So this asserts from where the agent stands: the five tools are on the
//! registry, and every one of them takes the `session_id` that makes
//! reviewing *someone else's* session expressible.

use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_lua::PluginSource;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

/// A search path holding a copy of the shipped `review` and nothing else, so
/// an unrelated plugin failing to load cannot fail this suite. The copy is of
/// the real files, so it cannot drift from what ships.
fn review_search_path(tmp: &Path) -> PathBuf {
    let root = tmp.join("plugins");
    copy_dir(
        &repo_root().join("runtime/plugins/review"),
        &root.join("review"),
    );
    root
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

async fn load_review(tmp: &Path) -> DaemonPluginLoader {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(review_search_path(tmp), PluginSource::EnvPath)])
        .await
        .expect("load plugins");
    loader
}

#[tokio::test]
async fn the_review_plugin_offers_every_review_operation_as_a_tool() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = load_review(tmp.path()).await;
    let names = loader.plugin_registry().tool_names();

    for tool in [
        "review_list_hunks",
        "review_set_state",
        "review_comment",
        "review_resolve_comment",
    ] {
        assert!(names.contains(tool), "missing tool {tool}; got {names:?}");
    }
}

/// Every tool names the session under review explicitly. An implicit "current
/// session" would make the one case §6 exists for — a delegator reviewing its
/// child — the one case the tools cannot express.
#[tokio::test]
async fn every_review_tool_takes_the_session_it_reviews() {
    let tmp = tempfile::TempDir::new().unwrap();
    let loader = load_review(tmp.path()).await;

    let definitions = loader.plugin_registry().tool_definitions();
    let review: Vec<_> = definitions
        .iter()
        .filter(|d| d.name.starts_with("review_"))
        .collect();
    assert_eq!(
        review.len(),
        4,
        "expected every review tool: {:?}",
        review.iter().map(|d| &d.name).collect::<Vec<_>>()
    );

    for def in review {
        let properties = def
            .parameters
            .as_ref()
            .and_then(|p| p.get("properties"))
            .and_then(|p| p.as_object())
            .unwrap_or_else(|| panic!("{} has no parameter properties", def.name));
        assert!(
            properties.contains_key("session_id"),
            "{} does not take session_id",
            def.name
        );
    }
}
