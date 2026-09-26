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
use crucible_daemon::{DaemonClient, Server};

const BASE: &str = "one\ntwo\nthree\n";

struct Daemon {
    _dir: tempfile::TempDir,
    kiln: PathBuf,
    data: PathBuf,
    client: DaemonClient,
    shutdown: tokio::sync::broadcast::Sender<()>,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Daemon {
    /// A daemon with one registered kiln. With `open`, the client opens it.
    async fn start(open: bool) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        let kiln = dir.path().join("kiln");
        std::fs::create_dir(&kiln).unwrap();
        let data = dir.path().join("data");
        let socket = dir.path().join("daemon.sock");
        let server =
            Server::bind_with_data_home_and_kilns(&socket, data.clone(), &[("notes", &kiln)])
                .await
                .unwrap();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        let client = DaemonClient::connect_to(&socket).await.unwrap();
        if open {
            client.kiln_open(&kiln).await.unwrap();
        }
        Self {
            _dir: dir,
            kiln,
            data,
            client,
            shutdown,
            task,
        }
    }

    fn file(&self, path: &str) -> PathBuf {
        self.kiln.join(path)
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.file(path)).unwrap()
    }

    /// Propose `new_text` for each path, from the base `BASE`.
    fn propose(&self, writes: &[(&str, &str)]) -> Proposal {
        let store = ProposalStore::new(proposals_root(&self.data));
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
                        PhysicalRoot::from_top_level(&self.kiln),
                        path,
                        text_base(BASE),
                        new_text.to_string(),
                    )
                    .unwrap(),
            );
        }
        proposal.expect("at least one write")
    }

    async fn stop(self) {
        drop(self.client);
        self.shutdown.send(()).unwrap();
        self.task.await.unwrap().unwrap();
    }
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
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);

    let accepted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "ONE\ntwo\nthree\n");
    // The file keeps the state, so a second read agrees.
    let again = daemon.client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(again.state, ProposalState::Accepted);
    daemon.stop().await;
}

#[tokio::test]
async fn accept_after_an_outside_edit_merges_cleanly() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "one\ntwo\nTHREE\n");

    let accepted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    assert_eq!(accepted.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "ONE\ntwo\nTHREE\n");
    daemon.stop().await;
}

#[tokio::test]
async fn accept_with_a_conflict_writes_nothing_and_is_conflicted() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    write(&daemon.file("b.md"), BASE);
    // `a.md` has no outside edit and writes cleanly alone. `b.md` conflicts,
    // so the accept must not write `a.md` either.
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n"), ("b.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("b.md"), "eins\ntwo\nthree\n");

    let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();

    let ProposalState::Conflicted { files } = &conflicted.state else {
        panic!("expected a conflict, got {:?}", conflicted.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "b.md");
    assert_eq!(files[0].disk_text, "eins\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(daemon.read("a.md"), BASE);
    assert_eq!(daemon.read("b.md"), "eins\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn resolve_writes_the_settled_text() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "eins\ntwo\nthree\n");
    let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();
    assert!(matches!(conflicted.state, ProposalState::Conflicted { .. }));

    let resolved = daemon
        .client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    assert_eq!(resolved.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "uno eins\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn resolve_after_another_move_stays_conflicted() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "eins\ntwo\nthree\n");
    daemon.client.proposal_accept(&proposal.id).await.unwrap();
    // The disk moves again on the same line before the user settles the text.
    write(&daemon.file("a.md"), "one again\ntwo\nthree\n");

    let resolved = daemon
        .client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    let ProposalState::Conflicted { files } = &resolved.state else {
        panic!("expected a conflict, got {:?}", resolved.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].disk_text, "one again\ntwo\nthree\n");
    assert!(!files[0].regions.is_empty());
    assert_eq!(daemon.read("a.md"), "one again\ntwo\nthree\n");
    daemon.stop().await;
}

#[tokio::test]
async fn a_proposal_on_a_closed_kiln_goes_stale_at_list() {
    // The client does not open the kiln, so no watcher sees the edit.
    let daemon = Daemon::start(false).await;
    write(&daemon.file("a.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "ONE\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "one\ntwo\nTHREE\n");

    let listed = daemon.client.proposal_list(false).await.unwrap();

    let row = listed.iter().find(|p| p.id == proposal.id).unwrap();
    assert_eq!(row.state, ProposalState::Stale);
    // The check writes the state, so a read of the one proposal agrees.
    let got = daemon.client.proposal_get(&proposal.id).await.unwrap();
    assert_eq!(got.state, ProposalState::Stale);
    daemon.stop().await;
}

#[tokio::test]
async fn resolve_waits_for_every_settled_text() {
    let daemon = Daemon::start(true).await;
    write(&daemon.file("a.md"), BASE);
    write(&daemon.file("b.md"), BASE);
    let proposal = daemon.propose(&[("a.md", "uno\ntwo\nthree\n"), ("b.md", "uno\ntwo\nthree\n")]);
    write(&daemon.file("a.md"), "eins\ntwo\nthree\n");
    write(&daemon.file("b.md"), "eins\ntwo\nthree\n");
    let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();
    let ProposalState::Conflicted { files } = &conflicted.state else {
        panic!("expected a conflict, got {:?}", conflicted.state);
    };
    assert_eq!(files.len(), 2, "{files:?}");
    // Now `b.md` merges cleanly with its proposed text. The user did not
    // settle it yet, so the first resolve must still write nothing.
    write(&daemon.file("b.md"), "one\ntwo\nTHREE\n");

    let first = daemon
        .client
        .proposal_resolve(&proposal.id, "a.md", "uno eins\ntwo\nthree\n")
        .await
        .unwrap();

    // One file still waits for its settled text, so no file changes.
    let ProposalState::Conflicted { files } = &first.state else {
        panic!("expected a conflict, got {:?}", first.state);
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert_eq!(files[0].path, "b.md");
    assert_eq!(daemon.read("a.md"), "eins\ntwo\nthree\n");
    assert_eq!(daemon.read("b.md"), "one\ntwo\nTHREE\n");
    // The disk goes back to the text of the conflict, so the settled text
    // writes with no merge.
    write(&daemon.file("b.md"), "eins\ntwo\nthree\n");

    let second = daemon
        .client
        .proposal_resolve(&proposal.id, "b.md", "uno zwei\ntwo\nthree\n")
        .await
        .unwrap();

    assert_eq!(second.state, ProposalState::Accepted);
    assert_eq!(daemon.read("a.md"), "uno eins\ntwo\nthree\n");
    assert_eq!(daemon.read("b.md"), "uno zwei\ntwo\nthree\n");
    daemon.stop().await;
}

enum FileDecision {
    Accept,
    Reject,
    Resolve,
}

/// Legacy path-only decisions must fail when the path names two files.
/// The real socket must carry the refusal without changing either kiln.
async fn file_decision_in_two_kilns(decision: FileDecision, qualified: bool) {
    let daemon = Daemon::start(true).await;
    let other = daemon._dir.path().join("other-kiln");
    std::fs::create_dir(&other).unwrap();
    daemon
        .client
        .kiln_register("other", &other, false, false)
        .await
        .unwrap();
    write(&daemon.file("a.md"), BASE);
    write(&other.join("a.md"), BASE);

    let store = ProposalStore::new(proposals_root(&daemon.data));
    let session = SessionId::parse("aux-two-kilns").unwrap();
    let mut proposal = None;
    for root in [&daemon.kiln, &other] {
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
        write(&daemon.file("a.md"), "outside\n");
        write(&other.join("a.md"), "outside\n");
        let conflicted = daemon.client.proposal_accept(&proposal.id).await.unwrap();
        let ProposalState::Conflicted { files } = conflicted.state else {
            panic!("both files must conflict before testing resolution");
        };
        assert_eq!(files.len(), 2);
    }
    let before = daemon.client.proposal_get(&proposal.id).await.unwrap();
    let first_before = daemon.read("a.md");
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
    assert!(daemon
        .client
        .proposal_accept_files(&proposal.id, &[], &unknown)
        .await
        .is_err());
    assert!(daemon
        .client
        .proposal_reject_files(&proposal.id, &selected, &files, None)
        .await
        .is_err());
    assert_eq!(
        daemon.client.proposal_get(&proposal.id).await.unwrap(),
        before
    );
    let result = match decision {
        FileDecision::Accept if qualified => {
            daemon
                .client
                .proposal_accept_files(&proposal.id, &[], &files)
                .await
        }
        FileDecision::Reject if qualified => {
            daemon
                .client
                .proposal_reject_files(&proposal.id, &[], &files, None)
                .await
        }
        FileDecision::Resolve if qualified => {
            daemon
                .client
                .proposal_resolve_file(&proposal.id, "a.md", Some(&files[0].root), "resolved\n")
                .await
        }
        FileDecision::Accept => {
            daemon
                .client
                .proposal_accept_paths(&proposal.id, &selected)
                .await
        }
        FileDecision::Reject => {
            daemon
                .client
                .proposal_reject_paths(&proposal.id, &selected, None)
                .await
        }
        FileDecision::Resolve => {
            daemon
                .client
                .proposal_resolve(&proposal.id, "a.md", "resolved\n")
                .await
        }
    };
    let stored = daemon.client.proposal_get(&proposal.id).await.unwrap();
    let first_text = daemon.read("a.md");
    let second_text = std::fs::read_to_string(other.join("a.md")).unwrap();
    let first_root = PhysicalRoot::from_top_level(&daemon.kiln);
    daemon.stop().await;

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
