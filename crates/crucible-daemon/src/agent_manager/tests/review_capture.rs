//! Review-capture wiring: which calls get bracketed, how a delegation is
//! linked to its child, and that a session's ledger dies with the session.

use super::*;
use crate::agent_manager::messaging::review_capture::{delegated_child_id, needs_review_bracket};
use crucible_core::agent::ToolPolicy;
use crucible_core::session::ReviewState;

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

/// A git repo with one committed file, for the full-turn fixture below.
/// [`git_fixture`] deliberately leaves its file uncommitted; the composed diff
/// needs a base commit to sit against.
async fn committed_git_repo() -> tempfile::TempDir {
    let dir = git_fixture().await;
    for args in [vec!["add", "."], vec!["commit", "-q", "-m", "init"]] {
        let status = tokio::process::Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .status()
            .await
            .expect("git");
        assert!(status.success(), "git {args:?} failed");
    }
    dir
}

/// The first event named `event` — with a longer budget than
/// `next_event_or_skip`'s two seconds, because this turn builds a session
/// dispatcher and opens a kiln before it reaches the gate.
async fn wait_for_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(30), async {
        loop {
            match rx.recv().await {
                Ok(msg) if msg.event == event => return msg,
                Ok(_) => continue,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(err) => panic!("event channel closed waiting for {event}: {err}"),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {event}"))
}

/// The narrowed capture bracket, end to end.
///
/// The review gate is an unbounded wait on a human. When the bracket opened
/// above it, everything the user did while the call was parked — including the
/// revert that rejecting a hunk performs, which is what releases the gate —
/// landed inside the held call's interval and was reported as the agent's
/// work. The bracket now opens below the gate.
///
/// This has to be a manager-level test: `ReviewLedgers` sees only a pair of
/// tree SHAs, so a human write and a tool write inside one window are
/// identical evidence to it. Where the bracket opens relative to the gate is a
/// property of `handle_tool_call_in_stream` and of nothing below it.
#[tokio::test]
async fn a_human_edit_during_the_review_gates_hold_is_not_attributed_to_the_held_call() {
    let repo = committed_git_repo().await;
    // The kiln is a separate directory so the session's own `.crucible/`
    // storage does not land inside the repo under review.
    let kiln = tempfile::tempdir().expect("kiln tempdir");

    let session_manager = temp_session_manager();
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![kiln.path().to_path_buf()],
            Some(repo.path().to_path_buf()),
            None,
        )
        .await
        .expect("create session");
    let manager = create_test_agent_manager(session_manager.clone());

    // An earlier turn's write, left unreviewed. This is what the gate holds
    // the next call on. Opening the ledger here also makes `send_message`'s
    // own `open` the documented no-op, so `session_base` is this commit.
    manager
        .review
        .open(&session.id, &[repo.path().to_path_buf()])
        .await
        .expect("open ledger");
    let earlier = manager
        .review
        .open_bracket(&session.id)
        .await
        .expect("bracket");
    tokio::fs::write(repo.path().join("a.txt"), "written earlier\n")
        .await
        .expect("write");
    assert!(manager
        .review
        .close(&session.id, earlier, "call-earlier", 0)
        .await
        .expect("close"));

    // The card allows `write_file` so the permission prompt — the *other*
    // unbounded wait, and not what this test is about — never opens.
    let mut agent = test_agent();
    agent.tool_policy = Some(
        [("write_file".to_string(), ToolPolicy::Allow)]
            .into_iter()
            .collect(),
    );
    manager
        .configure_agent(&session.id, agent)
        .await
        .expect("configure agent");
    manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::tool_call(
                "call-held",
                "write_file",
                serde_json::json!({ "path": "a.txt", "content": "written by the tool\n" }),
            )],
        }) as BoxedAgentHandle)),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(256);
    let (_message_id, done) = manager
        .send_message_notified(&session.id, "go".to_string(), &event_tx, true, None)
        .await
        .expect("send");

    let gate = wait_for_event(&mut event_rx, "review_gate").await;
    assert_eq!(
        gate.data["blocked"],
        serde_json::json!(true),
        "the turn must be parked before the human edits anything"
    );

    // The human, in their own editor, while the call is parked.
    tokio::fs::write(repo.path().join("b.txt"), "typed by hand\n")
        .await
        .expect("write");

    // ...and then they review the hunk that was blocking, releasing the gate.
    let held = manager
        .review
        .list_hunks(&session.id)
        .await
        .expect("list hunks")
        .into_iter()
        .find(|h| h.path.contains("a.txt"))
        .expect("the blocking hunk");
    manager
        .review
        .set_state(&session.id, &held.id, ReviewState::Accepted)
        .await
        .expect("accept");

    timeout(Duration::from_secs(60), done)
        .await
        .expect("turn did not finish")
        .expect("turn outcome");

    let hunks = manager.review.list_hunks(&session.id).await.expect("hunks");
    let b = hunks
        .iter()
        .find(|h| h.path.contains("b.txt"))
        .expect("the human's file");
    assert!(
        b.is_external(),
        "the human's edit during the gate's hold was attributed to {:?}",
        b.tool_call_ids
    );
    let a = hunks
        .iter()
        .find(|h| h.path.contains("a.txt"))
        .expect("the tool's file");
    assert!(
        a.tool_call_ids.iter().any(|id| id == "call-held"),
        "narrowing the bracket cost the call its own attribution: {:?}",
        a.tool_call_ids
    );
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
