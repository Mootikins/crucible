//! Tests for `AgentManager::{set,get,clear}_grammar`.
//!
//! Backend support is the load-bearing assertion: `set_grammar` must
//! hard-error against backends that don't support GBNF (currently
//! everything except `Mock`). Wave 2 Item 5 explicitly forbids silent
//! fallback.

use super::*;
use crucible_core::types::Grammar;

async fn make_session(provider: BackendType) -> (Arc<SessionManager>, AgentManager, String) {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut agent = test_agent();
    agent.provider = provider;
    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();
    let sid = session.id.clone();
    (session_manager, agent_manager, sid)
}

#[tokio::test]
async fn set_grammar_hard_errors_on_openai_backend() {
    // OpenAI doesn't speak GBNF. Per the Wave 2 design plan, the
    // attach call must fail loudly rather than silently dropping the
    // constraint on the next agent turn.
    let (_, am, sid) = make_session(BackendType::OpenAI).await;
    let grammar = Grammar::named("yn", r#"root ::= "yes" | "no""#);

    let err = am.set_grammar(&sid, grammar, None).await.unwrap_err();
    match err {
        AgentError::NotSupported(msg) => {
            assert!(
                msg.contains("openai"),
                "want backend name in msg, got: {msg}"
            );
            assert!(
                msg.contains("GBNF") || msg.contains("grammar"),
                "want grammar-related msg, got: {msg}"
            );
        }
        other => panic!("expected NotSupported, got {other:?}"),
    }

    // And the session config must NOT have been mutated.
    let attached = am.get_grammar(&sid).unwrap();
    assert!(
        attached.is_none(),
        "grammar must remain detached on hard error"
    );
}

#[tokio::test]
async fn set_grammar_hard_errors_on_anthropic_backend() {
    let (_, am, sid) = make_session(BackendType::Anthropic).await;
    let grammar = Grammar::new(r#"root ::= "a""#);
    let err = am.set_grammar(&sid, grammar, None).await.unwrap_err();
    assert!(matches!(err, AgentError::NotSupported(_)));
}

#[tokio::test]
async fn set_grammar_attaches_when_backend_supports_it() {
    // Mock is the only `supports_grammar() == true` backend today —
    // it stands in for llama-cpp until that backend lands.
    let (_, am, sid) = make_session(BackendType::Mock).await;
    let grammar = Grammar::named("yn", r#"root ::= "yes" | "no""#);

    am.set_grammar(&sid, grammar.clone(), None).await.unwrap();

    let attached = am.get_grammar(&sid).unwrap().expect("grammar must persist");
    assert_eq!(attached, grammar);
}

#[tokio::test]
async fn clear_grammar_removes_attached_grammar() {
    let (_, am, sid) = make_session(BackendType::Mock).await;
    let g = Grammar::new(r#"root ::= "hi""#);
    am.set_grammar(&sid, g, None).await.unwrap();
    assert!(am.get_grammar(&sid).unwrap().is_some());

    am.clear_grammar(&sid, None).await.unwrap();
    assert!(am.get_grammar(&sid).unwrap().is_none());
}

#[tokio::test]
async fn clear_grammar_is_idempotent_when_none_attached() {
    let (_, am, sid) = make_session(BackendType::Mock).await;
    // Never attached → clearing must succeed.
    am.clear_grammar(&sid, None).await.unwrap();
    assert!(am.get_grammar(&sid).unwrap().is_none());
}

#[tokio::test]
async fn set_grammar_returns_session_not_found_for_unknown_session() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let am = create_test_agent_manager(session_manager);
    let err = am
        .set_grammar("does-not-exist", Grammar::new(r#"root ::= "x""#), None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, AgentError::SessionNotFound(_)),
        "got: {err:?}"
    );
}
