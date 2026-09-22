//! Review-capture wiring: which calls get bracketed, how a delegation is
//! linked to its child, and that a session's ledger dies with the session.

use super::*;
use crate::agent_manager::messaging::review_capture::{delegated_child_id, needs_review_bracket};

fn manager() -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: temp_session_manager(),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        card_roots: Default::default(),
        review_snapshot_root: crate::test_support::scratch_snapshot_root(),
    })
}

/// A git repo with one committed file, so `capture_tree` has something to
/// hash and `top_level` resolves.
async fn git_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    // No files: `init_repo` would commit them, and this fixture's whole point
    // is an uncommitted worktree. `committed_git_repo` layers a commit on top
    // where a base is needed.
    crate::test_support::init_repo(dir.path(), &[]).await;
    tokio::fs::write(dir.path().join("a.txt"), "one\n")
        .await
        .expect("write");
    dir
}

#[test]
fn read_only_tools_are_not_bracketed() {
    assert!(!needs_review_bracket("read_file"));
    assert!(!needs_review_bracket("grep"));
    assert!(!needs_review_bracket("semantic_search"));
}

#[test]
fn writing_and_unknown_tools_are_bracketed() {
    assert!(needs_review_bracket("write_file"));
    assert!(needs_review_bracket("edit_file"));
    assert!(needs_review_bracket("bash"));
    // Not knowing what a third-party MCP tool does is a reason to
    // bracket it, not a reason to skip it.
    assert!(needs_review_bracket("mcp__vendor__do_something"));
}

#[test]
fn delegate_session_is_not_bracketed() {
    assert!(!needs_review_bracket("delegate_session"));
}

#[test]
fn child_id_is_read_from_the_delegation_result() {
    let result = serde_json::json!({
        "delegation_id": "sess-child",
        "child_session_id": "sess-child",
        "status": "completed",
        "result": "done",
    })
    .to_string();
    assert_eq!(delegated_child_id(&result).as_deref(), Some("sess-child"));
}

#[test]
fn an_unparsable_delegation_result_links_nothing() {
    assert_eq!(delegated_child_id("Error: delegation refused"), None);
    assert_eq!(delegated_child_id("{\"status\":\"failed\"}"), None);
}

/// Delegated children share the parent's workspace verbatim, so
/// overlapping brackets on one root are the default configuration, not an
/// edge case. Both sides must degrade rather than claim attribution.
#[tokio::test]
async fn overlapping_brackets_on_one_root_are_contested() {
    let repo = git_fixture().await;
    let manager = manager();
    for session in ["parent", "child"] {
        manager
            .review
            .open(session, &[repo.path().to_path_buf()])
            .await
            .expect("open ledger");
    }

    let parent = manager
        .review
        .open_bracket("parent")
        .await
        .expect("bracket");
    let child = manager.review.open_bracket("child").await.expect("bracket");

    tokio::fs::write(repo.path().join("a.txt"), "two\n")
        .await
        .expect("write");
    assert!(manager
        .review
        .close("child", child, "call-c", 1)
        .await
        .expect("close"));
    tokio::fs::write(repo.path().join("a.txt"), "three\n")
        .await
        .expect("write");
    assert!(manager
        .review
        .close("parent", parent, "call-p", 1)
        .await
        .expect("close"));

    for session in ["parent", "child"] {
        let ledger = manager.review.ledger(session).expect("ledger");
        assert!(
            ledger.intervals()[0].contested,
            "{session} overlapped another writer and must not claim attribution"
        );
    }
}

#[tokio::test]
async fn session_cleanup_drops_the_ledger() {
    let repo = git_fixture().await;
    let manager = manager();
    manager
        .review
        .open("s1", &[repo.path().to_path_buf()])
        .await
        .expect("open ledger");
    assert!(manager.review.is_open("s1"));

    manager.cleanup_session("s1");

    assert!(!manager.review.is_open("s1"));
}

/// The routing half of the harvest. `ReviewLedgers::harvest_and_clear` is
/// tested against the ledger directly; this is the wire that decides whether
/// anything ever calls it, and getting it wrong looks exactly like a session
/// tearing down normally.
#[tokio::test]
async fn cleaning_up_a_delegated_child_harvests_into_its_parent_first() {
    let repo = git_fixture().await;
    let manager = manager();
    let root = repo.path().to_path_buf();

    manager
        .review
        .open("parent", std::slice::from_ref(&root))
        .await
        .expect("parent ledger");
    manager
        .review
        .open("child", std::slice::from_ref(&root))
        .await
        .expect("child ledger");
    manager.review.set_parent("child", "parent");

    let handle = manager
        .review
        .open_bracket("child")
        .await
        .expect("child bracket");
    tokio::fs::write(repo.path().join("a.txt"), "by the child\n")
        .await
        .expect("write");
    manager
        .review
        .close("child", handle, "child-call", 3)
        .await
        .expect("close");

    manager.cleanup_session("child");

    // Spawned, because the harvest journals and `cleanup_session` is sync.
    let harvested = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let ledger = manager.review.ledger("parent").expect("parent ledger");
            if ledger
                .intervals()
                .iter()
                .any(|i| i.tool_call_id == "child-call")
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await;

    assert!(
        harvested.is_ok(),
        "the delegated child's attribution died with its ledger"
    );
    assert!(
        !manager.review.is_open("child"),
        "the child's ledger survived"
    );
}
