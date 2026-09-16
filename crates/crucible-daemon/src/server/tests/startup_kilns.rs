//! What a starting daemon opens.
//!
//! A kiln is registered in `kilns.json` or in the config, and it is *open* in
//! the [`KilnManager`]. Those are different facts with different owners, and
//! the daemon has to turn the first into the second at boot or a restart
//! leaves every kiln closed: `kiln.list` reports the manager, the web file
//! routes gate on `kiln.list`, and a note write to a registered kiln then 404s
//! against a kiln the user can see in their own config.

use super::*;
use crate::kiln_state::KilnStateStore;
use crucible_core::config::KilnName;

/// Register a kiln, start a fresh daemon over the same data root, and the
/// kiln is reachable: `kiln.list` names it, and the note the user writes to it
/// lands.
///
/// The boot list used to come from the manager (empty on a fresh process) plus
/// the *project* registry. A kiln root that was also a project therefore
/// opened by accident, and when kiln roots stopped being projects
/// (59a1ee3ea) nothing opened them at all.
#[tokio::test]
async fn a_registered_kiln_is_open_after_a_restart() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let vault = tmp.path().join("Team Notes");
    std::fs::create_dir_all(&vault).unwrap();

    // The first daemon's registration.
    let state = Arc::new(KilnStateStore::new(&data_home));
    state
        .register(&KilnName::parse("Team Notes").unwrap(), &vault, false, true)
        .expect("the registration must be written");

    // The next daemon, over the same data root: build the registry the way
    // bind does, then open what boot decides to open.
    let registry = crate::test_support::kiln_registry(&data_home, &[]);
    registry.overlay_state(state.registrations());
    let km = Arc::new(crate::kiln_manager::KilnManager::new().with_kiln_registry(registry.clone()));

    let roots = crate::server::startup_kiln_roots(Vec::new(), Vec::new(), &registry);
    assert!(
        roots.iter().any(|root| root == &vault),
        "a registered kiln must be in the set boot opens: {roots:?}"
    );
    for root in &roots {
        km.open(root).await.expect("boot opens what it selected");
    }

    let listed = crate::server::kiln::handle_kiln_list(
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(crucible_core::protocol::RequestId::Number(1)),
            method: "kiln.list".to_string(),
            params: json!({}),
        },
        &km,
        &registry,
        &data_home,
    )
    .await
    .result
    .expect("kiln.list returns a list");

    let rows = listed.as_array().expect("an array");
    assert!(
        rows.iter().any(|row| row["name"] == json!("Team Notes")),
        "a restarted daemon must still list the kiln: {listed}"
    );

    // And the write the web route would make, through the same open kiln.
    let upsert = crate::server::kiln::handle_note_upsert(
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(crucible_core::protocol::RequestId::Number(2)),
            method: "note.upsert".to_string(),
            params: json!({
                "kiln": vault.to_string_lossy(),
                "note": {
                    "path": "Welcome.md",
                    "content_hash": crucible_core::parser::BlockHash::zero(),
                    "title": "Welcome",
                    "tags": [],
                    "links_to": [],
                    "properties": {},
                    "updated_at": chrono::Utc::now().to_rfc3339(),
                },
            }),
        },
        &km,
    )
    .await;
    assert!(
        upsert.error.is_none(),
        "the note write must succeed: {:?}",
        upsert.error
    );
}

/// A lazy entry stays shut. That is the whole of what `lazy` means, and the
/// bundled help corpus is lazy: opening it at boot would index Crucible's own
/// documentation into every search about the user's notes.
#[tokio::test]
async fn boot_leaves_a_lazy_kiln_closed() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let eager = tmp.path().join("notes");
    let lazy = tmp.path().join("help");
    std::fs::create_dir_all(&eager).unwrap();
    std::fs::create_dir_all(&lazy).unwrap();

    let registry = crate::test_support::kiln_registry_with_lazy(
        &data_home,
        &[("notes", &eager, false), ("help", &lazy, true)],
    );

    let roots = crate::server::startup_kiln_roots(Vec::new(), Vec::new(), &registry);

    assert!(roots.contains(&eager), "{roots:?}");
    assert!(
        !roots.contains(&lazy),
        "a lazy kiln must not open unasked: {roots:?}"
    );
}

/// A write into a REGISTERED kiln is admitted after a restart, and the write
/// opens it.
///
/// Root admission read the kilns the manager held OPEN, and a daemon that has
/// just started holds none, so a note write answered
/// `File not within any open kiln or registered project` against a kiln the
/// user could see in `kiln.list` and in their own config. The listing and the
/// admission disagreed about what a kiln is.
///
/// Admission asks IDENTITY now — `KilnManager::admissible_kiln_roots` — and
/// opening is a consequence of admitting rather than a precondition for it.
#[tokio::test]
async fn a_write_into_a_registered_kiln_is_admitted_after_a_restart() {
    let tmp = TempDir::new().unwrap();
    let data_home = tmp.path().join("data");
    let vault = tmp.path().join("alpha");
    std::fs::create_dir_all(&vault).unwrap();

    // The first daemon's registration, exactly what `cru init -p <dir> -y`
    // leaves behind.
    let state = Arc::new(KilnStateStore::new(&data_home));
    state
        .register(&KilnName::parse("alpha").unwrap(), &vault, false, false)
        .expect("the registration must be written");

    // A fresh daemon over the same data root. Nothing has opened anything: the
    // boot open is a task of its own, and a request can land before it.
    let registry = crate::test_support::kiln_registry(&data_home, &[]);
    registry.overlay_state(state.registrations());
    let km = Arc::new(crate::kiln_manager::KilnManager::new().with_kiln_registry(registry.clone()));
    assert!(
        km.list().await.is_empty(),
        "precondition: a fresh daemon holds nothing open"
    );

    let target = vault.join("Watched.md");
    let answer = crate::file_write::write_for_roots(
        crucible_core::file_write::FileWriteRequest {
            path: target.to_string_lossy().into_owned(),
            change: crucible_core::file_write::FileChange::Put {
                content: "after the restart\n".to_string(),
                base_hash: None,
                base_text: None,
            },
        },
        &km.admissible_kiln_roots().await,
        &[],
    )
    .await;

    assert_eq!(
        answer["ok"],
        json!(true),
        "a registered kiln must admit the write: {answer}"
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("the file must be written"),
        "after the restart\n"
    );

    // And admitting opens it, so the bytes just written are watched and
    // indexed rather than sitting outside the kiln the daemon serves.
    assert_eq!(
        km.admit_kiln_root(&target).await.as_deref(),
        Some(vault.as_path()),
        "the admission names the kiln root it opened"
    );
    let listed = crate::server::kiln::handle_kiln_list(
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(crucible_core::protocol::RequestId::Number(9)),
            method: "kiln.list".to_string(),
            params: json!({}),
        },
        &km,
        &registry,
        &data_home,
    )
    .await
    .result
    .expect("kiln.list returns a list");
    let row = listed
        .as_array()
        .expect("an array")
        .iter()
        .find(|row| row["name"] == json!("alpha"))
        .unwrap_or_else(|| panic!("the kiln must be listed: {listed}"));
    assert_eq!(row["open"], json!(true), "the use opened it: {listed}");
}
