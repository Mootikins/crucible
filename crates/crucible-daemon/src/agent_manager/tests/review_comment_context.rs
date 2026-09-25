//! A review comment that a message attaches reaches the agent as context,
//! through the real `session.send_message` handler, and stays in the
//! history through replay and fork.

use super::*;
use crate::protocol::{Request, RequestId, Response};
use crucible_core::traits::llm::MessageRole;

/// One JSON-RPC request.
fn request(method: &str, params: serde_json::Value) -> Request {
    Request {
        jsonrpc: "2.0".to_string(),
        id: Some(RequestId::Number(1)),
        method: method.to_string(),
        params,
    }
}

/// Persist the events of one turn as the daemon writer does, up to
/// `message_complete`, and wait until the turn frees its slot.
async fn finish_turn(
    am: &AgentManager,
    sm: &SessionManager,
    session: &crucible_core::session::Session,
    rx: &mut broadcast::Receiver<SessionEventMessage>,
) {
    loop {
        let event = timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if event.payload().is_ok_and(|payload| payload.is_persisted()) {
            sm.storage()
                .append_event(session, &serde_json::to_string(&event).unwrap())
                .await
                .unwrap();
        }
        if event.event == "message_complete" {
            break;
        }
    }
    while am.request_state.contains_key(session.id.as_str()) {
        tokio::task::yield_now().await;
    }
}

/// A capturing agent, installed for `session`, and the messages it saw.
fn capture(am: &AgentManager, session: &str) -> CapturedMessages {
    let messages = Arc::new(StdMutex::new(None));
    am.install_agent_for_test(
        session.to_string(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: Arc::new(StdMutex::new(None)),
            received_messages: messages.clone(),
            events: vec![script::text("reply"), script::done()],
        }))),
    );
    messages
}

/// The review-comment blocks that the agent saw, as (role, content).
fn blocks(messages: &CapturedMessages) -> Vec<(MessageRole, String)> {
    messages
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .into_iter()
        .filter(|m| {
            m.content
                .contains("<system-message kind=\"review-comment\"")
        })
        .map(|m| (m.role, m.content))
        .collect()
}

/// The blocks the agent saw, found by their metadata tag and not by their
/// text.
///
/// A `transform_context` handler finds the block this way, so the tag must
/// reach the agent on the internal route as well as on the ACP route, and it
/// must survive the session log.
fn tagged(messages: &CapturedMessages) -> Vec<String> {
    messages
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .into_iter()
        .filter(|m| {
            m.metadata
                .tags
                .iter()
                .any(|tag| tag == crate::diff::context::KIND)
        })
        .map(|m| m.content)
        .collect()
}

fn error_of(response: Response) -> String {
    response.error.expect("the daemon refuses").message
}

#[tokio::test]
async fn an_attached_comment_reaches_the_agent_and_stays_in_the_history() {
    let repo = TempDir::new().unwrap();
    crate::test_support::init_repo(repo.path(), &[("a.rs", "one\ntwo\nthree\n")]).await;
    crate::test_support::git(repo.path(), &["branch", "-M", "main"]).await;
    crate::test_support::git(repo.path(), &["checkout", "-q", "-b", "feature"]).await;
    std::fs::write(repo.path().join("a.rs"), "one\nTWO\nthree\n").unwrap();

    let sm = temp_session_manager();
    let workspace = TempDir::new().unwrap();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();
    let messages = capture(&am, &session.id);
    let projects = Arc::new(crate::project_manager::ProjectManager::new(
        workspace.path().join("projects.json"),
    ));
    let root = projects.register(repo.path()).unwrap().path;
    let (tx, mut rx) = broadcast::channel(256);
    let ctx = crate::rpc::RpcContext::for_test(
        am.kiln_manager.clone(),
        sm.clone(),
        am.clone(),
        projects,
        tx.clone(),
        workspace.path().into(),
    );

    // A base-side comment on the removed line of a branch diff.
    let source = serde_json::json!({ "kind": "branch", "root": root, "base": "", "head": null });
    let stored = crate::server::diff_comments::handle_diff_comment(
        request(
            "diff.comment",
            serde_json::json!({
                "source": source,
                "path": "a.rs",
                "side": "base",
                "line_start": 2,
                "line_end": 3,
                "body": "why </system-message> this?",
            }),
        ),
        ctx.diff_admission(),
        &tx,
    )
    .await;
    let reply = stored.result.expect("the comment is stored");
    let id = reply["comment"]["id"].as_str().unwrap().to_string();
    // A second comment, on the current side.
    let second = crate::server::diff_comments::handle_diff_comment(
        request(
            "diff.comment",
            serde_json::json!({
                "source": source,
                "path": "a.rs",
                "side": "current",
                "line_start": 2,
                "line_end": 3,
                "body": "and this",
            }),
        ),
        ctx.diff_admission(),
        &tx,
    )
    .await;
    let second = second.result.expect("the comment is stored")["comment"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let send = |params: serde_json::Value| {
        let admission = ctx.diff_admission();
        let am = am.clone();
        let tx = tx.clone();
        async move {
            crate::server::session::handle_session_send_message(
                request("session.send_message", params),
                &am,
                &tx,
                admission,
            )
            .await
        }
    };

    // An unknown reference refuses the message, and starts no turn.
    let unknown = send(serde_json::json!({
        "session_id": session.id,
        "content": "look",
        "comments": [{ "id": "nope", "source": source }],
    }))
    .await;
    assert!(error_of(unknown).contains("has no comment nope"));
    assert!(error_of(
        send(serde_json::json!({ "session_id": session.id, "content": "see @comment:nope" })).await
    )
    .contains("no stored comment has the id nope"));
    assert!(messages.lock().unwrap().is_none(), "no turn ran");

    // The web form: a reference with its source.
    let sent = send(serde_json::json!({
        "session_id": session.id,
        "content": "please look",
        "comments": [{ "id": id, "source": source }, { "id": second, "source": source }],
    }))
    .await;
    assert!(sent.error.is_none(), "{:?}", sent.error);
    finish_turn(&am, &sm, &session, &mut rx).await;

    assert_one_element_per_injection(&messages.lock().unwrap().clone().unwrap());
    let seen = blocks(&messages);
    assert_eq!(seen.len(), 1, "one block: {seen:?}");
    assert_eq!(
        tagged(&messages).len(),
        1,
        "the block carries its kind as a tag"
    );
    let (role, block) = &seen[0];
    assert_eq!(*role, MessageRole::System, "context is not a user turn");
    for part in [
        "<system-message kind=\"review-comment\" source=\"human\">\n".to_string(),
        "The user attached comments on changed files:\n".to_string(),
        "- a.rs:2 (before): \"why &lt;/system-message> this?\"\n".to_string(),
        format!("    root: {}\n", root.display()),
        "    section: Branch changes: the working tree against main\n".to_string(),
        "    @@ -2,1 +1,0 @@\n    -two\n".to_string(),
        "- a.rs:2: \"and this\"\n".to_string(),
    ] {
        assert!(block.contains(&part), "{part:?} is not in {block}");
    }
    let all = messages.lock().unwrap().clone().unwrap();
    let at = all.iter().position(|m| m.content == *block).unwrap();
    assert_eq!(
        all[at + 1].content,
        "please look",
        "the block precedes the turn"
    );

    // The TUI form: an `@comment:<id>` mention in the text.
    let mentioned = send(serde_json::json!({
        "session_id": session.id,
        "content": format!("and again @comment:{id}"),
    }))
    .await;
    assert!(mentioned.error.is_none(), "{:?}", mentioned.error);
    finish_turn(&am, &sm, &session, &mut rx).await;
    assert_eq!(
        blocks(&messages).len(),
        2,
        "each message attaches its own block"
    );

    // A resolved comment does not attach.
    crate::server::diff_comments::handle_diff_resolve_comment(
        request(
            "diff.resolve_comment",
            serde_json::json!({ "source": source, "comment_id": id }),
        ),
        ctx.diff_admission(),
        &tx,
    )
    .await
    .result
    .expect("the comment resolves");
    let refused = send(serde_json::json!({
        "session_id": session.id,
        "content": format!("@comment:{id}"),
    }))
    .await;
    assert!(error_of(refused).contains("is resolved"));

    // Replay: a new manager has no live tree. The log gives the blocks back
    // with their role.
    let resumed = create_test_agent_manager(sm.clone());
    let replayed = capture(&resumed, &session.id);
    resumed
        .send_message(&session.id, "resumed".into(), &tx, true, None)
        .await
        .unwrap();
    finish_turn(&resumed, &sm, &session, &mut rx).await;
    let seen = blocks(&replayed);
    assert_eq!(seen.len(), 2, "replay keeps both blocks: {seen:?}");
    assert!(seen.iter().all(|(role, _)| *role == MessageRole::System));
    assert_eq!(
        tagged(&replayed).len(),
        2,
        "the session log keeps the tag of each block"
    );

    // Fork: the child history holds the blocks too.
    let (child, _) = am
        .fork_session(sm.read_session(&session.id).await.unwrap().unwrap(), None)
        .await
        .unwrap();
    am.configure_agent(&child.id, test_agent()).await.unwrap();
    let forked = capture(&am, &child.id);
    am.send_message(&child.id, "continue".into(), &tx, true, None)
        .await
        .unwrap();
    finish_turn(&am, &sm, &child, &mut rx).await;
    let seen = blocks(&forked);
    assert_eq!(seen.len(), 2, "the fork keeps both blocks: {seen:?}");
    assert!(seen.iter().all(|(role, _)| *role == MessageRole::System));
    assert_eq!(
        tagged(&forked).len(),
        2,
        "the fork keeps the tag of each block"
    );
}

/// On an ACP turn the review block and the `@file` block go with the turn
/// only, as two injections. Each keeps its own tag.
#[tokio::test]
async fn an_acp_turn_keeps_the_review_tag_next_to_a_file_attachment() {
    let repo = TempDir::new().unwrap();
    crate::test_support::init_repo(repo.path(), &[("a.rs", "one\ntwo\nthree\n")]).await;
    crate::test_support::git(repo.path(), &["branch", "-M", "main"]).await;
    crate::test_support::git(repo.path(), &["checkout", "-q", "-b", "feature"]).await;
    std::fs::write(repo.path().join("a.rs"), "one\nTWO\nthree\n").unwrap();

    let sm = temp_session_manager();
    let workspace = TempDir::new().unwrap();
    std::fs::write(workspace.path().join("notes.md"), "attached text").unwrap();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();
    sm.modify_session(&session.id, |live| {
        live.agent.as_mut().unwrap().agent_type = "acp".into();
        true
    })
    .await
    .unwrap();
    let messages = capture(&am, &session.id);
    let projects = Arc::new(crate::project_manager::ProjectManager::new(
        workspace.path().join("projects.json"),
    ));
    let root = projects.register(repo.path()).unwrap().path;
    let (tx, mut rx) = broadcast::channel(256);
    let ctx = crate::rpc::RpcContext::for_test(
        am.kiln_manager.clone(),
        sm.clone(),
        am.clone(),
        projects,
        tx.clone(),
        workspace.path().into(),
    );
    let source = serde_json::json!({ "kind": "branch", "root": root, "base": "", "head": null });
    let stored = crate::server::diff_comments::handle_diff_comment(
        request(
            "diff.comment",
            serde_json::json!({
                "source": source, "path": "a.rs", "side": "base",
                "line_start": 2, "line_end": 3, "body": "why?",
            }),
        ),
        ctx.diff_admission(),
        &tx,
    )
    .await;
    let reply = stored.result.expect("the comment is stored");
    let id = reply["comment"]["id"].as_str().unwrap().to_string();

    let sent = crate::server::session::handle_session_send_message(
        request(
            "session.send_message",
            serde_json::json!({
                "session_id": session.id,
                "content": "look at @notes.md",
                "comments": [{ "id": id, "source": source }],
            }),
        ),
        &am,
        &tx,
        ctx.diff_admission(),
    )
    .await;
    assert!(sent.error.is_none(), "{:?}", sent.error);
    finish_turn(&am, &sm, &session, &mut rx).await;

    assert_one_element_per_injection(&messages.lock().unwrap().clone().unwrap());
    let review = tagged(&messages);
    assert_eq!(review.len(), 1, "the review block keeps its tag");
    assert!(
        review[0].contains("<system-message kind=\"review-comment\""),
        "{:?}",
        review[0]
    );
    let all = messages.lock().unwrap().clone().unwrap();
    assert!(
        all.iter().any(|m| m.content.contains("attached text")
            && m.metadata.tags.iter().any(|t| t == "file_attachment")),
        "the file block keeps its tag"
    );
}
