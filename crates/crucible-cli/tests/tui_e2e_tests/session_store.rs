//! The TUI holds no session identity of its own.
//!
//! `cru chat` used to make up an id (`chat-%Y%m%d-%H%M%S`) before the daemon
//! made the real one. It made a folder under that id in the daemon's session
//! store, and it opened the Lua session under that id. The daemon could not
//! load the folder, so every start of the TUI added one unreadable entry to
//! the session listing. Only a spawned binary and a real daemon show that, so
//! this is a PTY test.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::tui_e2e_harness::{TuiTestConfig, TuiTestSession};

/// The session folders under the child's data root.
fn session_folders(home: &Path) -> Vec<PathBuf> {
    let root = home.join(".crucible/sessions");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    folders.sort();
    folders
}

/// A folder the daemon wrote. The daemon writes `meta.json` when it makes a
/// session, and it cannot load a folder without one.
fn is_daemon_session(folder: &Path) -> bool {
    folder.join("meta.json").is_file()
}

/// Wait for the daemon to write the session of this TUI, and return its id.
fn wait_for_daemon_session(home: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(folder) = session_folders(home)
            .into_iter()
            .find(|f| is_daemon_session(f))
        {
            return folder.file_name().unwrap().to_string_lossy().into_owned();
        }
        assert!(
            Instant::now() < deadline,
            "the daemon wrote no session; folders: {:?}",
            session_folders(home)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Ask the child's daemon whether a Lua session exists under `session_id`.
///
/// `lua.register_commands` with no commands changes nothing, and it answers
/// "session not found" for an id that has no Lua session.
fn lua_session_exists(home: &Path, session_id: &str) -> Result<(), String> {
    let socket = home.join("crucible.sock");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let client = crucible_daemon::DaemonClient::connect_to(&socket)
            .await
            .map_err(|e| format!("connect: {e}"))?;
        client
            .call(
                "lua.register_commands",
                serde_json::json!({ "session_id": session_id, "commands": [] }),
            )
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

#[test]
#[ignore = "requires: cru binary"]
fn cru_chat_makes_no_session_identity_of_its_own() {
    let mut session =
        TuiTestSession::spawn(TuiTestConfig::new("chat").with_env("RUST_LOG", "warn"))
            .expect("spawn");
    session.wait_for_ready().expect("TUI ready");
    let home = session.home().to_path_buf();

    let daemon_id = wait_for_daemon_session(&home);

    // The Lua session opens after the daemon session, so allow it a moment.
    let deadline = Instant::now() + Duration::from_secs(10);
    let lua = loop {
        match lua_session_exists(&home, &daemon_id) {
            Ok(()) => break Ok(()),
            Err(e) if Instant::now() >= deadline => break Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    };

    session.send(":quit\r").expect("quit");
    session.expect_eof().expect("runner exited");

    lua.unwrap_or_else(|e| {
        panic!("the Lua session must use the daemon's session id {daemon_id}: {e}")
    });
    let foreign: Vec<PathBuf> = session_folders(&home)
        .into_iter()
        .filter(|f| !is_daemon_session(f))
        .collect();
    assert!(
        foreign.is_empty(),
        "the TUI wrote folders into the daemon's session store that the daemon cannot load: {foreign:?}"
    );
}

/// Make a second chat session in the scope of `beside`: the same kilns and
/// the same workspace. Answer its id.
fn create_session_beside(home: &Path, beside: &str) -> String {
    let socket = home.join("crucible.sock");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let client = crucible_daemon::DaemonClient::connect_to(&socket)
            .await
            .expect("connect");
        let open = client.session_get(beside).await.expect("session.get");
        let kilns: Vec<crucible_core::config::KilnName> =
            serde_json::from_value(open["kilns"].clone()).expect("kilns");
        let workspace = open["workspace"].as_str().map(PathBuf::from);
        let created = client
            .session_create(crucible_daemon::rpc_client::SessionCreateParams {
                session_type: "chat".into(),
                kilns,
                workspace,
                recording_mode: None,
                recording_path: None,
                agent_type: None,
                isolation: None,
            })
            .await
            .expect("session.create");
        created["session_id"]
            .as_str()
            .expect("session_id")
            .to_string()
    })
}

/// US-912 across the process boundary: `/resume` lists a session the daemon
/// holds, and Enter moves the console to it. The new run opens the Lua
/// session under the chosen id, which the daemon then answers for.
#[test]
#[ignore = "requires: cru binary"]
fn resume_in_the_tui_moves_the_console_to_the_chosen_session() {
    let mut session = TuiTestSession::spawn(
        TuiTestConfig::new("chat")
            .with_env("RUST_LOG", "warn")
            .with_dimensions(100, 24),
    )
    .expect("spawn");
    session.wait_for_ready().expect("TUI ready");
    let home = session.home().to_path_buf();
    let open_id = wait_for_daemon_session(&home);
    let other_id = create_session_beside(&home, &open_id);

    session.send("/resume\r").expect("type /resume");
    session
        .wait_for_text(&other_id, Duration::from_secs(10))
        .expect("the picker lists the other session");
    assert!(
        !session.screen_contents().contains(&format!("▸ {open_id}")),
        "the picker must not offer the open session:\n{}",
        session.screen_contents()
    );

    session.send("\r").expect("choose it");
    let deadline = Instant::now() + Duration::from_secs(20);
    let switched = loop {
        match lua_session_exists(&home, &other_id) {
            Ok(()) => break Ok(()),
            Err(e) if Instant::now() >= deadline => break Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    session.wait_for_ready().expect("the TUI is ready again");

    session.send(":quit\r").expect("quit");
    session.expect_eof().expect("runner exited");

    switched
        .unwrap_or_else(|e| panic!("after the choice, the console must run on {other_id}: {e}"));
}
