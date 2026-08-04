//! The workspace-target flow, end to end through a real daemon.
//!
//! Every other test of this feature stubs something. The Lua suites stub
//! `cru.shell`, the dispatch tests stub the plugin, the web tests stub the
//! daemon. This one stubs nothing: a real `cru daemon serve` process extracts
//! the bundled `runtime/` tree, loads the shipped `worktree` plugin, shells out
//! to real `git` in a real repository, and answers over the real socket.
//!
//! It exists because the chain is long and every link is somebody else's mock:
//!
//!   publication → enumeration → resolution → session.workspace
//!
//! A field name that only one side knows, a plugin that fails to load, a
//! command whose name does not match what the publication advertised — none of
//! those show up in a suite that stubs the neighbour. They all show up here.

mod common;

use common::{RpcConn, TestDaemon};
use serde_json::json;
use std::path::{Path, PathBuf};

/// Run git, failing the test rather than the assertion when git itself errors.
fn git(args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn git_stdout(args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository on `master` with one commit and an unchecked-out `feat/x`.
///
/// Returned as its own TempDir so the worktrees the plugin creates — which land
/// under `{repo}/tree/` by default — are cleaned up with it.
fn a_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let repo_s = repo.to_string_lossy().to_string();

    git(&["-C", &repo_s, "init", "-q", "-b", "master"]);
    git(&["-C", &repo_s, "config", "user.email", "test@example.com"]);
    git(&["-C", &repo_s, "config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "# test\n").unwrap();
    git(&["-C", &repo_s, "add", "."]);
    git(&["-C", &repo_s, "commit", "-q", "-m", "initial"]);
    git(&["-C", &repo_s, "branch", "feat/x"]);

    (tmp, repo)
}

async fn daemon_and_conn() -> (TestDaemon, RpcConn) {
    let daemon = TestDaemon::start().await.expect("start daemon");
    let conn = RpcConn::connect(&daemon.socket_path)
        .await
        .expect("connect to daemon");
    (daemon, conn)
}

/// `result` of a call, panicking with the RPC error when there is one.
fn ok(response: &serde_json::Value) -> serde_json::Value {
    assert!(
        response.get("error").is_none_or(serde_json::Value::is_null),
        "RPC error: {}",
        response["error"]
    );
    response["result"].clone()
}

/// The `targets` publication for one plugin, or `None`.
async fn provider(conn: &mut RpcConn, plugin: &str) -> Option<serde_json::Value> {
    let result = ok(&conn.call_method("plugin.publications", json!({}), 1).await);
    result["publications"]["targets"]
        .get(plugin)
        .filter(|v| !v.is_null())
        .cloned()
}

async fn worktree_targets(conn: &mut RpcConn, workspace: &Path) -> Vec<serde_json::Value> {
    let result = ok(&conn
        .call_method(
            "plugin.run_command",
            json!({ "name": "worktree.targets", "args": { "workspace": workspace } }),
            2,
        )
        .await);
    result["result"]["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// `session.create` against a workspace target. Returns the raw response so a
/// caller can assert on either the session or the refusal.
async fn create_with_target(
    conn: &mut RpcConn,
    workspace: &Path,
    target: &str,
    id: i64,
) -> serde_json::Value {
    conn.call_method(
        "session.create",
        json!({
            "type": "chat",
            "workspace": workspace,
            "workspace_target": target,
        }),
        id,
    )
    .await
}

#[tokio::test]
#[ignore = "requires daemon"]
async fn the_shipped_worktree_plugin_declares_itself_on_the_workspace_axis() {
    let (_daemon, mut conn) = daemon_and_conn().await;

    let decl = provider(&mut conn, "worktree")
        .await
        .expect("the bundled worktree plugin published no targets provider");

    // The axis is what keeps a branch name out of the isolation channel, where
    // `oci` would be the one deciding what to do with it.
    assert_eq!(decl["axis"], "workspace");
    // Both commands, because the daemon calls one and the client the other. A
    // publication naming a command the plugin does not declare fails at use.
    assert_eq!(decl["targets_command"], "worktree.targets");
    assert_eq!(decl["resolve_command"], "worktree.resolve");
}

#[tokio::test]
#[ignore = "requires daemon"]
async fn enumerating_targets_says_what_picking_each_branch_will_do() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let targets = worktree_targets(&mut conn, &repo).await;
    let by_name = |name: &str| {
        targets
            .iter()
            .find(|t| t["value"] == name)
            .unwrap_or_else(|| panic!("no target for {name} in {targets:?}"))
            .clone()
    };

    // The branch that is checked out here carries its path, which is the only
    // place that mapping exists — the session tree labels checkouts from it.
    let master = by_name("master");
    assert_eq!(master["hint"], "current");
    assert_eq!(master["current"], true);
    assert_eq!(master["path"], repo.to_string_lossy().as_ref());

    // The one that is not says so, so the affordance is legible before the
    // user commits to it.
    let feat = by_name("feat/x");
    assert_eq!(feat["hint"], "new worktree");
    assert!(feat["path"].is_null(), "feat/x has no checkout yet");
}

#[tokio::test]
#[ignore = "requires daemon"]
async fn a_session_created_against_a_branch_is_born_in_that_branch_s_worktree() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let result = ok(&create_with_target(&mut conn, &repo, "worktree:feat/x", 3).await);

    // Resolution runs BEFORE the session exists, so the workspace it was
    // created with is already the new checkout — not the repo it was cut from.
    let expected = repo.join("tree").join("feat/x");
    assert_eq!(
        result["workspace"],
        *expected.to_string_lossy(),
        "the session's workspace must be the resolved worktree, not the repo"
    );

    // And the worktree is real: a LINKED one, whose `.git` is a file holding a
    // path into the main repo rather than a directory.
    assert!(expected.is_dir(), "no worktree at {}", expected.display());
    assert!(
        expected.join(".git").is_file(),
        "a linked worktree's .git is a file, not a directory"
    );
    assert_eq!(
        git_stdout(&[
            "-C",
            &expected.to_string_lossy(),
            "rev-parse",
            "--abbrev-ref",
            "HEAD"
        ]),
        "feat/x",
        "the worktree must be checked out on the branch that was asked for"
    );
}

/// The parallel-agents flow: N sessions on one branch is not an error.
///
/// git refuses two worktrees on the same branch, so a provider that created
/// blindly would fail every session after the first.
#[tokio::test]
#[ignore = "requires daemon"]
async fn a_second_session_on_the_same_branch_reuses_the_checkout() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let first = ok(&create_with_target(&mut conn, &repo, "worktree:feat/x", 4).await);
    let second = ok(&create_with_target(&mut conn, &repo, "worktree:feat/x", 5).await);

    assert_eq!(first["workspace"], second["workspace"]);
    assert_ne!(
        first["session_id"], second["session_id"],
        "two sessions, one checkout — not one session"
    );
}

/// A branch that does not exist yet is created, not refused.
#[tokio::test]
#[ignore = "requires daemon"]
async fn a_name_no_branch_has_yet_becomes_a_new_branch_and_worktree() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let result = ok(&create_with_target(&mut conn, &repo, "worktree:brand-new", 6).await);
    let expected = repo.join("tree").join("brand-new");

    assert_eq!(result["workspace"], *expected.to_string_lossy());
    assert_eq!(
        git_stdout(&[
            "-C",
            &expected.to_string_lossy(),
            "rev-parse",
            "--abbrev-ref",
            "HEAD"
        ]),
        "brand-new"
    );
}

/// Fail-closed, through the whole stack.
///
/// The unit tests pin each layer's refusal separately; this pins that a refusal
/// actually reaches the caller as a failed create rather than being logged and
/// swallowed somewhere in between. A session that quietly ran on `master` when
/// it was told `feat/x` commits there.
#[tokio::test]
#[ignore = "requires daemon"]
async fn a_target_the_provider_refuses_refuses_the_session() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    // `..` would escape the destination template; the plugin rejects the name
    // before git ever sees it.
    let response = create_with_target(&mut conn, &repo, "worktree:../evil", 7).await;

    let error = response["error"]
        .as_object()
        .expect("create must be refused");
    let message = error["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("../evil"),
        "the refusal must name the target that could not be resolved: {message}"
    );
    assert!(
        !repo.parent().unwrap().join("evil").exists(),
        "nothing may be created outside the repo"
    );
}

#[tokio::test]
#[ignore = "requires daemon"]
async fn a_target_naming_a_provider_nobody_has_refuses_the_session() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let response = create_with_target(&mut conn, &repo, "nosuchplugin:main", 8).await;
    assert!(
        response["error"].is_object(),
        "an unknown provider must refuse, not fall back to the plain workspace"
    );
}

/// The ordinary path stays ordinary: no target, no resolution, no new failure
/// mode for the sessions that never asked for one.
#[tokio::test]
#[ignore = "requires daemon"]
async fn a_session_that_asks_for_no_target_is_created_against_the_workspace_given() {
    let (_tmp, repo) = a_repo();
    let (_daemon, mut conn) = daemon_and_conn().await;

    let result = ok(&conn
        .call_method(
            "session.create",
            json!({ "type": "chat", "workspace": repo }),
            9,
        )
        .await);

    assert_eq!(result["workspace"], *repo.to_string_lossy());
    assert!(
        !repo.join("tree").exists(),
        "a session that asked for no worktree must not get one"
    );
}
