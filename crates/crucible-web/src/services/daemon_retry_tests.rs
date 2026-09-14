//! A peer applies a request then drops its socket WITHOUT replying.
//! Reconnects use the same isolated listener, never the developer's daemon.
use super::*;
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::time::{timeout, Duration};

struct Peer {
    daemon: ReconnectingDaemon,
    applied: Arc<AtomicUsize>,
    subscriptions: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Peer {
    async fn losing_first_reply(method: &'static str) -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("daemon.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let applied = Arc::new(AtomicUsize::new(0));
        let subscriptions = Arc::new(AtomicUsize::new(0));
        let calls = applied.clone();
        let subs = subscriptions.clone();
        let task = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let (read, mut write) = socket.into_split();
                let mut lines = BufReader::new(read).lines();
                while let Some(line) = lines.next_line().await.unwrap() {
                    let request: Value = serde_json::from_str(&line).unwrap();
                    if request["method"] == method && calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        // The effect landed. Neither a reply nor an RPC error did.
                        break;
                    }
                    if request["method"] == "session.subscribe" {
                        assert_eq!(request["params"]["session_ids"], json!(["system"]));
                        subs.fetch_add(1, Ordering::SeqCst);
                    }
                    let result = if request["method"] == "plugin.commands" {
                        json!([])
                    } else {
                        json!({})
                    };
                    let reply = json!({"jsonrpc":"2.0", "id":request["id"], "result":result});
                    write
                        .write_all(format!("{reply}\n").as_bytes())
                        .await
                        .unwrap();
                    if request["method"] == "plugin.commands" {
                        let event = SessionEvent::new("system", "file_changed", json!({}));
                        write
                            .write_all(
                                format!("{}\n", serde_json::to_string(&event).unwrap()).as_bytes(),
                            )
                            .await
                            .unwrap();
                    }
                }
            }
        });
        let (client, rx) = DaemonClient::connect_to_with_events(&path).await.unwrap();
        let mut daemon = ReconnectingDaemon::new(client, rx, Arc::new(EventBroker::new()));
        daemon.reconnect_socket = Some(path);
        Self {
            daemon,
            applied,
            subscriptions,
            task,
            _dir: dir,
        }
    }
}

#[tokio::test]
async fn mutations_are_not_replayed_when_the_reply_is_lost() {
    for method in [
        "plugin.run_command",
        "plugin.option_execute",
        "session.send_message",
        "review.rebase",
    ] {
        let peer = Peer::losing_first_reply(method).await;
        let result = timeout(Duration::from_secs(2), async {
            match method {
                "plugin.run_command" => peer
                    .daemon
                    .plugin_run_command("test", json!({}))
                    .await
                    .map(|_| ()),
                "plugin.option_execute" => {
                    peer.daemon
                        .plugin_option_execute("test", vec!["run".into()])
                        .await
                }
                "session.send_message" => peer
                    .daemon
                    .session_send_message("s", "hello")
                    .await
                    .map(|_| ()),
                "review.rebase" => peer.daemon.review_rebase("s").await.map(|_| ()),
                _ => unreachable!(),
            }
        })
        .await
        .expect("a disconnected request must finish promptly");
        assert!(
            result.is_err(),
            "{method}: an ambiguous outcome must reach the caller"
        );
        assert_eq!(
            peer.applied.load(Ordering::SeqCst),
            1,
            "{method}: apply once"
        );
        // Recovery is still possible on the next read, without resending the write.
        peer.daemon.plugin_commands().await.unwrap();
        assert_eq!(peer.applied.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn replay_safe_reads_reconnect_once_and_restore_sticky_events() {
    let peer = Peer::losing_first_reply("plugin.commands").await;
    let mut events = peer.daemon.broker.subscribe("system").await;
    peer.daemon.subscribe_sticky("system").await.unwrap();
    let result = timeout(Duration::from_secs(2), peer.daemon.plugin_commands())
        .await
        .expect("read should reconnect promptly")
        .unwrap();
    assert!(result.is_empty());
    assert_eq!(peer.applied.load(Ordering::SeqCst), 2);
    assert_eq!(peer.subscriptions.load(Ordering::SeqCst), 2);
    let event = timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.event, "file_changed");
}
