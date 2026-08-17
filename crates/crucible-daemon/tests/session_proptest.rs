use crucible_core::session::{SessionState, SessionType};
use crucible_daemon::session_manager::{SessionError, SessionManager};
use crucible_daemon::test_support::{kiln_name, temp_session_manager};
use rand::{prelude::IndexedRandom, RngExt};
use tempfile::TempDir;

#[derive(Debug, Clone, Copy)]
enum SessionOp {
    Pause,
    Resume,
    End,
    Compact,
}

fn state_after_successful_op(current: SessionState, op: SessionOp) -> SessionState {
    match (current, op) {
        (SessionState::Active, SessionOp::Pause) => SessionState::Paused,
        (SessionState::Paused, SessionOp::Resume) => SessionState::Active,
        (_, SessionOp::End) => SessionState::Ended,
        (SessionState::Active, SessionOp::Compact) => SessionState::Compacting,
        _ => current,
    }
}

fn op_should_succeed(current: SessionState, op: SessionOp) -> bool {
    match (current, op) {
        (SessionState::Active, SessionOp::Pause) => true,
        (SessionState::Paused, SessionOp::Resume) => true,
        (SessionState::Ended, SessionOp::End) => false,
        (_, SessionOp::End) => true,
        (SessionState::Active, SessionOp::Compact) => true,
        _ => false,
    }
}

async fn apply_op(
    manager: &SessionManager,
    session_id: &str,
    op: SessionOp,
) -> Result<SessionState, SessionError> {
    match op {
        SessionOp::Pause => manager.pause_session(session_id).await,
        SessionOp::Resume => manager.resume_session(session_id).await,
        SessionOp::End => manager.end_session(session_id).await.map(|s| s.state),
        SessionOp::Compact => manager
            .request_compaction(session_id)
            .await
            .map(|s| s.state),
    }
}

#[tokio::test]
async fn state_transitions_follow_rules_fuzz() {
    let ops = [
        SessionOp::Pause,
        SessionOp::Resume,
        SessionOp::End,
        SessionOp::Compact,
    ];

    for _ in 0..50 {
        let _tmp = TempDir::new().unwrap();
        let manager = temp_session_manager();
        let session = manager
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
            .await
            .unwrap();

        let mut expected_state = SessionState::Active;
        let mut rng = rand::rng();
        let num_ops = rng.random_range(1..20);

        for _ in 0..num_ops {
            let op = *ops.choose(&mut rng).unwrap();
            let should_succeed = op_should_succeed(expected_state, op);
            let result = apply_op(&manager, &session.id, op).await;

            if should_succeed {
                assert!(
                    result.is_ok(),
                    "Op {:?} from state {:?} should succeed but got {:?}",
                    op,
                    expected_state,
                    result
                );
                expected_state = state_after_successful_op(expected_state, op);
            } else {
                assert!(
                    result.is_err(),
                    "Op {:?} from state {:?} should fail but got {:?}",
                    op,
                    expected_state,
                    result
                );
            }

            // One rule for every state, `Ended` included. This used to fork:
            // `Ended` was asserted as *absent* because ending evicted, so the
            // fuzzer could not check the state it left behind and a wrong final
            // state would have read as a correct disappearance.
            let actual = manager.get_session(&session.id).unwrap();
            assert_eq!(
                actual.state, expected_state,
                "State mismatch after {:?}: expected {:?}, got {:?}",
                op, expected_state, actual.state
            );
        }
    }
}

#[tokio::test]
async fn ended_sessions_reject_all_state_changes_fuzz() {
    let ops = [
        SessionOp::Pause,
        SessionOp::Resume,
        SessionOp::End,
        SessionOp::Compact,
    ];

    for _ in 0..20 {
        let _tmp = TempDir::new().unwrap();
        let manager = temp_session_manager();
        let session = manager
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
            .await
            .unwrap();

        manager.end_session(&session.id).await.unwrap();

        let mut rng = rand::rng();
        let num_ops = rng.random_range(1..10);

        for _ in 0..num_ops {
            let op = *ops.choose(&mut rng).unwrap();
            let result = apply_op(&manager, &session.id, op).await;

            assert!(
                result.is_err(),
                "Op {:?} on ended session should fail, got {:?}",
                op,
                result
            );

            // Resident throughout, and never leaves `Ended`. This is what makes
            // the assertion above meaningful: the ops are refused by the state
            // machine, not by the session having disappeared. Ending used to
            // evict, so every op errored with `NotFound` and the rejection of an
            // *ended* session was never actually exercised.
            assert_eq!(
                manager.get_session(&session.id).map(|s| s.state),
                Some(SessionState::Ended),
                "Op {op:?} must leave the ended session resident and ended"
            );
        }
    }
}

#[tokio::test]
async fn pause_resume_cycle_is_idempotent() {
    for cycles in 1..10 {
        let _tmp = TempDir::new().unwrap();
        let manager = temp_session_manager();
        let session = manager
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
            .await
            .unwrap();

        for _ in 0..cycles {
            manager.pause_session(&session.id).await.unwrap();
            let paused = manager.get_session(&session.id).unwrap();
            assert_eq!(paused.state, SessionState::Paused);

            manager.resume_session(&session.id).await.unwrap();
            let active = manager.get_session(&session.id).unwrap();
            assert_eq!(active.state, SessionState::Active);
        }
    }
}

#[tokio::test]
async fn concurrent_pause_requests_one_succeeds() {
    let _tmp = TempDir::new().unwrap();
    let manager = temp_session_manager();
    let session = manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();

    let session_id = session.id.clone();

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let m = manager.clone();
            let id = session_id.clone();
            tokio::spawn(async move { m.pause_session(&id).await })
        })
        .collect();

    let results: Vec<_> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.unwrap())
        .collect();

    let successes = results.iter().filter(|r| r.is_ok()).count();
    let failures = results.iter().filter(|r| r.is_err()).count();

    assert_eq!(successes, 1, "Exactly one pause should succeed");
    assert_eq!(failures, 9, "Nine pauses should fail with InvalidState");

    let final_state = manager.get_session(&session_id).unwrap();
    assert_eq!(final_state.state, SessionState::Paused);
}

#[tokio::test]
async fn concurrent_different_ops_maintain_consistency() {
    let _tmp = TempDir::new().unwrap();
    let manager = temp_session_manager();
    let session = manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();

    let session_id = session.id.clone();

    let pause_handle = {
        let m = manager.clone();
        let id = session_id.clone();
        tokio::spawn(async move { m.pause_session(&id).await })
    };

    let end_handle = {
        let m = manager.clone();
        let id = session_id.clone();
        tokio::spawn(async move { m.end_session(&id).await })
    };

    let (_pause_result, _end_result) = tokio::join!(pause_handle, end_handle);

    // Both interleavings converge, and the session is resident either way now
    // that ending does not evict. `persist_guard` serializes the two, so either
    // End ran first and Pause was refused (`pause_session` demands `Active`), or
    // Pause ran first and End accepted a `Paused` session. Ended, not Paused, and
    // never absent — the previous version allowed `None` and so could not tell
    // "End won" apart from "the session was lost".
    assert_eq!(
        manager.get_session(&session_id).map(|s| s.state),
        Some(SessionState::Ended),
        "whichever op won, the session must be resident and Ended"
    );
}
