use crucible_core::file_write::{ExpectedBase, FileChange, FileWriteRequest};
use crucible_core::note_edit::disk_hash;
use crucible_daemon::file_write::{write_for_roots, write_many_for_roots, CheckedPut};
use crucible_daemon::{DaemonClient, Server};
use serde_json::json;

#[tokio::test]
async fn retrying_an_unacknowledged_write_after_restart_preserves_the_other_writer() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let kiln = dir.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    let path = kiln.join("retry.md");
    let socket = dir.path().join("daemon.sock");
    let data = dir.path().join("data");
    let base = "one\ntwo\nthree\n";
    let ours = "ONE\ntwo\nthree\n";
    for (theirs, expected) in [
        ("ONE\ntwo\nTHREE\n", Some("ONE\ntwo\nTHREE\n")),
        ("different\ntwo\nthree\n", None),
    ] {
        std::fs::write(&path, base).unwrap();
        let server =
            Server::bind_with_data_home_and_kilns(&socket, data.clone(), &[("notes", &kiln)])
                .await
                .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        client.kiln_open(&kiln).await.unwrap();
        let request = json!({ "path": path, "operation": "put", "content": ours, "base_hash": disk_hash(base), "base_text": base });
        assert_eq!(
            client.call("fs.write", request.clone()).await.unwrap()["ok"],
            true
        );
        // Treat the acknowledgment as lost: the caller retains the original
        // pending write. No daemon memory survives the next attempt.
        drop(client);
        shutdown.send(()).unwrap();
        task.await.unwrap().unwrap();
        std::fs::write(&path, theirs).unwrap();
        let server =
            Server::bind_with_data_home_and_kilns(&socket, data.clone(), &[("notes", &kiln)])
                .await
                .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        client.kiln_open(&kiln).await.unwrap();
        let result = client.call("fs.write", request).await.unwrap();
        if let Some(expected) = expected {
            assert_eq!(result["ok"], true, "{result}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        } else {
            assert_eq!(result["ok"], false, "{result}");
            assert!(
                !result["regions"].as_array().unwrap().is_empty(),
                "{result}"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), theirs);
        }
        drop(client);
        shutdown.send(()).unwrap();
        task.await.unwrap().unwrap();
    }
}

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

fn kiln_roots(kiln: &std::path::Path) -> Vec<std::path::PathBuf> {
    vec![kiln.to_path_buf()]
}

fn put(path: &std::path::Path, content: &str, base: ExpectedBase) -> CheckedPut {
    CheckedPut {
        path: path.to_string_lossy().into_owned(),
        content: content.into(),
        base,
    }
}

#[tokio::test]
async fn an_absent_base_with_no_file_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.md");
    let answer = write_many_for_roots(
        vec![put(&path, "fresh\n", ExpectedBase::Absent)],
        &kiln_roots(dir.path()),
        &[],
    )
    .await;
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
}

#[tokio::test]
async fn an_absent_base_with_an_empty_file_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.md");
    std::fs::write(&path, "").unwrap();
    // The wire form `base_hash: ""` with no text is the absent base.
    let answer = write_for_roots(
        FileWriteRequest {
            path: path.to_string_lossy().into_owned(),
            change: FileChange::Put {
                content: "fresh\n".into(),
                base_hash: Some(String::new()),
                base_text: None,
            },
        },
        &kiln_roots(dir.path()),
        &[],
    )
    .await;
    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["stale_base"], true, "{answer}");
    assert_eq!(answer["current_hash"], disk_hash(""), "{answer}");
    assert_eq!(answer["current_content"], "", "{answer}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}

#[tokio::test]
async fn a_failed_second_path_leaves_the_first_path_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a.md");
    let created = dir.path().join("sub/c.md");
    let stale = dir.path().join("b.md");
    std::fs::write(&first, "a before\n").unwrap();
    std::fs::write(&stale, "b on disk\n").unwrap();
    let answer = write_many_for_roots(
        vec![
            put(&first, "a after\n", ExpectedBase::Unchecked),
            put(&created, "c after\n", ExpectedBase::Absent),
            put(
                &stale,
                "b after\n",
                ExpectedBase::Hash {
                    hash: disk_hash("b before\n"),
                },
            ),
        ],
        &kiln_roots(dir.path()),
        &[],
    )
    .await;
    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["current_hash"], disk_hash("b on disk\n"), "{answer}");
    assert!(
        answer["path"].as_str().unwrap().ends_with("b.md"),
        "{answer}"
    );
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "a before\n");
    assert!(!created.exists(), "the absent file must stay absent");
    assert_eq!(std::fs::read_to_string(&stale).unwrap(), "b on disk\n");

    // A path outside every root refuses the whole set before any write.
    let outside = tempfile::tempdir().unwrap();
    let answer = write_many_for_roots(
        vec![
            put(&first, "a after\n", ExpectedBase::Unchecked),
            put(&outside.path().join("x.md"), "x", ExpectedBase::Unchecked),
        ],
        &kiln_roots(dir.path()),
        &[],
    )
    .await;
    assert_eq!(answer["failure"], "not_found", "{answer}");
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "a before\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_overlapping_writes_do_not_deadlock() {
    let dir = tempfile::tempdir().unwrap();
    let roots = kiln_roots(dir.path());
    let a = dir.path().join("a.md");
    let b = dir.path().join("b.md");
    let mut tasks = Vec::new();
    for i in 0..200 {
        let (first, second) = if i % 2 == 0 { (&a, &b) } else { (&b, &a) };
        let requests = vec![
            put(first, &format!("{i}\n"), ExpectedBase::Unchecked),
            put(second, &format!("{i}\n"), ExpectedBase::Unchecked),
        ];
        let roots = roots.clone();
        tasks.push(tokio::spawn(async move {
            write_many_for_roots(requests, &roots, &[]).await
        }));
    }
    let all = futures::future::join_all(tasks);
    let answers = tokio::time::timeout(std::time::Duration::from_secs(20), all)
        .await
        .expect("two writes that lock the same paths must not deadlock");
    for answer in answers {
        assert_eq!(answer.unwrap()["ok"], true);
    }
    // Each set writes both paths under both locks, so the two files agree.
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        std::fs::read_to_string(&b).unwrap()
    );
}

#[tokio::test]
async fn fs_write_fields_map_to_the_same_answers() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let kiln = dir.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    let path = kiln.join("a.md");
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
    let client = DaemonClient::connect_to(&socket).await.unwrap();
    client.kiln_open(&kiln).await.unwrap();
    let base = "one\ntwo\nthree\n";
    let disk = "one\ntwo\nTHREE\n";
    let write = |body: serde_json::Value| {
        let mut body = body;
        body["path"] = json!(path);
        client.call("fs.write", body)
    };

    // No base: the write replaces the text with no check.
    std::fs::write(&path, "old\n").unwrap();
    let answer = write(json!({"operation": "put", "content": disk}))
        .await
        .unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["content_hash"], disk_hash(disk), "{answer}");

    // A text with no hash, or a text that does not hash to the hash, is invalid.
    for body in [
        json!({"operation": "put", "content": "x", "base_text": base}),
        json!({"operation": "put", "content": "x", "base_text": base, "base_hash": "nope"}),
    ] {
        let answer = write(body).await.unwrap();
        assert_eq!(answer["failure"], "invalid", "{answer}");
    }

    // A stale hash with no text answers only the current hash.
    let answer = write(json!({"operation": "put", "content": "x", "base_hash": disk_hash(base)}))
        .await
        .unwrap();
    assert_eq!(
        answer,
        json!({"ok": false, "current_hash": disk_hash(disk)}),
        "{answer}"
    );

    // A stale hash with its text merges.
    let answer = write(json!({"operation": "put", "content": "ONE\ntwo\nthree\n",
        "base_hash": disk_hash(base), "base_text": base}))
    .await
    .unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["merged"], true, "{answer}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "ONE\ntwo\nTHREE\n");

    // A stale patch answers the current hash and no failed edits.
    let answer = write(
        json!({"operation": "patch", "edits": [{"expect": "two", "replace": "2"}],
        "base_hash": disk_hash(base)}),
    )
    .await
    .unwrap();
    assert_eq!(answer["ok"], false, "{answer}");
    assert_eq!(answer["stale_base"], true, "{answer}");
    assert_eq!(answer["failed"], json!([]), "{answer}");

    // An empty hash with no text on an absent file writes.
    let fresh = kiln.join("fresh.md");
    let answer = client
        .call(
            "fs.write",
            json!({"path": fresh, "operation": "put", "content": "new\n", "base_hash": ""}),
        )
        .await
        .unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "new\n");

    drop(client);
    let _ = shutdown.send(());
    task.await.unwrap().unwrap();
}
