//! A daemon notification crosses the socket into the TUI notification area.

use super::super::*;
use tokio::sync::mpsc;

/// A plugin `cru.log.notify` on a real daemon reaches the TUI as the same
/// notification, with its kind.
#[tokio::test(flavor = "multi_thread")]
async fn a_plugin_notification_reaches_the_tui_over_the_socket() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = crucible_daemon::Server::bind_with_plugin_config(
        crucible_daemon::BindWithPluginConfigParams {
            path: socket.clone(),
            data_home: Some(tmp.path().join("data")),
            config_home: Some(tmp.path().join("config")),
            ..Default::default()
        },
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());

    let (client, event_rx) = loop {
        match crucible_daemon::DaemonClient::connect_to_with_events(&socket).await {
            Ok(connected) => break connected,
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
        }
    };
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    tokio::spawn(session_event_consumer(
        "my-session".to_string(),
        event_rx,
        msg_tx,
        None,
    ));

    let message = "a notice from the daemon";
    // The daemon accepts only after the plugin boot, so the VM has its sink.
    let code = format!("cru.log.notify({message:?}, cru.log.levels.WARN)");
    client
        .call("lua.eval", serde_json::json!({ "code": code }))
        .await
        .expect("lua.eval");

    let notification = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match msg_rx.recv().await.expect("the consumer ended") {
                ChatAppMsg::Notification(n) if n.message == message => return n,
                _ => continue,
            }
        }
    })
    .await
    .expect("the notification never reached the TUI");
    assert_eq!(
        notification.kind,
        crucible_core::types::NotificationKind::Warning
    );
}

/// A TUI that attaches to a session reads the notifications that the
/// session already has, over the socket, and not those of another session.
#[tokio::test(flavor = "multi_thread")]
async fn an_attaching_tui_reads_the_notifications_of_its_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let socket = tmp.path().join("d.sock");
    let server = crucible_daemon::Server::bind_with_plugin_config(
        crucible_daemon::BindWithPluginConfigParams {
            path: socket.clone(),
            data_home: Some(tmp.path().join("data")),
            config_home: Some(tmp.path().join("config")),
            ..Default::default()
        },
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());
    let client = crucible_daemon::DaemonClient::connect_to(&socket)
        .await
        .expect("connect");

    let mut ids = Vec::new();
    for message in ["mine", "other"] {
        let created = client
            .call("session.create", serde_json::json!({ "type": "chat" }))
            .await
            .expect("session.create");
        let id = created["session_id"].as_str().unwrap().to_string();
        let notification = crucible_core::types::Notification::warning(message);
        client
            .call(
                "session.add_notification",
                serde_json::json!({ "session_id": id, "notification": notification }),
            )
            .await
            .expect("session.add_notification");
        ids.push(id);
    }

    let msgs = OilChatRunner::notification_msgs(&client, &ids[0]).await;
    assert!(
        matches!(&msgs[..], [ChatAppMsg::Notification(n)] if n.message == "mine"),
        "{msgs:?}"
    );
}

/// A daemon on a temporary socket, and a client of it.
async fn daemon(tmp: &tempfile::TempDir) -> crucible_daemon::DaemonClient {
    let socket = tmp.path().join("d.sock");
    let server = crucible_daemon::Server::bind_with_plugin_config(
        crucible_daemon::BindWithPluginConfigParams {
            path: socket.clone(),
            data_home: Some(tmp.path().join("data")),
            config_home: Some(tmp.path().join("config")),
            ..Default::default()
        },
    )
    .await
    .expect("the daemon binds");
    tokio::spawn(server.run());
    crucible_daemon::DaemonClient::connect_to(&socket)
        .await
        .expect("connect")
}

/// The TUI closes a shared notice over the socket. The daemon hides it for
/// the session of the TUI only: an attach of that session does not read it
/// again, and another session still reads it.
#[tokio::test(flavor = "multi_thread")]
async fn a_tui_close_hides_a_shared_notice_for_its_session_only() {
    let tmp = tempfile::TempDir::new().unwrap();
    let client = daemon(&tmp).await;
    let mut sessions = Vec::new();
    for _ in 0..2 {
        let created = client
            .call("session.create", serde_json::json!({ "type": "chat" }))
            .await
            .expect("session.create");
        sessions.push(created["session_id"].as_str().unwrap().to_string());
    }
    client
        .call(
            "lua.eval",
            serde_json::json!({ "code": "cru.log.notify('shared')" }),
        )
        .await
        .expect("lua.eval");
    // The hub stores what `cru.log.notify` queued on its own task.
    let shared = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let msgs = OilChatRunner::notification_msgs(&client, &sessions[0]).await;
            if let [ChatAppMsg::Notification(n)] = &msgs[..] {
                return n.id.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the hub never stored the notice");

    let failures = OilChatRunner::close_notification_msgs(&client, &sessions[0], &[shared]).await;

    assert!(failures.is_empty(), "{failures:?}");
    let mine = OilChatRunner::notification_msgs(&client, &sessions[0]).await;
    assert!(mine.is_empty(), "{mine:?}");
    let theirs = OilChatRunner::notification_msgs(&client, &sessions[1]).await;
    assert!(
        matches!(&theirs[..], [ChatAppMsg::Notification(n)] if n.message == "shared"),
        "{theirs:?}"
    );
}

/// A close that the daemon cannot take reaches the user as a warning.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_tui_close_is_a_warning() {
    let tmp = tempfile::TempDir::new().unwrap();
    let client = daemon(&tmp).await;

    let failures =
        OilChatRunner::close_notification_msgs(&client, "no-such-session", &["n".into()]).await;

    assert!(
        matches!(&failures[..], [ChatAppMsg::Error(e)] if e.contains("could not close")),
        "{failures:?}"
    );
}

/// Run `:messages clear` through the real `process_action`. Answer the
/// count of daemon calls that it started.
async fn closes_started(is_replay: bool) -> usize {
    use crucible_oil::terminal::Terminal;

    let mut runner = OilChatRunner::with_terminal(Terminal::with_size(80, 24));
    runner.is_replay = is_replay;
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::Notification(
        crucible_core::types::Notification::warning("shared"),
    ));
    // Real keystrokes, so the test runs the parse of the command too.
    for c in ":messages clear".chars() {
        app.update(crate::tui::oil::event::Event::Key(
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char(c),
                crossterm::event::KeyModifiers::NONE,
            ),
        ));
    }
    let action = app.update(crate::tui::oil::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    let daemon = crate::test_daemon::FakeDaemon::answering_null("chat-1").await;
    let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
    let mut background_tasks = Vec::new();
    runner
        .process_action(ProcessActionParams {
            action,
            app: &mut app,
            session: Some(&daemon.session),
            msg_tx: &msg_tx,
            background_tasks: &mut background_tasks,
        })
        .await
        .expect("process_action does not fail");

    // The test runtime runs one thread, and nothing yielded, so the close
    // never reaches a daemon before this abort.
    let started = background_tasks.len();
    OilChatRunner::abort_background_tasks(&mut background_tasks);
    started
}

#[tokio::test]
async fn messages_clear_starts_the_close_in_the_daemon() {
    assert_eq!(closes_started(false).await, 1);
}

#[tokio::test]
async fn a_replay_closes_nothing_in_the_daemon() {
    assert_eq!(closes_started(true).await, 0);
}
