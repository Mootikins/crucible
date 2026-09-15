use crucible_core::note_edit::disk_hash;
use crucible_daemon::{DaemonClient, Server};
use serde_json::json;

#[tokio::test]
async fn two_clients_merge_writes_through_the_daemon() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let kiln = dir.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    let path = kiln.join("a.md");
    let base = "one\ntwo\nthree\n";
    std::fs::write(&path, base).unwrap();
    let socket = dir.path().join("daemon.sock");
    let server = Server::bind_with_data_home_and_kilns(
        &socket,
        dir.path().join("data"),
        &[("notes", &kiln)],
    )
    .await
    .unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());
    let a = DaemonClient::connect_to(&socket).await.unwrap();
    let b = DaemonClient::connect_to(&socket).await.unwrap();
    a.kiln_open(&kiln).await.unwrap();
    let request = |text: &str| {
        json!({ "path": path, "operation": "put", "content": text,
        "base_hash": disk_hash(base), "base_text": base })
    };
    let (left, right) = tokio::join!(
        a.call("fs.write", request("ONE\ntwo\nthree\n")),
        b.call("fs.write", request("one\ntwo\nTHREE\n")),
    );
    assert!(left.unwrap()["ok"].as_bool().unwrap());
    assert!(right.unwrap()["ok"].as_bool().unwrap());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "ONE\ntwo\nTHREE\n");
    let outside = dir.path().join("outside.md");
    let refused = a
        .call(
            "fs.write",
            json!({"path": outside, "operation": "put", "content": "no"}),
        )
        .await
        .unwrap();
    assert_eq!(refused["failure"], "not_found");
    assert!(!outside.exists());
    let _ = shutdown.send(());
    task.await.unwrap().unwrap();
}
