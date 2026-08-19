//! What a `session_id` off the wire is allowed to reach.
//!
//! Every session method resolves `{sessions_root}/{session_id}` and then reads,
//! rewrites or removes what it finds. `Path::join` normalizes nothing and an
//! absolute component replaces the base outright, so before `SessionId` existed
//! `session.delete` with `"../Documents"` removed a directory beside the
//! sessions root and `session.archive` rewrote a `meta.json` inside one.
//!
//! These run against the real dispatcher over a real socket because that is
//! where the string still exists: inside the daemon the id is a `SessionId`
//! and a traversing one cannot be constructed at all.

use super::*;

/// Every spelling of "somewhere else" a caller might try.
const HOSTILE_IDS: &[&str] = &[
    "../Documents",
    "../../Documents",
    "..",
    ".",
    "sub/dir",
    "..\\Documents",
];

/// A directory beside the sessions root, holding something worth keeping.
///
/// Creates the sessions root too. A daemon that has filed a session has one,
/// and without it `{sessions_root}/../Documents` fails to resolve for a reason
/// that has nothing to do with the check under test — which is how a version of
/// this test passed against the vulnerable code.
fn bystander(server: &TestServer) -> PathBuf {
    std::fs::create_dir_all(server.sessions_root()).unwrap();
    let dir = server.tmp.path().join("Documents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("taxes.pdf"), "keep me").unwrap();
    dir
}

fn session_request(id: u64, method: &str, session_id: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": { "session_id": session_id },
    })
}

#[tokio::test]
async fn session_delete_never_removes_a_directory_outside_the_sessions_root() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let doomed = bystander(&server);
    // An absolute id needs no `..` at all: as a component it replaces the base.
    let elsewhere = TempDir::new().unwrap();
    std::fs::write(elsewhere.path().join("keep"), "me too").unwrap();

    for (n, id) in HOSTILE_IDS
        .iter()
        .map(|s| (*s).to_string())
        .chain([elsewhere.path().to_string_lossy().into_owned()])
        .enumerate()
    {
        let response = rpc_call(
            &mut client,
            session_request(800 + n as u64, "session.delete", &id),
        )
        .await;
        assert!(
            response["error"].is_object(),
            "session.delete accepted {id:?}: {response}"
        );
    }

    assert!(
        doomed.join("taxes.pdf").exists(),
        "session.delete removed a directory outside the sessions root"
    );
    assert!(
        elsewhere.path().join("keep").exists(),
        "session.delete removed the directory an absolute id named"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn session_archive_never_rewrites_a_file_outside_the_sessions_root() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    // A directory the caller can name but does not own, holding something that
    // parses as a session — a synced folder, a backup, another user's kiln.
    // Archive reads `meta.json`, flips `archived`, and writes it back.
    let dir = bystander(&server);
    let planted = crucible_core::session::Session::new(
        crucible_core::session::SessionType::Chat,
        vec![crucible_core::config::KilnName::parse("kiln").unwrap()],
    );
    let original = serde_json::to_string_pretty(&planted).unwrap();
    std::fs::write(dir.join("meta.json"), &original).unwrap();

    for (n, method) in ["session.archive", "session.unarchive"].iter().enumerate() {
        let response = rpc_call(
            &mut client,
            session_request(820 + n as u64, method, "../Documents"),
        )
        .await;
        assert!(
            response["error"].is_object(),
            "{method} accepted a traversing id: {response}"
        );
    }

    assert_eq!(
        std::fs::read_to_string(dir.join("meta.json")).unwrap(),
        original,
        "archive rewrote a meta.json outside the sessions root"
    );
    server.shutdown().await;
}

/// The read side of the same join. These handlers key on an id precisely so a
/// caller cannot name a directory; an unvalidated id gave the naming back.
#[tokio::test]
async fn the_observe_handlers_never_read_a_transcript_outside_the_sessions_root() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let dir = bystander(&server);
    std::fs::write(
        dir.join("session.jsonl"),
        "{\"type\":\"user\",\"ts\":\"2026-01-01T12:00:01Z\",\"content\":\"MY-BANK-PASSWORD\"}\n",
    )
    .unwrap();

    for (n, method) in [
        "session.load_events",
        "session.render_markdown",
        "session.resume_from_storage",
        // The write sink of the family: an accepted traversing id would put
        // `session.md` beside the transcript it was not allowed to read.
        "session.export_to_file",
    ]
    .iter()
    .enumerate()
    {
        let response = rpc_call(
            &mut client,
            session_request(840 + n as u64, method, "../Documents"),
        )
        .await;
        assert!(
            response["error"].is_object(),
            "{method} accepted a traversing id: {response}"
        );
        assert!(
            !response.to_string().contains("MY-BANK-PASSWORD"),
            "{method} returned a transcript from outside the sessions root: {response}"
        );
    }
    server.shutdown().await;
}

/// A refusal has to say which parameter and why. "Session not found" would send
/// the caller looking for a session, and an unlabeled internal error would send
/// them to the daemon log.
#[tokio::test]
async fn a_refused_session_id_is_an_invalid_params_error_naming_the_parameter() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let response = rpc_call(
        &mut client,
        session_request(860, "session.delete", "../Documents"),
    )
    .await;

    assert_eq!(response["error"]["code"], crate::protocol::INVALID_PARAMS);
    let message = response["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("session_id"),
        "the error must name the parameter: {message}"
    );
    server.shutdown().await;
}

/// The ids the daemon actually mints still work. A validator that rejected them
/// would be "secure" and useless, and this crate has shipped exactly that
/// before — `observe::SessionId::parse` accepted `chat-20260104-1530-a1b2`
/// while `Session::new` minted `chat-2026-01-04T1530-a1b2c3`.
#[tokio::test]
async fn a_minted_session_id_survives_the_boundary() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
    let session_id = create_chat_session(&mut client, TestServer::KILN, 880).await;

    let response = rpc_call(
        &mut client,
        session_request(881, "session.delete", &session_id),
    )
    .await;

    assert!(
        response["error"].is_null(),
        "a real session id was refused: {response}"
    );
    assert_eq!(response["result"]["deleted"], true);
    assert!(!server.sessions_root().join(&session_id).exists());
    server.shutdown().await;
}

/// `session.send_message` obeys the same rule as `session.delete`.
///
/// It did not. `get_or_revive_session` mapped a parse failure to
/// `AgentError::SessionNotFound`, whose Display is "Session not found: …", and
/// the handler passed every error through `internal_error`. So a caller bug
/// arrived as `-32603` with a message telling them to look for a session, and
/// crucible-web maps anything that is not `-32602` to HTTP 502 — a daemon
/// fault. Four wrong answers to one malformed string, on the busiest method on
/// the surface, while `session.delete` two files over got it right.
#[tokio::test]
async fn send_message_refuses_a_malformed_session_id_by_naming_the_parameter() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

    let response = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 890,
            "method": "session.send_message",
            "params": { "session_id": "../Documents", "content": "hello" },
        }),
    )
    .await;

    assert_eq!(
        response["error"]["code"],
        crate::protocol::INVALID_PARAMS,
        "a malformed id is the caller's bug, not the daemon's: {response}"
    );
    let message = response["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("session_id"),
        "the error must name the parameter, not send them hunting for a \
         session that was never named: {message}"
    );
    server.shutdown().await;
}
