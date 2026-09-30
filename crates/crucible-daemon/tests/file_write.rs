use crucible_core::config::ProjectFileAccess;
use crucible_core::file_write::{
    ExpectedBase, FileChange, FileEncoding, FileReadRequest, FileWriteRequest,
};
use crucible_core::note_edit::disk_hash;
use crucible_core::protocol::RpcMethod;
use crucible_daemon::file_write::{
    read_for_roots, write_for_roots, write_many_for_roots, CheckedPut,
};
use crucible_daemon::{DaemonClient, Server};
use serde_json::{json, Value};

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
            client
                .call::<_, serde_json::Value>(RpcMethod::FsWrite, request.clone())
                .await
                .unwrap()["ok"],
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
        let result: serde_json::Value = client.call(RpcMethod::FsWrite, request).await.unwrap();
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
        a.call::<_, serde_json::Value>(RpcMethod::FsWrite, request("ONE\ntwo\nthree\n")),
        b.call::<_, serde_json::Value>(RpcMethod::FsWrite, request("one\ntwo\nTHREE\n")),
    );
    assert!(left.unwrap()["ok"].as_bool().unwrap());
    assert!(right.unwrap()["ok"].as_bool().unwrap());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "ONE\ntwo\nTHREE\n");
    let outside = dir.path().join("outside.md");
    let refused = a
        .call::<_, serde_json::Value>(
            RpcMethod::FsWrite,
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
        remove: false,
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
        client.call::<_, serde_json::Value>(RpcMethod::FsWrite, body)
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
        .call::<_, serde_json::Value>(
            RpcMethod::FsWrite,
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

/// Nested kilns: the innermost kiln that holds a path is its root, in each
/// order of the roots. A link in the inner kiln that points into the outer
/// kiln leaves its root, so the daemon refuses the write.
#[cfg(unix)]
#[tokio::test]
async fn the_innermost_kiln_contains_a_path_in_each_root_order() {
    let dir = tempfile::tempdir().unwrap();
    let outer = dir.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    let secret = outer.join("secret.md");
    std::fs::write(&secret, "secret").unwrap();
    let link = inner.join("link.md");
    std::os::unix::fs::symlink(&secret, &link).unwrap();
    for roots in [
        vec![outer.clone(), inner.clone()],
        vec![inner.clone(), outer.clone()],
    ] {
        let answer = write_for_roots(
            FileWriteRequest {
                path: link.to_string_lossy().into_owned(),
                change: FileChange::Put {
                    content: "overwritten".into(),
                    base_hash: None,
                    base_text: None,
                },
            },
            &roots,
            &[],
        )
        .await;
        assert_eq!(answer["failure"], "invalid", "roots {roots:?}: {answer}");
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret");
    }
}

fn read(path: &std::path::Path, encoding: FileEncoding) -> FileReadRequest {
    FileReadRequest {
        path: path.to_string_lossy().into_owned(),
        encoding,
    }
}

fn put_request(path: &std::path::Path, content: &str) -> FileWriteRequest {
    FileWriteRequest {
        path: path.to_string_lossy().into_owned(),
        change: FileChange::Put {
            content: content.into(),
            base_hash: None,
            base_text: None,
        },
    }
}

/// A read chooses its root by the same rule as a write: the innermost kiln,
/// in each order of the roots. The answer names that root, so a client uses
/// it as it is.
#[cfg(unix)]
#[tokio::test]
async fn a_read_names_the_innermost_kiln_and_refuses_a_link_out_of_it() {
    let dir = tempfile::tempdir().unwrap();
    let outer = dir.path().join("outer");
    let inner = outer.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(outer.join("secret.md"), "secret").unwrap();
    std::fs::write(inner.join("note.md"), "note").unwrap();
    std::os::unix::fs::symlink(outer.join("secret.md"), inner.join("link.md")).unwrap();
    for roots in [
        vec![outer.clone(), inner.clone()],
        vec![inner.clone(), outer.clone()],
    ] {
        let answer = read_for_roots(
            read(&inner.join("note.md"), FileEncoding::Text),
            &roots,
            &[],
        )
        .await;
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(
            answer["root"],
            json!(inner.canonicalize().unwrap()),
            "{answer}"
        );
        assert_eq!(answer["content"]["text"], "note");
        let answer = read_for_roots(
            read(&inner.join("link.md"), FileEncoding::Text),
            &roots,
            &[],
        )
        .await;
        assert_eq!(answer["failure"], "invalid", "roots {roots:?}: {answer}");
    }
}

/// A kiln inside a project wins over the project, so the project policy does
/// not reach the kiln. A project's own files obey its policy.
#[tokio::test]
async fn a_read_obeys_the_kiln_first_rule_and_the_project_policy() {
    let project = tempfile::tempdir().unwrap();
    let kiln = project.path().join("docs");
    std::fs::create_dir(&kiln).unwrap();
    std::fs::write(kiln.join("note.md"), "n").unwrap();
    std::fs::write(project.path().join("README.md"), "r").unwrap();
    let off = [(project.path().to_path_buf(), ProjectFileAccess::Off)];

    let answer = read_for_roots(
        read(&kiln.join("note.md"), FileEncoding::Text),
        std::slice::from_ref(&kiln),
        &off,
    )
    .await;
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["access"], "read-write", "{answer}");

    let readme = project.path().join("README.md");
    for (policy, expected) in [
        (ProjectFileAccess::ReadWrite, Some("read-write")),
        (ProjectFileAccess::ReadOnly, Some("read-only")),
        (ProjectFileAccess::Off, None),
    ] {
        let projects = [(project.path().to_path_buf(), policy)];
        let answer = read_for_roots(read(&readme, FileEncoding::Text), &[], &projects).await;
        match expected {
            Some(access) => {
                assert_eq!(answer["access"], access, "{answer}");
                assert_eq!(answer["content"]["text"], "r", "{answer}");
            }
            None => assert_eq!(answer["failure"], "not_found", "{answer}"),
        }
    }
}

/// A path in no root, a traversal sequence, a NUL and a relative path are
/// refused before any read.
#[tokio::test]
async fn a_read_refuses_a_path_in_no_root_and_a_malformed_path() {
    let kiln = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.md"), "s").unwrap();
    let roots = kiln_roots(kiln.path());

    let answer = read_for_roots(
        read(&outside.path().join("secret.md"), FileEncoding::Text),
        &roots,
        &[],
    )
    .await;
    assert_eq!(answer["failure"], "not_found", "{answer}");
    for raw in [
        format!("{}/../secret.md", kiln.path().display()),
        format!("{}/a\0b", kiln.path().display()),
        "notes/daily.md".to_string(),
    ] {
        let request = FileReadRequest {
            path: raw.clone(),
            encoding: FileEncoding::Text,
        };
        let answer = read_for_roots(request, &roots, &[]).await;
        assert_eq!(answer["failure"], "invalid", "{raw:?}: {answer}");
    }
}

/// A directory link out of the kiln carries neither a read nor a new file
/// out of it, and a planted link as the final component carries no write.
#[cfg(unix)]
#[tokio::test]
async fn a_link_out_of_the_kiln_carries_no_read_and_no_write() {
    let kiln = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.md");
    std::fs::write(&secret, "secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), kiln.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(&secret, kiln.path().join("evil.md")).unwrap();
    let roots = kiln_roots(kiln.path());

    let through_dir = kiln.path().join("escape/secret.md");
    let answer = read_for_roots(read(&through_dir, FileEncoding::Base64), &roots, &[]).await;
    assert_eq!(answer["failure"], "invalid", "{answer}");
    let new_file = kiln.path().join("escape/new.md");
    let answer = write_for_roots(put_request(&new_file, "x"), &roots, &[]).await;
    assert_eq!(answer["failure"], "invalid", "{answer}");
    assert!(!outside.path().join("new.md").exists());
    let answer = write_for_roots(put_request(&kiln.path().join("evil.md"), "x"), &roots, &[]).await;
    assert_eq!(answer["failure"], "invalid", "{answer}");
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret");
}

/// Text carries its hash, bytes travel as base64, a file that is not UTF-8
/// refuses a text read, and a missing file or a directory has no content.
#[tokio::test]
async fn a_read_answers_the_content_in_the_requested_encoding() {
    use base64::Engine as _;
    let kiln = tempfile::tempdir().unwrap();
    let roots = kiln_roots(kiln.path());
    let png = kiln.path().join("shot.png");
    let bytes = b"\x89PNG\r\n\x1a\n";
    std::fs::write(&png, bytes).unwrap();
    std::fs::write(kiln.path().join("note.md"), "wörld ✅\n").unwrap();

    let answer = read_for_roots(
        read(&kiln.path().join("note.md"), FileEncoding::Text),
        &roots,
        &[],
    )
    .await;
    assert_eq!(answer["content"]["text"], "wörld ✅\n", "{answer}");
    assert_eq!(answer["content"]["content_hash"], disk_hash("wörld ✅\n"));

    let answer = read_for_roots(read(&png, FileEncoding::Text), &roots, &[]).await;
    assert_eq!(answer["failure"], "unsupported", "{answer}");
    let answer = read_for_roots(read(&png, FileEncoding::Base64), &roots, &[]).await;
    let data = answer["content"]["data"].as_str().expect("base64 data");
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .unwrap(),
        bytes
    );

    for absent in [kiln.path().join("nope.md"), kiln.path().to_path_buf()] {
        let answer = read_for_roots(read(&absent, FileEncoding::Text), &roots, &[]).await;
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(answer["content"], Value::Null, "{answer}");
    }
}

/// `fs.write`'s content-size and path-containment gates, through the live
/// RPC method — the path the TUI and a Lua script use, not a web route.
///
/// The web's `put_note` used to re-check a note's size and its name's
/// traversal safety before calling `fs.write`, as if this gate did not
/// already exist. It does: `write_locked`'s `MAX_CONTENT_SIZE` and
/// `enclosing_root`'s path-containment check are this critical section's own
/// gates, shared by every caller of `fs.write`, including the browser. These
/// tests prove that directly, with no web route in the path, so deleting the
/// web's redundant pre-checks does not open a gap for the TUI or Lua either.
mod fs_write_size_and_containment {
    use super::*;

    #[tokio::test]
    async fn fs_write_refuses_content_over_the_size_limit() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let kiln = dir.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        let socket = dir.path().join("daemon.sock");
        let data = dir.path().join("data");

        let server = Server::bind_with_data_home_and_kilns(&socket, data, &[("notes", &kiln)])
            .await
            .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        client.kiln_open(&kiln).await.unwrap();

        let oversized = "x".repeat(10 * 1024 * 1024 + 1);
        let request = json!({
            "path": kiln.join("Too Big.md"),
            "operation": "put",
            "content": oversized,
        });
        let answer: Value = client.call(RpcMethod::FsWrite, request).await.unwrap();
        assert_eq!(answer["ok"], false, "{answer}");
        assert_eq!(answer["failure"], "invalid", "{answer}");
        assert!(!kiln.join("Too Big.md").exists());

        drop(client);
        shutdown.send(()).unwrap();
        task.await.unwrap().unwrap();
    }

    /// A path that escapes the kiln through a literal parent-directory
    /// component is refused before any byte is written, whether or not the
    /// caller pre-validated the name — `enclosing_root` checks the joined
    /// path, not the caller's own bookkeeping.
    #[tokio::test]
    async fn fs_write_refuses_a_path_that_escapes_the_kiln() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let kiln = dir.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        let socket = dir.path().join("daemon.sock");
        let data = dir.path().join("data");

        let server = Server::bind_with_data_home_and_kilns(&socket, data, &[("notes", &kiln)])
            .await
            .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        client.kiln_open(&kiln).await.unwrap();

        let escaping = format!("{}/{}/evil.md", kiln.display(), "..");
        let request = json!({
            "path": escaping,
            "operation": "put",
            "content": "stolen",
        });
        let answer: Value = client.call(RpcMethod::FsWrite, request).await.unwrap();
        assert_eq!(answer["ok"], false, "{answer}");
        assert!(!dir.path().join("evil.md").exists());

        drop(client);
        shutdown.send(()).unwrap();
        task.await.unwrap().unwrap();
    }
}
