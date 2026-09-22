use crate::support::mock_agent::make_prompt_request;
use crate::support::{connect, prompt_with, MockScript, Step};
use crucible_daemon::acp::StreamingChunk;

const MAX_SUBAGENT_OUTPUT: usize = 10 * 1024 * 1024;

#[tokio::test]
async fn concurrent_dual_sessions_isolated_no_cross_contamination() {
    let (mut client_a, _agent_a) = connect(MockScript::default(), None, None).await;
    let (mut client_b, _agent_b) = connect(MockScript::default(), None, None).await;

    let (session_a, session_b) = tokio::join!(
        client_a.handshake(None, None),
        client_b.handshake(None, None)
    );

    let session_a = session_a.expect("session A should connect");
    let session_b = session_b.expect("session B should connect");

    assert_ne!(session_a.id(), session_b.id(), "sessions must be distinct");
    assert!(session_a.id().starts_with("mock-session-"));
    assert!(session_b.id().starts_with("mock-session-"));
}

#[tokio::test]
async fn stream_edge_chunk_ordering_preserved_per_stream_with_parallel_streams() {
    let text = |chunks: [&str; 3]| MockScript {
        turn: chunks.into_iter().map(|c| Step::Text(c.into())).collect(),
        ..MockScript::default()
    };
    let (client_a, _agent_a) = connect(text(["A-1", "A-2", "A-3"]), Some(300), None).await;
    let (client_b, _agent_b) = connect(text(["B-1", "B-2", "B-3"]), Some(300), None).await;

    let seen_a = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen_b = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let seen_a_cb = seen_a.clone();
    let stream_a = tokio::spawn(async move {
        let request = make_prompt_request("session-a", "stream A");
        prompt_with(&client_a, request, move |chunk| {
            if let StreamingChunk::Text(text) = chunk {
                seen_a_cb.lock().unwrap().push(text);
            }
            true
        })
        .await
        .unwrap();
    });

    let seen_b_cb = seen_b.clone();
    let stream_b = tokio::spawn(async move {
        let request = make_prompt_request("session-b", "stream B");
        prompt_with(&client_b, request, move |chunk| {
            if let StreamingChunk::Text(text) = chunk {
                seen_b_cb.lock().unwrap().push(text);
            }
            true
        })
        .await
        .unwrap();
    });

    let (turn_a, turn_b) = tokio::join!(stream_a, stream_b);
    turn_a.unwrap();
    turn_b.unwrap();

    assert_eq!(&*seen_a.lock().unwrap(), &["A-1", "A-2", "A-3"]);
    assert_eq!(&*seen_b.lock().unwrap(), &["B-1", "B-2", "B-3"]);
}

#[tokio::test]
async fn stream_edge_large_response_near_max_output_is_accumulated() {
    let large_text = "x".repeat(MAX_SUBAGENT_OUTPUT - 4096);
    let expected_len = large_text.len();
    let script = MockScript {
        turn: vec![Step::Text(large_text)],
        ..MockScript::default()
    };
    let (client, _agent) = connect(script, Some(1_000), None).await;

    let request = make_prompt_request("large-session", "big stream");
    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("large streaming response should succeed");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert_eq!(content.len(), expected_len);
    assert!(content.starts_with('x'));
    assert!(content.ends_with('x'));
    assert!(!summary.announced_any);
}

#[tokio::test]
async fn stream_edge_empty_response_returns_empty_content() {
    // The default script ends each turn with `end_turn` and sends no chunk.
    let (client, _agent) = connect(MockScript::default(), Some(200), None).await;

    let request = make_prompt_request("empty-session", "respond with nothing");
    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("empty response should still complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert!(content.is_empty(), "no chunks should produce empty content");
    assert!(!summary.announced_any);
    assert!(!summary.produced_content);
}
