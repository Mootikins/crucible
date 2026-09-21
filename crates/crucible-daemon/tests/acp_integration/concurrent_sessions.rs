use crate::scripted_agent::{
    client_with_custom_transport, final_response, make_prompt_request, read_request_id, text_chunk,
    write_json_line,
};
use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};
use crucible_core::config::AcpConfig;
use crucible_core::test_support::EnvVarGuard;
use crucible_daemon::acp::discovery::{discover_agent, reset_agent_cache};
use crucible_daemon::acp::StreamingChunk;
use tempfile::TempDir;
use tokio::sync::Barrier;

const MAX_SUBAGENT_OUTPUT: usize = 10 * 1024 * 1024;

#[tokio::test]
async fn concurrent_dual_sessions_isolated_no_cross_contamination() {
    let (mut client_a, _handle_a) =
        ThreadedMockAgent::spawn_with_client(MockStdioAgentConfig::opencode());
    let (mut client_b, _handle_b) =
        ThreadedMockAgent::spawn_with_client(MockStdioAgentConfig::opencode());

    let (session_a, session_b) = tokio::join!(
        client_a.connect_with_best_mcp(None),
        client_b.connect_with_best_mcp(None)
    );

    let session_a = session_a.expect("session A should connect");
    let session_b = session_b.expect("session B should connect");

    assert_ne!(session_a.id(), session_b.id(), "sessions must be distinct");
    assert!(session_a.id().starts_with("mock-session-"));
    assert!(session_b.id().starts_with("mock-session-"));
}

/// The process-wide discovery cache answers without probing until it is
/// reset, and a reset forces the next call to probe again.
///
/// The agent binary is removed between the calls, so a cache hit and a fresh
/// probe give different answers: a hit still names the removed agent, and a
/// probe finds nothing. `PATH` holds only the fake agent and `which`, so no
/// agent installed on the host can answer the probe.
#[tokio::test]
async fn discovery_cache_serves_a_removed_agent_until_it_is_reset() {
    reset_agent_cache();

    let agent_dir = TempDir::new().expect("temp dir for the fake agent");
    let fake_agent_path = agent_dir.path().join("opencode");
    std::fs::write(
        &fake_agent_path,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo fake-opencode-1.0\n  exit 0\nfi\nexit 0\n",
    )
    .expect("write fake agent script");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake_agent_path, std::fs::Permissions::from_mode(0o755))
            .expect("make the fake agent executable");
    }

    // Discovery probes with `which`, so `PATH` must still reach it.
    let tool_dir = TempDir::new().expect("temp dir for which");
    let which = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("which"))
        .find(|candidate| candidate.is_file())
        .expect("`which` is on the test host's PATH");
    std::os::unix::fs::symlink(&which, tool_dir.path().join("which")).expect("link which");

    let path = std::env::join_paths([agent_dir.path(), tool_dir.path()]).expect("join PATH");
    let _path_guard = EnvVarGuard::set("PATH", path.to_string_lossy().into_owned());

    let config = AcpConfig::default();
    let discovered = discover_agent(None, &config)
        .await
        .expect("discovery finds the fake agent on PATH");
    assert_eq!(discovered.name, "opencode");
    assert_eq!(discovered.command, "opencode");

    std::fs::remove_file(&fake_agent_path).expect("remove the fake agent");

    let cached = discover_agent(None, &config)
        .await
        .expect("a cache hit does not probe, so the removed agent is still served");
    assert_eq!(cached.name, "opencode");

    reset_agent_cache();
    let after_reset = discover_agent(None, &config).await;
    assert!(
        after_reset.is_err(),
        "after a reset the probe must run again and find no agent, got: {after_reset:?}"
    );
}

#[tokio::test]
async fn stream_edge_chunk_ordering_preserved_per_stream_with_parallel_streams() {
    let (mut client_a, mut agent_a_reader, mut agent_a_writer) =
        client_with_custom_transport(Some(300));
    let (mut client_b, mut agent_b_reader, mut agent_b_writer) =
        client_with_custom_transport(Some(300));
    let barrier = std::sync::Arc::new(Barrier::new(3));

    let barrier_a = barrier.clone();
    let agent_task_a = tokio::spawn(async move {
        let request_id = read_request_id(&mut agent_a_reader).await;

        barrier_a.wait().await;
        for chunk in ["A-1", "A-2", "A-3"] {
            write_json_line(&mut agent_a_writer, text_chunk("session-a", chunk)).await;
        }
        write_json_line(&mut agent_a_writer, final_response(request_id)).await;
    });

    let barrier_b = barrier.clone();
    let agent_task_b = tokio::spawn(async move {
        let request_id = read_request_id(&mut agent_b_reader).await;

        barrier_b.wait().await;
        for chunk in ["B-1", "B-2", "B-3"] {
            write_json_line(&mut agent_b_writer, text_chunk("session-b", chunk)).await;
        }
        write_json_line(&mut agent_b_writer, final_response(request_id)).await;
    });

    let seen_a = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen_b = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let seen_a_cb = seen_a.clone();
    let stream_a = tokio::spawn(async move {
        let request = make_prompt_request("session-a", "stream A");
        client_a
            .send_prompt_with_callback(
                request,
                Box::new(move |chunk| {
                    if let StreamingChunk::Text(text) = chunk {
                        seen_a_cb.lock().unwrap().push(text);
                    }
                    true
                }),
            )
            .await
            .unwrap();
    });

    let seen_b_cb = seen_b.clone();
    let stream_b = tokio::spawn(async move {
        let request = make_prompt_request("session-b", "stream B");
        client_b
            .send_prompt_with_callback(
                request,
                Box::new(move |chunk| {
                    if let StreamingChunk::Text(text) = chunk {
                        seen_b_cb.lock().unwrap().push(text);
                    }
                    true
                }),
            )
            .await
            .unwrap();
    });

    barrier.wait().await;

    let (turn_a, turn_b) = tokio::join!(stream_a, stream_b);
    turn_a.unwrap();
    turn_b.unwrap();

    assert_eq!(&*seen_a.lock().unwrap(), &["A-1", "A-2", "A-3"]);
    assert_eq!(&*seen_b.lock().unwrap(), &["B-1", "B-2", "B-3"]);

    let (_a, _b) = tokio::join!(agent_task_a, agent_task_b);
}

#[tokio::test]
async fn stream_edge_large_response_near_max_output_is_accumulated() {
    let (mut client, mut agent_reader, mut agent_writer) =
        client_with_custom_transport(Some(1_000));

    let large_text = "x".repeat(MAX_SUBAGENT_OUTPUT - 4096);
    let expected_len = large_text.len();

    tokio::spawn(async move {
        let request_id = read_request_id(&mut agent_reader).await;

        write_json_line(&mut agent_writer, text_chunk("large-session", &large_text)).await;
        write_json_line(&mut agent_writer, final_response(request_id)).await;
    });

    let request = make_prompt_request("large-session", "big stream");
    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, _response) = client
        .send_prompt_with_callback(request, callback)
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
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(200));

    tokio::spawn(async move {
        let request_id = read_request_id(&mut agent_reader).await;

        write_json_line(&mut agent_writer, final_response(request_id)).await;
    });

    let request = make_prompt_request("empty-session", "respond with nothing");
    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, _response) = client
        .send_prompt_with_callback(request, callback)
        .await
        .expect("empty response should still complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert!(content.is_empty(), "no chunks should produce empty content");
    assert!(!summary.announced_any);
    assert!(!summary.produced_content);
}
