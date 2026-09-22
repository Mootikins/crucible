//! A spike for the move to the `agent-client-protocol` SDK client.
//!
//! The daemon keeps one ACP connection for the life of an agent handle. The
//! handle is `Send` and many tasks use it, so the SDK connection must run on a
//! spawned tokio task. Its `ConnectionTo<Agent>` must also go to other tasks.
//! This test proves those properties over tokio pipes with the `compat`
//! adapters, and with a paused tokio clock.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    PromptRequest, PromptResponse, SessionConfigOption, SessionId, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, StopReason,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// The channel that receives the updates of the turn that runs now.
type TurnSlot = Arc<Mutex<Option<mpsc::UnboundedSender<SessionUpdate>>>>;

/// An agent that streams one chunk for a prompt. It then holds the turn
/// until `session/cancel` arrives, and ends it with `cancelled`.
async fn held_turn_agent(
    transport: ByteStreams<
        impl futures::AsyncWrite + Send + 'static,
        impl futures::AsyncRead + Send + 'static,
    >,
) {
    let held: Arc<Mutex<Option<oneshot::Sender<()>>>> = Arc::default();
    let on_cancel = Arc::clone(&held);
    Agent
        .builder()
        .on_receive_request(
            async |req: InitializeRequest, responder, _cx| {
                responder.respond(InitializeResponse::new(req.protocol_version))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                cx.send_notification(SessionNotification::new(
                    req.session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::from(
                        "first words",
                    ))),
                ))?;
                let (tx, rx) = oneshot::channel();
                *held.lock().expect("held lock") = Some(tx);
                // The dispatch loop waits for a handler. To read the cancel,
                // the held turn waits on its own task.
                cx.spawn(async move {
                    let _ = rx.await;
                    responder.respond(PromptResponse::new(StopReason::Cancelled))
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async |_req: SetSessionConfigOptionRequest, responder, _cx| {
                responder.respond(SetSessionConfigOptionResponse::new(Vec::<
                    SessionConfigOption,
                >::new(
                )))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            async move |_n: CancelNotification, _cx| {
                if let Some(tx) = on_cancel.lock().expect("held lock").take() {
                    let _ = tx.send(());
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
        .expect("the agent connection ends cleanly");
}

#[tokio::test(start_paused = true)]
async fn the_sdk_client_connection_runs_on_a_spawned_task() {
    let (agent_end, client_end) = tokio::io::duplex(16 * 1024);
    let (a_read, a_write) = tokio::io::split(agent_end);
    let (c_read, c_write) = tokio::io::split(client_end);
    let agent = tokio::spawn(held_turn_agent(ByteStreams::new(
        a_write.compat_write(),
        a_read.compat(),
    )));

    // The client connection: notifications go to the turn channel that is
    // installed now. `connect_with` gives the connection to its closure
    // only, so the closure sends it out and then waits for a stop signal.
    let turn: TurnSlot = Arc::default();
    let route = Arc::clone(&turn);
    let (cx_tx, cx_rx) = oneshot::channel::<ConnectionTo<Agent>>();
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let driver = tokio::spawn(
        Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    if let Some(tx) = route.lock().expect("turn lock").as_ref() {
                        let _ = tx.send(n.update);
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .connect_with(
                ByteStreams::new(c_write.compat_write(), c_read.compat()),
                async move |cx| {
                    let _ = cx_tx.send(cx);
                    let _ = stop_rx.await;
                    Ok(())
                },
            ),
    );
    let cx = cx_rx.await.expect("the driver sends its connection");

    // A request deadline on the paused clock does not fire while the agent
    // answers.
    let init = tokio::time::timeout(
        Duration::from_secs(30),
        cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
            .block_task(),
    )
    .await
    .expect("initialize inside the deadline")
    .expect("initialize succeeds");
    assert_eq!(init.protocol_version, ProtocolVersion::V1);

    // A turn on another task, with a clone of the connection.
    let (update_tx, mut update_rx) = mpsc::unbounded_channel();
    *turn.lock().expect("turn lock") = Some(update_tx);
    let session = SessionId::from("s-1".to_string());
    let prompt = tokio::spawn({
        let cx = cx.clone();
        let session = session.clone();
        async move {
            cx.send_request(PromptRequest::new(session, vec![ContentBlock::from("go")]))
                .block_task()
                .await
        }
    });
    let first = update_rx.recv().await.expect("the held turn streams");
    assert!(matches!(first, SessionUpdate::AgentMessageChunk(_)));

    // A request completes while the turn is held.
    tokio::time::timeout(
        Duration::from_secs(30),
        cx.send_request(SetSessionConfigOptionRequest::new(
            session.clone(),
            "model",
            "other",
        ))
        .block_task(),
    )
    .await
    .expect("set_config_option inside the deadline, while the turn is held")
    .expect("set_config_option succeeds");

    // A deadline on the paused clock fires for a turn the agent holds.
    let held = tokio::time::timeout(Duration::from_secs(300), async {
        while !prompt.is_finished() {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(held.is_err(), "the agent holds the turn until a cancel");

    // `session/cancel` ends the held turn.
    cx.send_notification(CancelNotification::new(session))
        .expect("send session/cancel");
    let response = prompt
        .await
        .expect("the prompt task")
        .expect("the prompt response");
    assert_eq!(response.stop_reason, StopReason::Cancelled);

    let _ = stop_tx.send(());
    driver
        .await
        .expect("the driver task")
        .expect("the client connection ends cleanly");
    agent.await.expect("the agent task");
}
