//! Accept, resolve and the stale check of a proposal, through a running daemon.
//!
//! Each test makes its proposal through a second `ProposalStore` over the
//! daemon's proposal directory. The files are the store, so the daemon reads
//! the same proposal.

use std::path::{Path, PathBuf};

use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{Proposal, ProposalAuthor, ProposalFile, ProposalState};
use crucible_core::session::{PhysicalRoot, SessionId};
use crucible_daemon::proposals::{proposals_root, ProposalStore};
use crucible_daemon::test_support::{InProcessDaemon, InProcessDaemonBuilder};
use crucible_daemon::DaemonClient;

const BASE: &str = "one\ntwo\nthree\n";

/// The name the tests register their one kiln under.
const KILN: &str = "notes";

/// A daemon with one registered kiln, and a client connected to it. With
/// `open`, the client opens the kiln.
async fn start(open: bool) -> (InProcessDaemon, DaemonClient) {
    let daemon = InProcessDaemonBuilder::new()
        .expect("a fresh data home")
        .with_kiln(KILN)
        .start()
        .await
        .expect("bind the daemon");
    let client = daemon.connect().await;
    if open {
        client.kiln_open(&daemon.kiln_dir(KILN)).await.unwrap();
    }
    (daemon, client)
}

fn kiln_file(daemon: &InProcessDaemon, path: &str) -> PathBuf {
    daemon.kiln_dir(KILN).join(path)
}

fn read(daemon: &InProcessDaemon, path: &str) -> String {
    std::fs::read_to_string(kiln_file(daemon, path)).unwrap()
}

/// Propose `new_text` for each path in the `notes` kiln, from the base `BASE`.
fn propose(daemon: &InProcessDaemon, writes: &[(&str, &str)]) -> Proposal {
    let store = ProposalStore::new(proposals_root(daemon.data_home()));
    let session = SessionId::parse("aux-proposals").unwrap();
    let mut proposal = None;
    for (path, new_text) in writes {
        proposal = Some(
            store
                .record_write(
                    ProposalAuthor::Plugin {
                        name: "consolidation".into(),
                    },
                    &session,
                    PhysicalRoot::from_top_level(daemon.kiln_dir(KILN)),
                    path,
                    text_base(BASE),
                    new_text.to_string(),
                )
                .unwrap(),
        );
    }
    proposal.expect("at least one write")
}

async fn stop(daemon: InProcessDaemon, client: DaemonClient) {
    drop(client);
    daemon.shutdown().await;
}

fn text_base(text: &str) -> ExpectedBase {
    ExpectedBase::Text {
        text: text.into(),
        hash: disk_hash(text),
    }
}

fn write(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
}

#[tokio::test]
async fn accept_on_an_unchanged_file_writes_it() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    let proposal = propose(&daemon, &[("a.md", "ONE\ntwo\nthree\n")]);

    let accepted = client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(read(&daemon, "a.md"), "ONE\ntwo\nthree\n");
    // The file keeps the state, so a second read agrees.
    let again = client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(again.state, ProposalState::Accepted);
    stop(daemon, client).await;
}

#[tokio::test]
async fn accept_after_an_outside_edit_merges_cleanly() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    let proposal = propose(&daemon, &[("a.md", "ONE\ntwo\nthree\n")]);
    write(&kiln_file(&daemon, "a.md"), "one\ntwo\nTHREE\n");

    let accepted = client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(read(&daemon, "a.md"), "ONE\ntwo\nTHREE\n");
    stop(daemon, client).await;
}

#[tokio::test]
async fn accept_with_a_conflict_writes_nothing_and_is_conflicted() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    write(&kiln_file(&daemon, "b.md"), BASE);
    // `a.md` has no outside edit and writes cleanly alone. `b.md` conflicts,
    // so the accept must not write `a.md` either.
    let proposal = propose(
        &daemon,
        &[("a.md", "ONE\ntwo\nthree\n"), ("b.md", "uno\ntwo\nthree\n")],
    );
    write(&kiln_file(&daemon, "b.md"), "eins\ntwo\nthree\n");

    let conflicted = client.proposal_accept(&proposal.id).await.unwrap();

    let ProposalState::Conflicted { files } = &conflicted.state else {
        panic!("expected a conflict, got {:?}", conflicted.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "b.md");
    assert_eq!(files[0].disk_text, "eins\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(read(&daemon, "a.md"), BASE);
    assert_eq!(read(&daemon, "b.md"), "eins\ntwo\nthree\n");
    stop(daemon, client).await;
}

#[tokio::test]
async fn resolve_writes_the_settled_text() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    let proposal = propose(&daemon, &[("a.md", "uno\ntwo\nthree\n")]);
    write(&kiln_file(&daemon, "a.md"), "eins\ntwo\nthree\n");
    let conflicted = client.proposal_accept(&proposal.id).await.unwrap();
    assert!(matches!(conflicted.state, ProposalState::Conflicted { .. }));

    let resolved = client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    assert_eq!(resolved.state, ProposalState::Accepted);
    assert_eq!(read(&daemon, "a.md"), "uno eins\ntwo\nthree\n");
    stop(daemon, client).await;
}

#[tokio::test]
async fn resolve_after_another_move_stays_conflicted() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    let proposal = propose(&daemon, &[("a.md", "uno\ntwo\nthree\n")]);
    write(&kiln_file(&daemon, "a.md"), "eins\ntwo\nthree\n");
    client.proposal_accept(&proposal.id).await.unwrap();
    // The disk moves again on the same line before the user settles the text.
    write(&kiln_file(&daemon, "a.md"), "one again\ntwo\nthree\n");

    let resolved = client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    let ProposalState::Conflicted { files } = &resolved.state else {
        panic!("expected a conflict, got {:?}", resolved.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].disk_text, "one again\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(read(&daemon, "a.md"), "one again\ntwo\nthree\n");
    stop(daemon, client).await;
}

#[tokio::test]
async fn a_proposal_on_a_closed_kiln_goes_stale_at_list() {
    // The client does not open the kiln, so no watcher sees the edit.
    let (daemon, client) = start(false).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    let proposal = propose(&daemon, &[("a.md", "ONE\ntwo\nthree\n")]);
    write(&kiln_file(&daemon, "a.md"), "one\ntwo\nTHREE\n");

    let listed = client.proposal_list(false).await.unwrap();

    let row = listed.iter().find(|p| p.id == proposal.id).unwrap();
    assert_eq!(row.state, ProposalState::Stale);
    // The check writes the state, so a read of the one proposal agrees.
    let got = client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(got.state, ProposalState::Stale);
    stop(daemon, client).await;
}

#[tokio::test]
async fn resolve_waits_for_every_settled_text() {
    let (daemon, client) = start(true).await;
    write(&kiln_file(&daemon, "a.md"), BASE);
    write(&kiln_file(&daemon, "b.md"), BASE);
    let proposal = propose(
        &daemon,
        &[("a.md", "uno\ntwo\nthree\n"), ("b.md", "uno\ntwo\nthree\n")],
    );
    write(&kiln_file(&daemon, "a.md"), "eins\ntwo\nthree\n");
    write(&kiln_file(&daemon, "b.md"), "eins\ntwo\nthree\n");
    let conflicted = client.proposal_accept(&proposal.id).await.unwrap();
    let ProposalState::Conflicted { files } = &conflicted.state else {
        panic!("expected a conflict, got {:?}", conflicted.state);
    };
    assert_eq!(files.len(), 2, "{files:?}");
    // Now `b.md` merges cleanly with its proposed text. The user did not
    // settle it yet, so the first resolve must still write nothing.
    write(&kiln_file(&daemon, "b.md"), "one\ntwo\nTHREE\n");

    let first = client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    // One file still waits for its settled text, so no file changes.
    let ProposalState::Conflicted { files } = &first.state else {
        panic!("expected a conflict, got {:?}", first.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "b.md");
    assert_eq!(read(&daemon, "a.md"), "eins\ntwo\nthree\n");
    assert_eq!(read(&daemon, "b.md"), "one\ntwo\nTHREE\n");
    // The disk goes back to the text of the conflict, so the settled text
    // writes with no merge.
    write(&kiln_file(&daemon, "b.md"), "eins\ntwo\nthree\n");

    let second = client
        .proposal_resolve(&proposal.id, "b.md", "uno zwei\ntwo\nthree\n")
        .await
        .unwrap();

    assert_eq!(second.state, ProposalState::Accepted);
    assert_eq!(read(&daemon, "a.md"), "uno eins\ntwo\nthree\n");
    assert_eq!(read(&daemon, "b.md"), "uno zwei\ntwo\nthree\n");
    stop(daemon, client).await;
}

enum FileDecision {
    Accept,
    Reject,
    Resolve,
}

/// Legacy path-only decisions must fail when the path names two files.
/// The real socket must carry the refusal without changing either kiln.
async fn file_decision_in_two_kilns(decision: FileDecision, qualified: bool) {
    let (daemon, client) = start(true).await;
    let other = daemon.data_home().join("other-kiln");
    std::fs::create_dir(&other).unwrap();
    client
        .kiln_register("other", &other, false, false)
        .await
        .unwrap();
    write(&kiln_file(&daemon, "a.md"), BASE);
    write(&other.join("a.md"), BASE);

    let store = ProposalStore::new(proposals_root(daemon.data_home()));
    let session = SessionId::parse("aux-two-kilns").unwrap();
    let mut proposal = None;
    for root in [daemon.kiln_dir(KILN), other.clone()] {
        proposal = Some(
            store
                .record_write(
                    ProposalAuthor::Plugin {
                        name: "reflection".into(),
                    },
                    &session,
                    PhysicalRoot::from_top_level(root),
                    "a.md",
                    text_base(BASE),
                    "proposed\n".into(),
                )
                .unwrap(),
        );
    }
    let proposal = proposal.unwrap();
    if matches!(decision, FileDecision::Resolve) {
        write(&kiln_file(&daemon, "a.md"), "outside\n");
        write(&other.join("a.md"), "outside\n");
        let conflicted = client.proposal_accept(&proposal.id).await.unwrap();
        let ProposalState::Conflicted { files } = conflicted.state else {
            panic!("both files must conflict before testing resolution");
        };
        assert_eq!(files.len(), 2);
    }
    let before = client.proposal_get(&proposal.id).await.unwrap();
    let first_before = read(&daemon, "a.md");
    let second_before = std::fs::read_to_string(other.join("a.md")).unwrap();
    let selected = vec!["a.md".to_string()];
    let files = vec![ProposalFile {
        root: PhysicalRoot::from_top_level(&other),
        path: "a.md".into(),
    }];
    // Invalid explicit selection and mixed formats must fail before splitting.
    let unknown = vec![ProposalFile {
        root: PhysicalRoot::from_top_level("/unregistered"),
        path: "a.md".into(),
    }];
    assert!(client
        .proposal_accept_files(&proposal.id, &[], &unknown)
        .await
        .is_err());
    assert!(client
        .proposal_reject_files(&proposal.id, &selected, &files, None)
        .await
        .is_err());
    assert_eq!(client.proposal_get(&proposal.id).await.unwrap(), before);
    let result = match decision {
        FileDecision::Accept if qualified => {
            client
                .proposal_accept_files(&proposal.id, &[], &files)
                .await
        }
        FileDecision::Reject if qualified => {
            client
                .proposal_reject_files(&proposal.id, &[], &files, None)
                .await
        }
        FileDecision::Resolve if qualified => {
            client
                .proposal_resolve_file(&proposal.id, "a.md", Some(&files[0].root), "resolved\n")
                .await
        }
        FileDecision::Accept => client.proposal_accept_paths(&proposal.id, &selected).await,
        FileDecision::Reject => {
            client
                .proposal_reject_paths(&proposal.id, &selected, None)
                .await
        }
        FileDecision::Resolve => {
            client
                .proposal_resolve(&proposal.id, "a.md", "resolved\n")
                .await
        }
    };
    let stored = client.proposal_get(&proposal.id).await.unwrap();
    let first_text = read(&daemon, "a.md");
    let second_text = std::fs::read_to_string(other.join("a.md")).unwrap();
    let first_root = PhysicalRoot::from_top_level(daemon.kiln_dir(KILN));
    stop(daemon, client).await;

    if qualified {
        let decided = result.unwrap();
        assert_eq!(first_text, first_before);
        match decision {
            FileDecision::Accept | FileDecision::Reject => {
                assert_eq!(decided.writes.len(), 1);
                assert_eq!(decided.writes[0].root, files[0].root);
                assert_eq!(stored.writes.len(), 1);
                assert_eq!(stored.writes[0].root, first_root);
                if matches!(decision, FileDecision::Accept) {
                    assert_eq!(second_text, "proposed\n");
                    assert_eq!(decided.state, ProposalState::Accepted);
                } else {
                    assert_eq!(second_text, second_before);
                    assert!(matches!(decided.state, ProposalState::Rejected { .. }));
                }
            }
            FileDecision::Resolve => {
                assert_eq!(second_text, second_before);
                let ProposalState::Conflicted { files: remaining } = decided.state else {
                    panic!("first kiln must still conflict")
                };
                assert_eq!(remaining.len(), 1);
                assert_eq!(remaining[0].root, first_root);
                assert_eq!(decided.writes[1].new_text, "resolved\n");
                assert_eq!(decided.writes[0].new_text, "proposed\n");
            }
        }
        return;
    }

    let error = result.expect_err("a path-only decision must not select files in two kilns");
    assert!(
        error.to_string().to_lowercase().contains("ambiguous"),
        "{error:#}"
    );
    assert_eq!(
        stored, before,
        "an ambiguous decision must leave the proposal unchanged"
    );
    assert_eq!(first_text, first_before);
    assert_eq!(second_text, second_before);
}

#[tokio::test]
async fn accepting_an_ambiguous_path_over_rpc_changes_neither_kiln() {
    file_decision_in_two_kilns(FileDecision::Accept, false).await;
}

#[tokio::test]
async fn rejecting_an_ambiguous_path_over_rpc_changes_neither_kiln() {
    file_decision_in_two_kilns(FileDecision::Reject, false).await;
}

#[tokio::test]
async fn resolving_an_ambiguous_path_over_rpc_preserves_both_conflicts() {
    file_decision_in_two_kilns(FileDecision::Resolve, false).await;
}

#[tokio::test]
async fn accepting_a_qualified_file_over_rpc_changes_only_its_kiln() {
    file_decision_in_two_kilns(FileDecision::Accept, true).await;
}

#[tokio::test]
async fn rejecting_a_qualified_file_over_rpc_keeps_the_other_kiln_open() {
    file_decision_in_two_kilns(FileDecision::Reject, true).await;
}

#[tokio::test]
async fn resolving_the_second_kiln_over_rpc_preserves_the_first_conflict() {
    file_decision_in_two_kilns(FileDecision::Resolve, true).await;
}
